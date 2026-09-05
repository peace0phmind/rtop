//! `rtop` 的稳定内核边界。此模块不公开 PostgreSQL 驱动类型。
mod config;
mod datasource;
mod facts;
mod mapping;
mod model;
mod ontology;
mod rdf;
mod server;
mod sparql;

pub use config::{
    load_configuration, DirectMappingConfiguration, KnowledgeGraphSpec, LoadedConfiguration,
};
pub use datasource::{
    DataSource, DataValue, DatabaseForeignKey, DatabaseMetadata, DatabaseMetadataColumn,
    DatabaseMetadataRelation, DatabaseRelationColumns, DatabaseUniqueConstraint, GeospatialValue,
    PostgresConnectionConfig, PostgresDataSource, QueryCancellation, RelationColumn,
    RelationConstraints, RelationForeignKey, RelationMetadata, StreamControl,
};
pub use facts::{parse_nquads, parse_rdf_xml, parse_turtle};
pub use mapping::Mapping;
pub use model::{Binding, QueryResult, RdfFact, RdfTerm, RuntimeError};
pub use rdf::format_rdf_term;
pub use server::serve;

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use mapping::BindingTerm;
use rand::random;
use regex::RegexBuilder;
use sha2::{Digest, Sha256};
use sparql::{
    parse as parse_query, Aggregate, AggregateKind, ArithmeticOperator, Bind, ComparisonOperator,
    Expression, Filter, FilterValue, GraphPattern, LogicalOperator, OrderByTerm, Query,
    TriplePattern,
};
use std::{cmp::Reverse, collections::BTreeMap, io::Read};
use uuid::Uuid;

/// VKG 的唯一高层 seam：加载配置并执行查询。
pub struct VkgRuntime<D> {
    spec: KnowledgeGraphSpec,
    source: D,
    mapping: Mapping,
    facts: Vec<RdfFact>,
    ontology: ontology::Ontology,
}

impl<D: DataSource> VkgRuntime<D> {
    pub fn new(spec: KnowledgeGraphSpec, source: D) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse_file(&spec.mapping_file)?;
        Self::from_mapping(spec, source, mapping)
    }

    /// 从 reader 加载 Turtle R2RML；显式 base IRI 使相对 IRI 语义不依赖临时文件路径。
    pub fn new_with_r2rml_reader<R: Read>(
        spec: KnowledgeGraphSpec,
        reader: R,
        base_iri: &str,
        source: D,
    ) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse_r2rml_reader(reader, base_iri)?;
        Self::from_mapping(spec, source, mapping)
    }

    fn from_mapping(
        spec: KnowledgeGraphSpec,
        source: D,
        mapping: Mapping,
    ) -> Result<Self, RuntimeError> {
        let facts = match &spec.facts_file {
            None => Vec::new(),
            Some(path) => {
                let content = std::fs::read(path)
                    .map_err(|e| RuntimeError::Facts(format!("无法读取 facts：{e}")))?;
                let format = spec
                    .facts_format
                    .as_deref()
                    .or_else(|| path.extension().and_then(|extension| extension.to_str()));
                let base_iri = spec
                    .facts_base_iri
                    .as_ref()
                    .map(|iri| oxiri::Iri::parse(iri.clone()))
                    .transpose()
                    .map_err(|e| RuntimeError::Facts(format!("无效 facts_base_iri：{e}")))?;
                match format {
                    Some("ttl" | "turtle") => parse_turtle(content, base_iri)?,
                    Some("nq" | "nquads") => parse_nquads(content)?,
                    Some("rdf" | "xml" | "rdfxml") => parse_rdf_xml(content, base_iri)?,
                    _ => return Err(RuntimeError::Facts("未提供有效的 facts 文件格式".into())),
                }
            }
        };
        let ontology = match &spec.ontology_file {
            Some(path) => ontology::Ontology::load(path)?,
            None => ontology::Ontology::default(),
        };
        ontology.validate_facts(&facts)?;
        Ok(Self {
            spec,
            source,
            mapping,
            facts,
            ontology,
        })
    }

    pub fn query(&mut self, sparql: &str) -> Result<QueryResult, RuntimeError> {
        match parse_query(sparql)? {
            Query::Select {
                variables,
                pattern,
                aggregates,
                projection_binds,
                group_by,
                distinct,
                order_by,
                offset,
                limit,
            } => {
                let mut rows = self.evaluate_graph_pattern(&pattern, vec![Binding::new()])?;
                if !aggregates.is_empty()
                    || projection_binds
                        .iter()
                        .any(|bind| expression_contains_aggregate(&bind.expression))
                {
                    rows = aggregate_bindings(rows, &aggregates, &group_by, &projection_binds);
                } else if !group_by.is_empty() {
                    rows = group_binding_representatives(rows, &group_by);
                    rows = apply_projection_binds(rows, &projection_binds);
                } else {
                    rows = apply_projection_binds(rows, &projection_binds);
                }
                sort_bindings_by(&mut rows, &order_by);
                let mut rows = rows
                    .into_iter()
                    .map(|row| {
                        variables
                            .iter()
                            .filter_map(|name| {
                                row.get(name).cloned().map(|value| (name.clone(), value))
                            })
                            .collect()
                    })
                    // Ontop virtual-mode SELECT 会在 projection 后消除首个投影变量
                    // 未绑定的 OPTIONAL 行；其余投影变量可保持未绑定。
                    .filter(|row: &Binding| {
                        variables
                            .first()
                            .is_none_or(|first| row.contains_key(first))
                    })
                    .collect::<Vec<Binding>>();
                if distinct {
                    let mut seen = BTreeMap::new();
                    rows.retain(|row| seen.insert(row.clone(), ()).is_none());
                }
                if let Some(offset) = offset {
                    rows.drain(..offset.min(rows.len()));
                }
                if let Some(limit) = limit {
                    rows.truncate(limit);
                }
                Ok(QueryResult::Bindings(rows))
            }
            Query::Ask { patterns } => Ok(QueryResult::Boolean(
                !self
                    .select_bgp(&patterns, &variables(&patterns[0]))?
                    .is_empty(),
            )),
            Query::Construct { template, patterns } => {
                let variables = patterns.iter().flat_map(variables).collect::<Vec<_>>();
                let rows = self.select_bgp(&patterns, &variables)?;
                Ok(QueryResult::Graph(
                    rows.into_iter()
                        .flat_map(|row| {
                            template
                                .iter()
                                .filter_map(move |pattern| instantiate(pattern, &row))
                        })
                        .collect(),
                ))
            }
            Query::Describe { resource } => {
                let mut graph = self
                    .facts
                    .iter()
                    .filter(
                        |fact| matches!(&fact.subject, RdfTerm::Iri(value) if value == &resource),
                    )
                    .cloned()
                    .collect::<Vec<_>>();
                let mut mapped_subjects = std::collections::BTreeSet::new();
                for plan in self.mapping.describe()? {
                    mapped_subjects.extend(
                        self.source
                            .execute(&plan.sql, &plan.parameters)?
                            .into_iter()
                            .filter_map(|row| row.into_iter().next().flatten())
                            .filter(|subject| subject == &resource),
                    );
                }
                graph.extend(
                    mapped_subjects
                        .into_iter()
                        .flat_map(|subject| self.mapping.mapped_facts(subject)),
                );
                Ok(QueryResult::Graph(graph))
            }
        }
    }

    /// 开发诊断：只暴露当前方言的 SQL 快照，不把它作为跨方言稳定契约。
    pub fn reformulate(&self, sparql: &str) -> Result<String, RuntimeError> {
        match parse_query(sparql)? {
            Query::Select {
                variables,
                pattern: GraphPattern::Bgp(patterns),
                ..
            } if patterns.len() == 1 => Ok(self
                .mapping
                .reformulate(&patterns[0], &variables)?
                .into_iter()
                .map(|plan| plan.sql)
                .collect::<Vec<_>>()
                .join("\nUNION ALL\n")),
            Query::Select { .. } => Err(RuntimeError::NotFullyTranslatable(
                "BGP 改写诊断尚未支持".into(),
            )),
            _ => Err(RuntimeError::UnsupportedSparql(
                "改写诊断目前仅支持 SELECT".into(),
            )),
        }
    }

    fn select(
        &mut self,
        query: &TriplePattern,
        projection_variables: &[String],
    ) -> Result<Vec<Binding>, RuntimeError> {
        let plans = if let Some(predicate_variable) = query.predicate.strip_prefix('?') {
            let mapping_projection_variables = projection_variables
                .iter()
                .filter(|variable| variable.as_str() != predicate_variable)
                .cloned()
                .collect::<Vec<_>>();
            self.mapping
                .reformulate_predicate_variable(query, &mapping_projection_variables)
                .map(|plans| {
                    plans
                        .into_iter()
                        .map(|(plan, predicate)| {
                            (plan, Some((predicate_variable.to_owned(), predicate)))
                        })
                        .collect::<Vec<_>>()
                })
        } else {
            match self.mapping.reformulate(query, projection_variables) {
                Ok(mut plans)
                    if query.predicate == "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"
                        && !query.object.starts_with('?') =>
                {
                    // 父类已有 mapping 也不能遗漏子类 mapping；例如 Actionreach 自身
                    // 虽有空 source，但 Brute_Action 的 source 仍应参与回答。
                    for class in self
                        .ontology
                        .subclasses_of(query.object.trim_matches(['<', '>']))
                    {
                        if class == query.object.trim_matches(['<', '>']) {
                            continue;
                        }
                        let mut candidate = query.clone();
                        candidate.object = format!("<{class}>");
                        if let Ok(mut candidate_plans) =
                            self.mapping.reformulate(&candidate, projection_variables)
                        {
                            plans.append(&mut candidate_plans);
                        }
                    }
                    Ok(plans.into_iter().map(|plan| (plan, None)).collect())
                }
                Ok(plans) => Ok(plans.into_iter().map(|plan| (plan, None)).collect()),
                Err(error @ RuntimeError::NotFullyTranslatable(_))
                    if query.predicate == "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"
                        && !query.object.starts_with('?') =>
                {
                    let mut plans = Vec::new();
                    for class in self
                        .ontology
                        .subclasses_of(query.object.trim_matches(['<', '>']))
                    {
                        let mut candidate = query.clone();
                        candidate.object = format!("<{class}>");
                        if let Ok(mut candidate_plans) =
                            self.mapping.reformulate(&candidate, projection_variables)
                        {
                            plans.append(&mut candidate_plans);
                        }
                    }
                    if plans.is_empty() {
                        Err(error)
                    } else {
                        Ok(plans.into_iter().map(|plan| (plan, None)).collect())
                    }
                }
                Err(error @ RuntimeError::NotFullyTranslatable(_))
                    if query.predicate != "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>" =>
                {
                    let mut plans = Vec::new();
                    for inverse in self
                        .ontology
                        .inverse_properties(query.predicate.trim_matches(['<', '>']))
                    {
                        for property in self.ontology.subproperties_of(inverse) {
                            let mut candidate = query.clone();
                            candidate.predicate = format!("<{property}>");
                            std::mem::swap(&mut candidate.subject, &mut candidate.object);
                            if let Ok(mut candidate_plans) =
                                self.mapping.reformulate(&candidate, &variables(&candidate))
                            {
                                plans.append(&mut candidate_plans);
                            }
                        }
                    }
                    for property in self
                        .ontology
                        .subproperties_of(query.predicate.trim_matches(['<', '>']))
                    {
                        let mut candidate = query.clone();
                        candidate.predicate = format!("<{property}>");
                        if let Ok(mut candidate_plans) =
                            self.mapping.reformulate(&candidate, projection_variables)
                        {
                            plans.append(&mut candidate_plans);
                        }
                    }
                    if plans.is_empty() {
                        Err(error)
                    } else {
                        Ok(plans.into_iter().map(|plan| (plan, None)).collect())
                    }
                }
                Err(error) => Err(error),
            }
        };
        // 保留原始的 NotFullyTranslatable，直到 facts 回退和 domain/range mapping
        // 推理都没有候选为止。静态 facts 查询不应因为没有 SQL mapping rule 而失败。
        let mut unmatched_mapping = None;
        let mut plans = match plans {
            Ok(plans) => plans,
            Err(error @ RuntimeError::NotFullyTranslatable(_)) => {
                unmatched_mapping = Some(error);
                Vec::new()
            }
            Err(error) => return Err(error),
        };
        if query.predicate == "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"
            && !query.object.starts_with('?')
            && query.subject.starts_with('?')
        {
            let class = query.object.trim_matches(['<', '>']);
            let subject_variable = query.subject.trim_start_matches('?').to_owned();
            for property in self.ontology.properties_asserting_type(class, true) {
                let auxiliary = format!("__rtop_type_domain_{subject_variable}");
                let candidate = TriplePattern {
                    subject: query.subject.clone(),
                    predicate: format!("<{property}>"),
                    object: format!("?{auxiliary}"),
                    graph: query.graph.clone(),
                };
                if let Ok(mut inferred) = self
                    .mapping
                    .reformulate(&candidate, &[subject_variable.clone(), auxiliary])
                {
                    plans.extend(inferred.drain(..).map(|plan| (plan, None)));
                }
            }
            for property in self.ontology.properties_asserting_type(class, false) {
                let auxiliary = format!("__rtop_type_range_{subject_variable}");
                let candidate = TriplePattern {
                    subject: format!("?{auxiliary}"),
                    predicate: format!("<{property}>"),
                    object: query.subject.clone(),
                    graph: query.graph.clone(),
                };
                if let Ok(mut inferred) = self
                    .mapping
                    .reformulate(&candidate, &[auxiliary, subject_variable.clone()])
                {
                    plans.extend(inferred.drain(..).map(|plan| (plan, None)));
                }
            }
        }
        if plans.is_empty() && self.facts.is_empty() && self.ontology.facts().is_empty() {
            if let Some(error) = unmatched_mapping.take() {
                return Err(error);
            }
        }
        let mut bindings = plans
            .into_iter()
            .map(|(plan, predicate_binding)| {
                let object_validation = plan.object_validation.clone();
                self.source
                    .execute_typed(&plan.sql, &plan.parameters)
                    .and_then(|rows| {
                        rows.into_iter()
                            .filter(|row| row.iter().all(Option::is_some))
                            .map(|row| {
                                let mut binding = Binding::new();
                                for (index, name) in plan.variables.iter().enumerate() {
                                    let value =
                                        row[index].as_ref().expect("NULL rows are filtered");
                                    let term = match &plan.terms[index] {
                                        BindingTerm::Iri => RdfTerm::Iri(resolve_mapping_iri(
                                            &value.value,
                                            plan.iri_bases[index].as_deref(),
                                        )?),
                                        BindingTerm::BlankNode => {
                                            RdfTerm::BlankNode(value.value.clone())
                                        }
                                        BindingTerm::Literal {
                                            datatype,
                                            language,
                                            infer_datatype,
                                        } => {
                                            let datatype = datatype.clone().or_else(|| {
                                                (*infer_datatype)
                                                    .then(|| value.datatype.clone())
                                                    .flatten()
                                            });
                                            RdfTerm::Literal {
                                                value: canonical_floating_lexical(
                                                    &value.value,
                                                    datatype.as_deref(),
                                                ),
                                                datatype,
                                                language: language.clone(),
                                            }
                                        }
                                    };
                                    binding.insert(name.clone(), term);
                                }
                                if let Some((name, predicate)) = &predicate_binding {
                                    binding.insert(
                                        name.clone(),
                                        RdfTerm::Iri(predicate.trim_matches(['<', '>']).into()),
                                    );
                                }
                                Ok(binding)
                            })
                            .collect::<Result<Vec<_>, RuntimeError>>()
                            .map(|mut bindings| {
                                if let Some(expected) = object_validation.as_deref() {
                                    bindings.retain(|binding| {
                                        binding
                                            .get("__rtop_constant_object")
                                            .is_some_and(|term| rdf_term_matches(term, expected))
                                    });
                                    for binding in &mut bindings {
                                        binding.remove("__rtop_constant_object");
                                    }
                                }
                                bindings
                            })
                    })
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        bindings.extend(
            self.facts
                .iter()
                .chain(self.ontology.facts())
                .flat_map(|fact| {
                    let mut candidates = vec![fact.clone()];
                    if matches!(fact.object, RdfTerm::Iri(_) | RdfTerm::BlankNode(_)) {
                        candidates.extend(self.ontology.inverse_properties(&fact.predicate).map(
                            |predicate| RdfFact {
                                subject: fact.object.clone(),
                                predicate: predicate.into(),
                                object: fact.subject.clone(),
                                graph: fact.graph.clone(),
                            },
                        ));
                    }
                    if query.predicate == "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>" {
                        for class in self.ontology.inferred_types(&fact.predicate, true) {
                            candidates.push(RdfFact {
                                subject: fact.subject.clone(),
                                predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".into(),
                                object: RdfTerm::Iri(class),
                                graph: fact.graph.clone(),
                            });
                        }
                        for class in self.ontology.inferred_types(&fact.predicate, false) {
                            if matches!(fact.object, RdfTerm::Iri(_) | RdfTerm::BlankNode(_)) {
                                candidates.push(RdfFact {
                                    subject: fact.object.clone(),
                                    predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                                        .into(),
                                    object: RdfTerm::Iri(class),
                                    graph: fact.graph.clone(),
                                });
                            }
                        }
                    }
                    candidates.into_iter().filter_map(|candidate| {
                        (graph_matches(query.graph.as_deref(), candidate.graph.as_ref())
                            && self.ontology.is_subproperty_of(
                                &candidate.predicate,
                                query.predicate.trim_matches(['<', '>']),
                            )
                            && fact_matches(query, &candidate, &self.ontology))
                        .then(|| fact_binding(query, &candidate, &self.ontology))
                        .flatten()
                    })
                }),
        );
        if query.predicate == "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>" {
            // domain/range 规划需要临时投影 property 的另一端来让 SQL mapping
            // 生成 subject/object；它不是原 RDF pattern 的变量，不能成为 solution
            // mapping 的一部分，否则相同 rdf:type 会被不同 property value 放大。
            for binding in &mut bindings {
                binding.retain(|name, _| !name.starts_with("__rtop_type_"));
            }
        }
        // mapping 中重复的 class assertion 不能把同一资源的 rdf:type BGP 放大；
        // 普通 predicate 仍保留 SQL bag 语义（例如 DOID annotation 的 76 行）。
        if query.predicate == "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"
            || (query.subject.starts_with('?')
                && query.predicate.starts_with('?')
                && query.object.starts_with('?'))
        {
            let mut seen = BTreeMap::new();
            bindings.retain(|binding| seen.insert(binding.clone(), ()).is_none());
        }
        Ok(bindings)
    }

    fn select_bgp(
        &mut self,
        patterns: &[TriplePattern],
        projections: &[String],
    ) -> Result<Vec<Binding>, RuntimeError> {
        if patterns.len() == 1 {
            return Ok(self
                .select(&patterns[0], &variables(&patterns[0]))?
                .into_iter()
                .map(|row| {
                    projections
                        .iter()
                        .filter_map(|name| {
                            row.get(name).cloned().map(|value| (name.clone(), value))
                        })
                        .collect()
                })
                .collect());
        }
        // BGP 中 triple 的书写顺序不改变语义。先取每个 pattern 的候选，再优先沿已
        // 绑定变量连接，避免 IMDB 这类高基数 mapping 形成平方级中间结果。
        let mut candidates = patterns
            .iter()
            .map(|pattern| self.select(pattern, &variables(pattern)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut rows = vec![Binding::new()];
        while !candidates.is_empty() {
            let index = next_bgp_candidate(&rows, &candidates);
            let matches = candidates.swap_remove(index);
            rows = join_binding_relations(rows, matches);
        }
        Ok(rows
            .into_iter()
            .map(|row| {
                projections
                    .iter()
                    .filter_map(|name| row.get(name).cloned().map(|value| (name.clone(), value)))
                    .collect()
            })
            .collect())
    }

    fn evaluate_graph_pattern(
        &mut self,
        pattern: &GraphPattern,
        input: Vec<Binding>,
    ) -> Result<Vec<Binding>, RuntimeError> {
        match pattern {
            GraphPattern::Empty => Ok(input),
            GraphPattern::Bgp(patterns) => {
                // Basic graph pattern 是交换律 join；优先有共享变量的候选会显著压低
                // 虚拟 mapping 的中间 binding 数量，同时保留输入 binding 的兼容性语义。
                let mut candidates = patterns
                    .iter()
                    .map(|pattern| self.select(pattern, &variables(pattern)))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut rows = input;
                while !candidates.is_empty() {
                    let index = next_bgp_candidate(&rows, &candidates);
                    let matches = candidates.swap_remove(index);
                    rows = join_binding_relations(rows, matches);
                }
                Ok(rows)
            }
            GraphPattern::Join(left, right) => {
                let rows = self.evaluate_graph_pattern(left, input)?;
                self.evaluate_graph_pattern(right, rows)
            }
            GraphPattern::LeftJoin(left, right) => {
                let left_rows = self.evaluate_graph_pattern(left, input)?;
                let mut rows = Vec::new();
                for left in left_rows {
                    let right_rows = match self.evaluate_graph_pattern(right, vec![left.clone()]) {
                        Ok(rows) => rows,
                        // 对 virtual graph 中没有 mapping rule 的 optional property，SPARQL
                        // 语义是空右侧而非整个查询失败。
                        Err(RuntimeError::NotFullyTranslatable(_)) => Vec::new(),
                        Err(error) => return Err(error),
                    };
                    if right_rows.is_empty() {
                        rows.push(left);
                    } else {
                        rows.extend(right_rows);
                    }
                }
                Ok(rows)
            }
            GraphPattern::Minus(left, right) => {
                let left_rows = self.evaluate_graph_pattern(left, input.clone())?;
                let right_rows = match self.evaluate_graph_pattern(right, input) {
                    // 虚拟图中没有对应 mapping rule 的右侧模式等价于空 multiset。
                    Err(RuntimeError::NotFullyTranslatable(_)) => Vec::new(),
                    result => result?,
                };
                Ok(left_rows
                    .into_iter()
                    .filter(|left| {
                        !right_rows.iter().any(|right| {
                            let shared = left.keys().any(|name| right.contains_key(name));
                            shared && join(left, right).is_some()
                        })
                    })
                    .collect())
            }
            GraphPattern::Union(left, right) => {
                let mut rows = self.evaluate_graph_pattern(left, input.clone())?;
                rows.extend(self.evaluate_graph_pattern(right, input)?);
                Ok(rows)
            }
            GraphPattern::Subquery {
                pattern,
                variables,
                aggregates,
                projection_binds,
                group_by,
                distinct,
                order_by,
                offset,
                limit,
            } => {
                let mut inner = self.evaluate_graph_pattern(pattern, vec![Binding::new()])?;
                if !aggregates.is_empty() {
                    inner = aggregate_bindings(inner, aggregates, group_by, projection_binds);
                } else if !group_by.is_empty() {
                    inner = group_binding_representatives(inner, group_by);
                    inner = apply_projection_binds(inner, projection_binds);
                } else {
                    inner = apply_projection_binds(inner, projection_binds);
                }
                sort_bindings_by(&mut inner, order_by);
                inner = project_bindings(inner, variables);
                if *distinct {
                    inner = distinct_bindings(inner);
                }
                if let Some(offset) = offset {
                    inner.drain(..(*offset).min(inner.len()));
                }
                if let Some(limit) = limit {
                    inner.truncate(*limit);
                }
                Ok(input
                    .into_iter()
                    .flat_map(|outer| inner.iter().filter_map(move |inner| join(&outer, inner)))
                    .collect())
            }
            GraphPattern::Values(values) => Ok(input
                .into_iter()
                .flat_map(|row| values.iter().filter_map(move |value| join(&row, value)))
                .collect()),
            GraphPattern::Bind(pattern, binds) => {
                let mut rows = self.evaluate_graph_pattern(pattern, input)?;
                for bind in binds {
                    for row in &mut rows {
                        if let Some(value) = self
                            .geospatial_expression(row, &bind.expression)?
                            .or_else(|| evaluate_expression(row, &bind.expression))
                        {
                            row.insert(bind.variable.clone(), value);
                        }
                    }
                }
                Ok(rows)
            }
            GraphPattern::Filter(pattern, filters) => {
                let mut rows = self.evaluate_graph_pattern(pattern, input)?;
                for filter in filters {
                    rows.retain(|row| matches_filter(row, filter));
                }
                Ok(rows)
            }
        }
    }

    fn geospatial_expression(
        &mut self,
        row: &Binding,
        expression: &Expression,
    ) -> Result<Option<RdfTerm>, RuntimeError> {
        let Expression::Function {
            name, arguments, ..
        } = expression
        else {
            return Ok(None);
        };
        let arguments = arguments
            .iter()
            .map(|argument| {
                evaluate_expression(row, argument).and_then(|term| match term {
                    RdfTerm::Iri(value) | RdfTerm::Literal { value, .. } => Some(value),
                    RdfTerm::BlankNode(_) => None,
                })
            })
            .collect::<Option<Vec<_>>>();
        let Some(arguments) = arguments else {
            return Ok(None);
        };
        Ok(self
            .source
            .geospatial(name, &arguments)?
            .map(|value| match value {
                GeospatialValue::Boolean(value) => boolean_term(value).expect("boolean term"),
                GeospatialValue::Wkt(value) => RdfTerm::Literal {
                    value,
                    datatype: Some("http://www.opengis.net/ont/geosparql#wktLiteral".into()),
                    language: None,
                },
            }))
    }

    pub fn spec(&self) -> &KnowledgeGraphSpec {
        &self.spec
    }
}

impl VkgRuntime<PostgresDataSource> {
    /// 以 PostgreSQL catalog 原生规划并加载 RDB2RDF Direct Mapping。
    ///
    /// `relations` 是显式 allow-list，避免无意把 schema 的内部表暴露为 RDF；外键
    /// parent relation 也必须位于该列表中。
    pub fn new_with_direct_mapping(
        spec: KnowledgeGraphSpec,
        mut source: PostgresDataSource,
        base_iri: &str,
        relations: &[String],
    ) -> Result<Self, RuntimeError> {
        let metadata = relations
            .iter()
            .map(|relation| {
                source
                    .relation_metadata(relation)
                    .map(|metadata| (relation.clone(), metadata))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mapping = Mapping::from_direct_mapping(base_iri, &metadata)?;
        Self::from_mapping(spec, source, mapping)
    }
}

fn sort_bindings_by(rows: &mut [Binding], order_by: &[OrderByTerm]) {
    use std::cmp::Ordering;
    if order_by.is_empty() {
        return;
    }
    rows.sort_by(|left, right| {
        for term in order_by {
            let ordering = left
                .get(&term.variable)
                .map(format_rdf_term)
                .unwrap_or_default()
                .cmp(
                    &right
                        .get(&term.variable)
                        .map(format_rdf_term)
                        .unwrap_or_default(),
                );
            if ordering != Ordering::Equal {
                return if term.descending {
                    ordering.reverse()
                } else {
                    ordering
                };
            }
        }
        Ordering::Equal
    });
}

fn project_bindings(rows: Vec<Binding>, variables: &[String]) -> Vec<Binding> {
    rows.into_iter()
        .map(|row| {
            variables
                .iter()
                .filter_map(|name| row.get(name).cloned().map(|value| (name.clone(), value)))
                .collect()
        })
        .collect()
}

fn distinct_bindings(rows: Vec<Binding>) -> Vec<Binding> {
    let mut unique = Vec::new();
    for row in rows {
        if !unique.contains(&row) {
            unique.push(row);
        }
    }
    unique
}

/// GROUP BY 即使没有 aggregate，也会将 solution sequence 折叠为每组一行。
///
/// 保留组中首个完整 binding，而不是只保留 group key：SELECT 投影表达式在解析时以
/// BIND 表示，仍需要读取由 group key 决定的输入变量。对于合法的分组投影，这与在
/// 分组后计算投影有相同的可观察结果。
fn group_binding_representatives(rows: Vec<Binding>, group_by: &[String]) -> Vec<Binding> {
    let mut groups = Vec::new();
    for row in rows {
        let key = group_by
            .iter()
            .filter_map(|variable| {
                row.get(variable)
                    .cloned()
                    .map(|value| (variable.clone(), value))
            })
            .collect::<Binding>();
        if !groups
            .iter()
            .any(|(existing, _): &(Binding, Binding)| *existing == key)
        {
            groups.push((key, row));
        }
    }
    groups
        .into_iter()
        .map(|(_, representative)| representative)
        .collect()
}

fn apply_projection_binds(mut rows: Vec<Binding>, binds: &[Bind]) -> Vec<Binding> {
    for bind in binds {
        for row in &mut rows {
            if let Some(value) = evaluate_expression(row, &bind.expression) {
                row.insert(bind.variable.clone(), value);
            }
        }
    }
    rows
}

fn expression_contains_aggregate(expression: &Expression) -> bool {
    match expression {
        Expression::Aggregate { .. } => true,
        Expression::Not(inner) => expression_contains_aggregate(inner),
        Expression::Binary { left, right, .. }
        | Expression::Logical { left, right, .. }
        | Expression::Comparison { left, right, .. } => {
            expression_contains_aggregate(left) || expression_contains_aggregate(right)
        }
        Expression::Function { arguments, .. } => {
            arguments.iter().any(expression_contains_aggregate)
        }
        Expression::Replace {
            value,
            pattern,
            replacement,
        } => {
            expression_contains_aggregate(value)
                || expression_contains_aggregate(pattern)
                || expression_contains_aggregate(replacement)
        }
        _ => false,
    }
}

fn aggregate_bindings(
    rows: Vec<Binding>,
    aggregates: &[Aggregate],
    group_by: &[String],
    projection_binds: &[Bind],
) -> Vec<Binding> {
    let mut groups: Vec<(Binding, Vec<Binding>)> = Vec::new();
    for row in rows {
        let key = group_by
            .iter()
            .filter_map(|variable| {
                row.get(variable)
                    .cloned()
                    .map(|value| (variable.clone(), value))
            })
            .collect::<Binding>();
        if let Some((_, members)) = groups.iter_mut().find(|(existing, _)| *existing == key) {
            members.push(row);
        } else {
            groups.push((key, vec![row]));
        }
    }
    if groups.is_empty() && group_by.is_empty() {
        groups.push((Binding::new(), Vec::new()));
    }
    groups
        .into_iter()
        .map(|(mut key, members)| {
            for aggregate in aggregates {
                if let Some(value) = evaluate_aggregate(&members, aggregate).or_else(|| {
                    aggregate
                        .fallback
                        .as_ref()
                        .and_then(|fallback| evaluate_expression(&Binding::new(), fallback))
                }) {
                    key.insert(aggregate.variable.clone(), value);
                }
            }
            for bind in projection_binds {
                if let Some(value) =
                    evaluate_expression_with_aggregates(&key, &bind.expression, &members)
                {
                    key.insert(bind.variable.clone(), value);
                }
            }
            key
        })
        .collect()
}

fn evaluate_expression_with_aggregates(
    row: &Binding,
    expression: &Expression,
    members: &[Binding],
) -> Option<RdfTerm> {
    let mut bindings = row.clone();
    let mut next = 0usize;
    let expression = materialize_aggregates(expression, members, &mut bindings, &mut next)?;
    evaluate_expression(&bindings, &expression)
}

fn materialize_aggregates(
    expression: &Expression,
    members: &[Binding],
    bindings: &mut Binding,
    next: &mut usize,
) -> Option<Expression> {
    let recurse = |expression, bindings: &mut Binding, next: &mut usize| {
        materialize_aggregates(expression, members, bindings, next)
    };
    Some(match expression {
        Expression::Aggregate {
            kind,
            expression,
            distinct,
        } => {
            let aggregate = Aggregate {
                variable: String::new(),
                kind: *kind,
                expression: (*expression.clone()),
                distinct: *distinct,
                separator: None,
                fallback: None,
            };
            let value = evaluate_aggregate(members, &aggregate)?;
            let variable = format!("__rtop_aggregate_{next}");
            *next += 1;
            bindings.insert(variable.clone(), value);
            Expression::Variable(variable)
        }
        Expression::Not(inner) => Expression::Not(Box::new(recurse(inner, bindings, next)?)),
        Expression::Binary {
            operator,
            left,
            right,
        } => Expression::Binary {
            operator: *operator,
            left: Box::new(recurse(left, bindings, next)?),
            right: Box::new(recurse(right, bindings, next)?),
        },
        Expression::Logical {
            operator,
            left,
            right,
        } => Expression::Logical {
            operator: *operator,
            left: Box::new(recurse(left, bindings, next)?),
            right: Box::new(recurse(right, bindings, next)?),
        },
        Expression::Comparison {
            operator,
            left,
            right,
        } => Expression::Comparison {
            operator: *operator,
            left: Box::new(recurse(left, bindings, next)?),
            right: Box::new(recurse(right, bindings, next)?),
        },
        Expression::Function {
            name,
            arguments,
            base_iri,
        } => {
            let postgres_avg = if name.eq_ignore_ascii_case("STR") {
                match arguments.as_slice() {
                    [Expression::Aggregate {
                        kind: AggregateKind::Avg,
                        expression,
                        distinct,
                    }] => postgres_integer_avg_lexical(members, expression, *distinct),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(value) = postgres_avg {
                let variable = format!("__rtop_aggregate_{next}");
                *next += 1;
                bindings.insert(variable.clone(), string_term(&value)?);
                Expression::Variable(variable)
            } else {
                Expression::Function {
                    name: name.clone(),
                    arguments: arguments
                        .iter()
                        .map(|argument| recurse(argument, bindings, next))
                        .collect::<Option<_>>()?,
                    base_iri: base_iri.clone(),
                }
            }
        }
        Expression::Replace {
            value,
            pattern,
            replacement,
        } => Expression::Replace {
            value: Box::new(recurse(value, bindings, next)?),
            pattern: Box::new(recurse(pattern, bindings, next)?),
            replacement: Box::new(recurse(replacement, bindings, next)?),
        },
        other => other.clone(),
    })
}

fn postgres_integer_avg_lexical(
    members: &[Binding],
    expression: &Expression,
    distinct: bool,
) -> Option<String> {
    let mut values = members
        .iter()
        .map(|row| match evaluate_expression(row, expression)? {
            RdfTerm::Literal {
                value,
                datatype: Some(datatype),
                ..
            } if datatype.ends_with("#integer") => value.parse::<i128>().ok(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if distinct {
        values.sort_unstable();
        values.dedup();
    }
    let sum = values.iter().sum::<i128>();
    let divisor = values.len() as i128;
    let scale_digits = if sum % divisor == 0 { 16 } else { 12 };
    let scale = 10_i128.pow(scale_digits);
    let scaled = sum * scale;
    let rounded = if scaled >= 0 {
        (scaled + divisor / 2) / divisor
    } else {
        (scaled - divisor / 2) / divisor
    };
    let integer = rounded / scale;
    let fraction = (rounded % scale).abs();
    Some(format!(
        "{integer}.{fraction:0width$}",
        width = scale_digits as usize
    ))
}

fn evaluate_aggregate(rows: &[Binding], aggregate: &Aggregate) -> Option<RdfTerm> {
    let mut values = rows
        .iter()
        .filter_map(|row| evaluate_expression(row, &aggregate.expression))
        .collect::<Vec<_>>();
    if aggregate.distinct {
        let mut unique = Vec::new();
        for value in values {
            if !unique.contains(&value) {
                unique.push(value);
            }
        }
        values = unique;
    }
    match aggregate.kind {
        AggregateKind::Count => Some(RdfTerm::Literal {
            value: values.len().to_string(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        }),
        AggregateKind::GroupConcat => Some(RdfTerm::Literal {
            value: values
                .iter()
                .map(term_lexical_form)
                .collect::<Option<Vec<_>>>()?
                .join(aggregate.separator.as_deref().unwrap_or(" ")),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        }),
        AggregateKind::Min | AggregateKind::Max => values
            .into_iter()
            .filter_map(|term| term_numeric_value(&term).map(|(value, _)| (value, term)))
            .reduce(|left, right| {
                let choose_right = matches!(aggregate.kind, AggregateKind::Min)
                    .then(|| right.0 < left.0)
                    .unwrap_or_else(|| right.0 > left.0);
                if choose_right {
                    right
                } else {
                    left
                }
            })
            .map(|(_, term)| term),
        AggregateKind::Sum | AggregateKind::Avg => {
            // SPARQL 数值聚合遇到已绑定但非数值的 expression result 时是 type
            // error；投影变量保持未绑定。不能像 SQL NULL 一样忽略它，否则 MINUS
            // 会错误地看到一个与左侧 literal 不兼容的 aggregate binding。
            if values
                .iter()
                .any(|value| term_numeric_value(value).is_none())
            {
                return None;
            }
            let numeric = values
                .iter()
                .filter_map(term_numeric_value)
                .collect::<Vec<_>>();
            if numeric.is_empty() {
                return match aggregate.kind {
                    AggregateKind::Sum => Some(RdfTerm::Literal {
                        value: "0".into(),
                        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                        language: None,
                    }),
                    AggregateKind::Avg => {
                        numeric_result_term(0.0, "http://www.w3.org/2001/XMLSchema#decimal")
                    }
                    _ => unreachable!("aggregate kind was matched above"),
                };
            }
            let sum = numeric.iter().map(|(value, _)| value).sum::<f64>();
            if matches!(aggregate.kind, AggregateKind::Sum)
                && numeric
                    .iter()
                    .all(|(_, datatype)| datatype.ends_with("#integer"))
            {
                return Some(RdfTerm::Literal {
                    value: format!("{sum:.0}"),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                    language: None,
                });
            }
            let value = if matches!(aggregate.kind, AggregateKind::Avg) {
                sum / numeric.len() as f64
            } else {
                sum
            };
            numeric_result_term(value, "http://www.w3.org/2001/XMLSchema#decimal")
        }
    }
}

fn term_lexical_form(term: &RdfTerm) -> Option<String> {
    match term {
        RdfTerm::Literal {
            value, datatype, ..
        } if datatype
            .as_deref()
            .is_none_or(|datatype| datatype == "http://www.w3.org/2001/XMLSchema#string") =>
        {
            Some(value.clone())
        }
        RdfTerm::Iri(value) => Some(value.clone()),
        RdfTerm::BlankNode(_) => None,
        RdfTerm::Literal { .. } => None,
    }
}

fn term_numeric_value(term: &RdfTerm) -> Option<(f64, String)> {
    let RdfTerm::Literal {
        value, datatype, ..
    } = term
    else {
        return None;
    };
    let datatype = datatype
        .as_deref()
        .filter(|datatype| is_numeric_datatype(datatype))?;
    Some((value.parse().ok()?, datatype.into()))
}

fn evaluate_expression(row: &Binding, expression: &Expression) -> Option<RdfTerm> {
    match expression {
        Expression::Not(inner) => boolean_term(!expression_boolean(row, inner)?),
        Expression::Variable(variable) => row.get(variable).cloned(),
        Expression::String(value) => string_term(value),
        Expression::TypedLiteral { value, datatype } => Some(RdfTerm::Literal {
            value: value.clone(),
            datatype: Some(datatype.clone()),
            language: None,
        }),
        Expression::Iri(value) => Some(RdfTerm::Iri(value.clone())),
        Expression::Number(value) => Some(RdfTerm::Literal {
            value: value.clone(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
            language: None,
        }),
        Expression::Aggregate { .. } => None,
        Expression::Binary {
            operator,
            left,
            right,
        } => {
            let (left, left_datatype) = numeric_value(row, left)?;
            let (right, right_datatype) = numeric_value(row, right)?;
            let value = match operator {
                ArithmeticOperator::Add => left + right,
                ArithmeticOperator::Subtract => left - right,
                ArithmeticOperator::Multiply => left * right,
                ArithmeticOperator::Divide if right != 0.0 => left / right,
                ArithmeticOperator::Divide => return None,
            };
            numeric_result_term(
                value,
                promoted_numeric_datatype(&left_datatype, &right_datatype),
            )
        }
        Expression::Logical {
            operator,
            left,
            right,
        } => match operator {
            // SPARQL 的 EBV error 不是普通 false：只有可决定结果的另一侧能吸收它。
            LogicalOperator::Or => match expression_boolean(row, left) {
                Some(true) => boolean_term(true),
                Some(false) => expression_boolean(row, right).and_then(boolean_term),
                None => expression_boolean(row, right)
                    .filter(|value| *value)
                    .and_then(boolean_term),
            },
            LogicalOperator::And => match expression_boolean(row, left) {
                Some(false) => boolean_term(false),
                Some(true) => expression_boolean(row, right).and_then(boolean_term),
                None => expression_boolean(row, right)
                    .filter(|value| !*value)
                    .and_then(boolean_term),
            },
        },
        Expression::Comparison {
            operator,
            left,
            right,
        } => {
            let left_value = evaluate_expression(row, left)?;
            let right_value = evaluate_expression(row, right)?;
            let value = match operator {
                ComparisonOperator::Equal => sparql_value_equal(&left_value, &right_value),
                ComparisonOperator::NotEqual => !sparql_value_equal(&left_value, &right_value),
                ComparisonOperator::Less
                | ComparisonOperator::Greater
                | ComparisonOperator::LessOrEqual
                | ComparisonOperator::GreaterOrEqual => {
                    let comparison = sparql_value_ordering(&left_value, &right_value)?;
                    match operator {
                        ComparisonOperator::Less => comparison.is_lt(),
                        ComparisonOperator::Greater => comparison.is_gt(),
                        ComparisonOperator::LessOrEqual => comparison.is_le(),
                        ComparisonOperator::GreaterOrEqual => comparison.is_ge(),
                        _ => unreachable!(),
                    }
                }
            };
            boolean_term(value)
        }
        Expression::Function {
            name,
            arguments,
            base_iri,
        } => evaluate_function(row, name, arguments, base_iri.as_deref()),
        Expression::Replace {
            value,
            pattern,
            replacement,
        } => {
            let source = evaluate_expression(row, value)?;
            let RdfTerm::Literal {
                value: string,
                datatype,
                language,
            } = source
            else {
                return None;
            };
            let pattern = expression_string(row, pattern)?;
            let replacement = expression_string(row, replacement)?;
            Some(RdfTerm::Literal {
                value: string.replace(&pattern, &replacement),
                datatype,
                language,
            })
        }
    }
}

fn sparql_term_equal(left: &RdfTerm, right: &RdfTerm) -> bool {
    match (left, right) {
        (
            RdfTerm::Literal {
                value: left_value,
                datatype: left_datatype,
                language: None,
            },
            RdfTerm::Literal {
                value: right_value,
                datatype: right_datatype,
                language: None,
            },
        ) if left_value == right_value
            && [left_datatype.as_deref(), right_datatype.as_deref()]
                .into_iter()
                .all(|datatype| {
                    datatype.is_none_or(|datatype| {
                        datatype == "http://www.w3.org/2001/XMLSchema#string"
                    })
                }) =>
        {
            true
        }
        (
            RdfTerm::Literal {
                language: Some(left_language),
                ..
            },
            RdfTerm::Literal {
                language: Some(right_language),
                ..
            },
        ) => left_language.eq_ignore_ascii_case(right_language),
        _ => left == right,
    }
}

/// SPARQL 的 `=`/`!=` 先按数值与 dateTime 的 value space 比较；其余 RDF term
/// 保留 term equality。这让 PostgreSQL integer/numeric/timestamp 的自然 datatype
/// 能与 query 中的 xsd:decimal/dateTime 常量正确比较。
fn sparql_value_equal(left: &RdfTerm, right: &RdfTerm) -> bool {
    sparql_value_ordering(left, right).is_some_and(std::cmp::Ordering::is_eq)
        || sparql_term_equal(left, right)
}

fn sparql_value_ordering(left: &RdfTerm, right: &RdfTerm) -> Option<std::cmp::Ordering> {
    if let (Some(left), Some(right)) = (numeric_term(left), numeric_term(right)) {
        return left.partial_cmp(&right);
    }
    let (
        RdfTerm::Literal {
            value: left_value,
            datatype: Some(left_datatype),
            language: None,
        },
        RdfTerm::Literal {
            value: right_value,
            datatype: Some(right_datatype),
            language: None,
        },
    ) = (left, right)
    else {
        return None;
    };
    (left_datatype == "http://www.w3.org/2001/XMLSchema#dateTime"
        && right_datatype == "http://www.w3.org/2001/XMLSchema#dateTime")
        .then(|| temporal_datetime(left_value)?.partial_cmp(&temporal_datetime(right_value)?))?
}

fn expression_boolean(row: &Binding, expression: &Expression) -> Option<bool> {
    let RdfTerm::Literal {
        value,
        datatype: Some(datatype),
        language: None,
    } = evaluate_expression(row, expression)?
    else {
        return None;
    };
    (datatype == "http://www.w3.org/2001/XMLSchema#boolean").then_some(value == "true")
}

fn string_term(value: &str) -> Option<RdfTerm> {
    Some(RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
        language: None,
    })
}

fn boolean_term(value: bool) -> Option<RdfTerm> {
    Some(RdfTerm::Literal {
        value: value.to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()),
        language: None,
    })
}

fn evaluate_function(
    row: &Binding,
    name: &str,
    arguments: &[Expression],
    base_iri: Option<&str>,
) -> Option<RdfTerm> {
    let string = |index| {
        arguments
            .get(index)
            .and_then(|expression| expression_string(row, expression))
    };
    match name {
        "XSD:FLOAT" if arguments.len() == 1 => cast_numeric(row, &arguments[0], "float"),
        "XSD:DOUBLE" if arguments.len() == 1 => cast_numeric(row, &arguments[0], "double"),
        "XSD:DECIMAL" if arguments.len() == 1 => cast_numeric(row, &arguments[0], "decimal"),
        "XSD:INTEGER" if arguments.len() == 1 => cast_integer(row, &arguments[0]),
        "XSD:BOOLEAN" if arguments.len() == 1 => cast_boolean(row, &arguments[0]),
        "XSD:STRING" if arguments.len() == 1 => cast_string(row, &arguments[0]),
        "XSD:DATE" if arguments.len() == 1 => cast_date(row, &arguments[0]),
        "XSD:DATETIME" if arguments.len() == 1 => cast_datetime(row, &arguments[0]),
        "IRI" | "URI" if arguments.len() == 1 => {
            let value = evaluate_expression(row, &arguments[0])?;
            let value = match value {
                RdfTerm::Iri(value) | RdfTerm::Literal { value, .. } => value,
                RdfTerm::BlankNode(_) => return None,
            };
            if oxiri::Iri::parse(value.clone()).is_ok() {
                return Some(RdfTerm::Iri(value));
            }
            let base_iri = base_iri?;
            let resolved = if base_iri.contains('#')
                && base_iri.ends_with('/')
                && !value.starts_with('/')
                && !value.starts_with('#')
            {
                // Ontop 的 IRI() 基线将 `BASE <...#data/>` 视作可追加的词法基底；
                // 这与 RFC 3986 通常会丢弃 fragment 的 resolve 结果不同。
                format!("{base_iri}{value}")
            } else {
                let base = oxiri::Iri::parse(base_iri.to_owned()).ok()?;
                oxiri::IriRef::from(base).resolve(&value).ok()?.into_inner()
            };
            oxiri::Iri::parse(resolved.clone()).ok()?;
            Some(RdfTerm::Iri(resolved))
        }
        "BNODE" if arguments.is_empty() => Some(RdfTerm::BlankNode(Uuid::new_v4().to_string())),
        "BNODE" if arguments.len() == 1 => match evaluate_expression(row, &arguments[0])? {
            RdfTerm::Literal { value, .. } => Some(RdfTerm::BlankNode(value)),
            _ => None,
        },
        "IF" if arguments.len() == 3 => {
            if expression_boolean(row, &arguments[0])? {
                evaluate_expression(row, &arguments[1])
            } else {
                evaluate_expression(row, &arguments[2])
            }
        }
        "COALESCE" if !arguments.is_empty() => arguments
            .iter()
            .find_map(|argument| evaluate_expression(row, argument)),
        "SAMETERM" if arguments.len() == 2 => boolean_term(
            evaluate_expression(row, &arguments[0])? == evaluate_expression(row, &arguments[1])?,
        ),
        "LANG" if arguments.len() == 1 => {
            let RdfTerm::Literal { language, .. } = evaluate_expression(row, &arguments[0])? else {
                return None;
            };
            string_term(language.as_deref().unwrap_or(""))
        }
        "LANGMATCHES" if arguments.len() == 2 => {
            let language = string(0)?.to_ascii_lowercase();
            let range = string(1)?.to_ascii_lowercase();
            boolean_term(
                (!language.is_empty() && range == "*")
                    || language == range
                    || language
                        .strip_prefix(&range)
                        .is_some_and(|suffix| suffix.starts_with('-')),
            )
        }
        "DATATYPE" if arguments.len() == 1 => {
            let RdfTerm::Literal {
                datatype, language, ..
            } = evaluate_expression(row, &arguments[0])?
            else {
                return None;
            };
            if language.is_some() {
                return None;
            }
            Some(RdfTerm::Iri(datatype.unwrap_or_else(|| {
                "http://www.w3.org/2001/XMLSchema#string".into()
            })))
        }
        "BOUND" if arguments.len() == 1 => match &arguments[0] {
            Expression::Variable(variable) => boolean_term(row.contains_key(variable)),
            _ => None,
        },
        "ISIRI" | "ISURI" if arguments.len() == 1 => boolean_term(matches!(
            evaluate_expression(row, &arguments[0])?,
            RdfTerm::Iri(_)
        )),
        "ISBLANK" if arguments.len() == 1 => boolean_term(matches!(
            evaluate_expression(row, &arguments[0])?,
            RdfTerm::BlankNode(_)
        )),
        "ISLITERAL" if arguments.len() == 1 => boolean_term(matches!(
            evaluate_expression(row, &arguments[0])?,
            RdfTerm::Literal { .. }
        )),
        "ISNUMERIC" if arguments.len() == 1 => {
            let value = evaluate_expression(row, &arguments[0])?;
            boolean_term(matches!(
                value,
                RdfTerm::Literal {
                    datatype: Some(ref datatype),
                    ..
                } if is_numeric_datatype(datatype)
            ))
        }
        "ABS" if arguments.len() == 1 => decimal_term(numeric(row, &arguments[0])?.abs()),
        "CEIL" if arguments.len() == 1 => decimal_term(numeric(row, &arguments[0])?.ceil()),
        "FLOOR" if arguments.len() == 1 => decimal_term(numeric(row, &arguments[0])?.floor()),
        "ROUND" if arguments.len() == 1 => decimal_term(numeric(row, &arguments[0])?.round()),
        "YEAR" if arguments.len() == 1 => {
            temporal_integer(row, &arguments[0], |date, _| date.year())
        }
        "MONTH" if arguments.len() == 1 => {
            temporal_integer(row, &arguments[0], |date, _| date.month() as i32)
        }
        "DAY" if arguments.len() == 1 => {
            temporal_integer(row, &arguments[0], |date, _| date.day() as i32)
        }
        "HOURS" if arguments.len() == 1 => {
            temporal_integer(row, &arguments[0], |_, time| time.hour() as i32)
        }
        "MINUTES" if arguments.len() == 1 => {
            temporal_integer(row, &arguments[0], |_, time| time.minute() as i32)
        }
        "SECONDS" if arguments.len() == 1 => {
            let (_, time) = temporal_value(&expression_string(row, &arguments[0])?)?;
            decimal_term(time.second() as f64 + f64::from(time.nanosecond()) / 1_000_000_000.0)
        }
        "TZ" if arguments.len() == 1 => {
            let value = expression_string(row, &arguments[0])?;
            let timezone = chrono::DateTime::parse_from_rfc3339(&value)
                .ok()
                .map(|datetime| datetime.format("%:z").to_string())
                .map(|timezone| timezone.trim_start_matches('+').to_owned())
                .unwrap_or_default();
            string_term(&timezone)
        }
        "NOW" if arguments.is_empty() => Some(RdfTerm::Literal {
            value: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            datatype: Some("http://www.w3.org/2001/XMLSchema#dateTime".into()),
            language: None,
        }),
        "UUID" if arguments.is_empty() => {
            Some(RdfTerm::Iri(format!("urn:uuid:{}", Uuid::new_v4())))
        }
        "STRUUID" if arguments.is_empty() => string_term(&Uuid::new_v4().to_string()),
        "RAND" if arguments.is_empty() => decimal_term(random::<f64>()),
        "SHA256" if arguments.len() == 1 => {
            let digest = Sha256::digest(string(0)?.as_bytes());
            string_term(&format!("{digest:x}"))
        }
        "OFN:WEEKSBETWEEN" if arguments.len() == 2 => {
            duration_long(row, arguments, |duration| duration.num_days() / 7)
        }
        "OFN:DAYSBETWEEN" if arguments.len() == 2 => {
            duration_long(row, arguments, |duration| duration.num_days())
        }
        "OFN:HOURSBETWEEN" if arguments.len() == 2 => {
            duration_long(row, arguments, |duration| duration.num_hours())
        }
        "OFN:MINUTESBETWEEN" if arguments.len() == 2 => {
            duration_long(row, arguments, |duration| duration.num_minutes())
        }
        "OFN:SECONDSBETWEEN" if arguments.len() == 2 => {
            duration_long(row, arguments, |duration| duration.num_seconds())
        }
        "OFN:MILLISBETWEEN" if arguments.len() == 2 => {
            duration_long(row, arguments, |duration| duration.num_milliseconds())
        }
        "REGEX" if arguments.len() == 2 || arguments.len() == 3 => {
            let pattern = string(1)?;
            let mut regex = RegexBuilder::new(&pattern);
            if let Some(flags) = arguments
                .get(2)
                .and_then(|expression| expression_string(row, expression))
            {
                for flag in flags.chars() {
                    match flag {
                        'i' => {
                            regex.case_insensitive(true);
                        }
                        'm' => {
                            regex.multi_line(true);
                        }
                        's' => {
                            regex.dot_matches_new_line(true);
                        }
                        'x' => {
                            regex.ignore_whitespace(true);
                        }
                        _ => return None,
                    }
                }
            }
            boolean_term(regex.build().ok()?.is_match(&string(0)?))
        }
        "STR" if arguments.len() == 1 => {
            let value = match evaluate_expression(row, &arguments[0])? {
                RdfTerm::Iri(value) | RdfTerm::Literal { value, .. } => value,
                RdfTerm::BlankNode(_) => return None,
            };
            string_term(&value)
        }
        "ENCODE_FOR_URI" if arguments.len() == 1 => string_term(&encode_for_uri(&string(0)?)),
        "UCASE" if arguments.len() == 1 => map_literal_case(row, &arguments[0], str::to_uppercase),
        "LCASE" if arguments.len() == 1 => map_literal_case(row, &arguments[0], str::to_lowercase),
        "STRLEN" if arguments.len() == 1 => Some(RdfTerm::Literal {
            value: string(0)?.chars().count().to_string(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        }),
        "CONCAT" if !arguments.is_empty() => string_term(
            &arguments
                .iter()
                .map(|argument| expression_string(row, argument))
                .collect::<Option<Vec<_>>>()?
                .join(""),
        ),
        "CONTAINS" if arguments.len() == 2 => boolean_term(string(0)?.contains(&string(1)?)),
        "STRSTARTS" if arguments.len() == 2 => boolean_term(string(0)?.starts_with(&string(1)?)),
        "STRENDS" if arguments.len() == 2 => boolean_term(string(0)?.ends_with(&string(1)?)),
        "STRBEFORE" if arguments.len() == 2 => {
            let RdfTerm::Literal {
                value,
                datatype,
                language,
            } = evaluate_expression(row, &arguments[0])?
            else {
                return None;
            };
            let needle = string(1)?;
            match value.split_once(&needle) {
                Some((before, _)) => Some(RdfTerm::Literal {
                    value: before.into(),
                    datatype,
                    language,
                }),
                None => string_term(""),
            }
        }
        "STRAFTER" if arguments.len() == 2 => {
            let RdfTerm::Literal {
                value,
                datatype,
                language,
            } = evaluate_expression(row, &arguments[0])?
            else {
                return None;
            };
            let needle = string(1)?;
            match value.split_once(&needle) {
                Some((_, after)) => Some(RdfTerm::Literal {
                    value: after.into(),
                    datatype,
                    language,
                }),
                None => string_term(""),
            }
        }
        "SUBSTR" if arguments.len() == 2 || arguments.len() == 3 => {
            let RdfTerm::Literal {
                value,
                datatype,
                language,
            } = evaluate_expression(row, &arguments[0])?
            else {
                return None;
            };
            let start = string(1)?.parse::<usize>().ok()?.saturating_sub(1);
            let characters = value.chars().skip(start);
            let result: String = if let Some(length) = arguments.get(2) {
                characters
                    .take(expression_string(row, length)?.parse::<usize>().ok()?)
                    .collect::<String>()
            } else {
                characters.collect::<String>()
            };
            Some(RdfTerm::Literal {
                value: result,
                datatype,
                language,
            })
        }
        _ => None,
    }
}

fn cast_numeric(row: &Binding, expression: &Expression, datatype: &str) -> Option<RdfTerm> {
    let value = evaluate_expression(row, expression)?;
    let target_datatype = format!("http://www.w3.org/2001/XMLSchema#{datatype}");
    let number = match &value {
        RdfTerm::Literal {
            value,
            datatype: Some(source),
            language: None,
        } if source == "http://www.w3.org/2001/XMLSchema#boolean" => match value.as_str() {
            "true" | "1" => 1.0,
            "false" | "0" => 0.0,
            _ => return None,
        },
        RdfTerm::Literal {
            value,
            datatype: Some(source),
            language: None,
        } if source == "http://www.w3.org/2001/XMLSchema#string" => value.parse().ok()?,
        _ => term_numeric_value(&value)?.0,
    };
    if matches!(
        &value,
        RdfTerm::Literal {
            value,
            datatype: Some(source),
            language: None,
        } if source == &target_datatype && value.parse::<f64>().is_ok()
    ) {
        return Some(RdfTerm::Literal {
            value: match value {
                RdfTerm::Literal { value, .. } => value.clone(),
                _ => unreachable!(),
            },
            datatype: Some(target_datatype),
            language: None,
        });
    }
    numeric_result_term(number, &target_datatype)
}

fn cast_integer(row: &Binding, expression: &Expression) -> Option<RdfTerm> {
    let value = evaluate_expression(row, expression)?;
    let number = match &value {
        RdfTerm::Literal {
            value,
            datatype: Some(source),
            language: None,
        } if source == "http://www.w3.org/2001/XMLSchema#boolean" => match value.as_str() {
            "true" | "1" => 1.0,
            "false" | "0" => 0.0,
            _ => return None,
        },
        RdfTerm::Literal {
            value,
            datatype: Some(source),
            language: None,
        } if source == "http://www.w3.org/2001/XMLSchema#string" => value.parse::<f64>().ok()?,
        _ => term_numeric_value(&value)?.0,
    };
    if !number.is_finite() || number < i64::MIN as f64 || number > i64::MAX as f64 {
        return None;
    }
    Some(RdfTerm::Literal {
        value: (number.trunc() as i64).to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    })
}

fn cast_boolean(row: &Binding, expression: &Expression) -> Option<RdfTerm> {
    let value = evaluate_expression(row, expression)?;
    let boolean = match &value {
        RdfTerm::Literal {
            value,
            datatype: Some(source),
            language: None,
        } if source == "http://www.w3.org/2001/XMLSchema#boolean" => match value.as_str() {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return None,
        },
        RdfTerm::Literal {
            value,
            datatype: Some(source),
            language: None,
        } if source == "http://www.w3.org/2001/XMLSchema#string" => match value.as_str() {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return None,
        },
        _ => term_numeric_value(&value).map(|(number, _)| number != 0.0)?,
    };
    boolean_term(boolean)
}

fn cast_string(row: &Binding, expression: &Expression) -> Option<RdfTerm> {
    let value = match evaluate_expression(row, expression)? {
        RdfTerm::Literal { value, .. } | RdfTerm::Iri(value) => value,
        RdfTerm::BlankNode(_) => return None,
    };
    string_term(&value)
}

fn temporal_cast_input(row: &Binding, expression: &Expression) -> Option<String> {
    let RdfTerm::Literal {
        value,
        datatype: Some(datatype),
        language: None,
    } = evaluate_expression(row, expression)?
    else {
        return None;
    };
    matches!(
        datatype.as_str(),
        "http://www.w3.org/2001/XMLSchema#date"
            | "http://www.w3.org/2001/XMLSchema#dateTime"
            | "http://www.w3.org/2001/XMLSchema#string"
    )
    .then_some(value)
}

fn cast_date(row: &Binding, expression: &Expression) -> Option<RdfTerm> {
    let value = temporal_cast_input(row, expression)?;
    let date = temporal_value(&value)?.0;
    Some(RdfTerm::Literal {
        value: date.format("%Y-%m-%d").to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#date".into()),
        language: None,
    })
}

fn cast_datetime(row: &Binding, expression: &Expression) -> Option<RdfTerm> {
    let value = temporal_cast_input(row, expression)?;
    let datetime = temporal_datetime(&value)?;
    Some(RdfTerm::Literal {
        value: datetime.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#dateTime".into()),
        language: None,
    })
}

fn numeric(row: &Binding, expression: &Expression) -> Option<f64> {
    numeric_value(row, expression).map(|(value, _)| value)
}

fn encode_for_uri(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn map_literal_case(
    row: &Binding,
    expression: &Expression,
    map: impl FnOnce(&str) -> String,
) -> Option<RdfTerm> {
    let RdfTerm::Literal {
        value,
        datatype,
        language,
    } = evaluate_expression(row, expression)?
    else {
        return None;
    };
    Some(RdfTerm::Literal {
        value: map(&value),
        datatype,
        language,
    })
}

fn numeric_value(row: &Binding, expression: &Expression) -> Option<(f64, String)> {
    let RdfTerm::Literal {
        value, datatype, ..
    } = evaluate_expression(row, expression)?
    else {
        return None;
    };
    let datatype = datatype.unwrap_or_else(|| "http://www.w3.org/2001/XMLSchema#string".into());
    if !is_numeric_datatype(&datatype) {
        return None;
    }
    Some((value.parse().ok()?, datatype))
}

fn promoted_numeric_datatype(left: &str, right: &str) -> &'static str {
    if left.ends_with("#double") || right.ends_with("#double") {
        "http://www.w3.org/2001/XMLSchema#double"
    } else if left.ends_with("#float") || right.ends_with("#float") {
        "http://www.w3.org/2001/XMLSchema#float"
    } else {
        "http://www.w3.org/2001/XMLSchema#decimal"
    }
}

fn numeric_result_term(value: f64, datatype: &str) -> Option<RdfTerm> {
    let decimal = datatype.ends_with("#decimal");
    Some(RdfTerm::Literal {
        value: if decimal {
            let lexical = format!("{value:.12}");
            let lexical = lexical.trim_end_matches('0').trim_end_matches('.');
            if lexical.is_empty() || lexical == "-0" {
                "0".into()
            } else {
                lexical.into()
            }
        } else if value.fract() == 0.0 {
            format!("{value:.0}")
        } else {
            value.to_string()
        },
        datatype: Some(datatype.into()),
        language: None,
    })
}

fn numeric_term(value: &RdfTerm) -> Option<f64> {
    let RdfTerm::Literal {
        value, datatype, ..
    } = value
    else {
        return None;
    };
    datatype
        .as_deref()
        .is_none_or(is_numeric_datatype)
        .then(|| value.parse().ok())?
}

fn is_numeric_datatype(datatype: &str) -> bool {
    matches!(
        datatype,
        "http://www.w3.org/2001/XMLSchema#byte"
            | "http://www.w3.org/2001/XMLSchema#short"
            | "http://www.w3.org/2001/XMLSchema#int"
            | "http://www.w3.org/2001/XMLSchema#integer"
            | "http://www.w3.org/2001/XMLSchema#long"
            | "http://www.w3.org/2001/XMLSchema#decimal"
            | "http://www.w3.org/2001/XMLSchema#float"
            | "http://www.w3.org/2001/XMLSchema#double"
            | "http://www.w3.org/2001/XMLSchema#nonPositiveInteger"
            | "http://www.w3.org/2001/XMLSchema#negativeInteger"
            | "http://www.w3.org/2001/XMLSchema#nonNegativeInteger"
            | "http://www.w3.org/2001/XMLSchema#positiveInteger"
            | "http://www.w3.org/2001/XMLSchema#unsignedByte"
            | "http://www.w3.org/2001/XMLSchema#unsignedShort"
            | "http://www.w3.org/2001/XMLSchema#unsignedInt"
            | "http://www.w3.org/2001/XMLSchema#unsignedLong"
    )
}

fn decimal_term(value: f64) -> Option<RdfTerm> {
    Some(RdfTerm::Literal {
        value: if value.fract() == 0.0 {
            format!("{value:.0}")
        } else {
            value.to_string()
        },
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    })
}

fn integer_term(value: i32) -> Option<RdfTerm> {
    Some(RdfTerm::Literal {
        value: value.to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    })
}

fn temporal_integer(
    row: &Binding,
    expression: &Expression,
    extract: impl FnOnce(NaiveDate, NaiveTime) -> i32,
) -> Option<RdfTerm> {
    let (date, time) = temporal_value(&expression_string(row, expression)?)?;
    integer_term(extract(date, time))
}

fn temporal_value(value: &str) -> Option<(NaiveDate, NaiveTime)> {
    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some((datetime.date_naive(), datetime.time()));
    }
    if let Ok(datetime) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some((datetime.date(), datetime.time()));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some((date, NaiveTime::from_hms_opt(0, 0, 0)?));
    }
    let time = NaiveTime::parse_from_str(value, "%H:%M:%S%.f").ok()?;
    Some((NaiveDate::from_ymd_opt(1970, 1, 1)?, time))
}

fn duration_long(
    row: &Binding,
    arguments: &[Expression],
    extract: impl FnOnce(chrono::Duration) -> i64,
) -> Option<RdfTerm> {
    let start = temporal_datetime(&expression_string(row, &arguments[0])?)?;
    let end = temporal_datetime(&expression_string(row, &arguments[1])?)?;
    Some(RdfTerm::Literal {
        value: extract(end - start).to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#long".into()),
        language: None,
    })
}

fn temporal_datetime(value: &str) -> Option<NaiveDateTime> {
    let (date, time) = temporal_value(value)?;
    Some(NaiveDateTime::new(date, time))
}

fn expression_string(row: &Binding, expression: &Expression) -> Option<String> {
    match evaluate_expression(row, expression)? {
        RdfTerm::Literal { value, .. } => Some(value),
        _ => None,
    }
}

fn matches_filter(row: &Binding, filter: &Filter) -> bool {
    match filter {
        Filter::Language { variable, language } => {
            matches!(row.get(variable), Some(RdfTerm::Literal { language: Some(actual), .. }) if actual == language)
        }
        Filter::Equal { variable, value } => {
            let Some(RdfTerm::Literal {
                value: actual,
                datatype,
                ..
            }) = row.get(variable)
            else {
                return false;
            };
            match value {
                FilterValue::Numeric(expected) => actual
                    .parse::<f64>()
                    .is_ok_and(|actual| actual == *expected),
                FilterValue::Literal {
                    value: expected,
                    datatype: expected_datatype,
                } => {
                    if expected_datatype
                        .as_deref()
                        .is_some_and(is_numeric_datatype)
                        && datatype.as_deref().is_some_and(is_numeric_datatype)
                    {
                        return actual.parse::<f64>().ok() == expected.parse::<f64>().ok();
                    }
                    if expected_datatype
                        .as_deref()
                        .is_some_and(is_boolean_datatype)
                    {
                        return boolean_lexical(actual) == boolean_lexical(expected);
                    }
                    if expected_datatype
                        .as_deref()
                        .is_some_and(is_datetime_stamp_datatype)
                        && datatype.as_deref().is_some_and(is_datetime_stamp_datatype)
                    {
                        return chrono::DateTime::parse_from_rfc3339(actual).ok()
                            == chrono::DateTime::parse_from_rfc3339(expected).ok();
                    }
                    if expected_datatype.as_deref()
                        == Some("http://www.w3.org/2001/XMLSchema#dateTime")
                        && datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#dateTime")
                    {
                        return temporal_datetime(actual) == temporal_datetime(expected);
                    }
                    // PostgreSQL TIMESTAMPTZ wire 值不携带原始 offset；adapter 以 UTC
                    // 规范化它。映射显式标为 xsd:string 时，Ontop 基线仍以同一 instant
                    // 的带 offset 词法作 FILTER。本分支只处理两侧皆可解析的时区 datetime，
                    // 其余 xsd:string 保持严格词法比较。
                    if expected_datatype.as_deref()
                        == Some("http://www.w3.org/2001/XMLSchema#string")
                        && datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#string")
                    {
                        if let (Ok(actual), Ok(expected)) = (
                            chrono::DateTime::parse_from_rfc3339(actual),
                            chrono::DateTime::parse_from_rfc3339(expected),
                        ) {
                            return actual == expected;
                        }
                    }
                    actual == expected
                        && match expected_datatype {
                            Some(expected) => datatype.as_ref() == Some(expected),
                            // SPARQL simple literal 只与 simple/xsd:string literal 相等，
                            // 不可因词法相同而与 date、time 等有类型值相等。
                            None => datatype.as_deref().is_none_or(|actual| {
                                actual == "http://www.w3.org/2001/XMLSchema#string"
                            }),
                        }
                }
            }
        }
        Filter::Expression(expression) => matches!(
            evaluate_expression(row, expression),
            Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None })
                if datatype == "http://www.w3.org/2001/XMLSchema#boolean" && value == "true"
        ),
    }
}

fn is_boolean_datatype(datatype: &str) -> bool {
    datatype == "http://www.w3.org/2001/XMLSchema#boolean"
}

fn is_datetime_stamp_datatype(datatype: &str) -> bool {
    datatype == "http://www.w3.org/2001/XMLSchema#dateTimeStamp"
}

fn boolean_lexical(value: &str) -> Option<bool> {
    match value {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn join(left: &Binding, right: &Binding) -> Option<Binding> {
    let mut merged = left.clone();
    for (name, value) in right {
        if let Some(existing) = merged.get(name) {
            if existing != value {
                return None;
            }
        } else {
            merged.insert(name.clone(), value.clone());
        }
    }
    Some(merged)
}

/// 将两个 solution multiset 连接。若某变量在两个 relation 的每一行中都绑定，使用它
/// 作为稳定索引键；否则退回通用的 SPARQL 兼容性 nested-loop join（未绑定变量仍可兼容）。
fn join_binding_relations(left: Vec<Binding>, right: Vec<Binding>) -> Vec<Binding> {
    let Some(first_left) = left.first() else {
        return Vec::new();
    };
    let Some(first_right) = right.first() else {
        return Vec::new();
    };
    let shared_key = first_left
        .keys()
        .find(|name| {
            first_right.contains_key(*name)
                && left.iter().all(|row| row.contains_key(*name))
                && right.iter().all(|row| row.contains_key(*name))
        })
        .cloned();

    let Some(shared_key) = shared_key else {
        return left
            .into_iter()
            .flat_map(|row| right.iter().filter_map(move |next| join(&row, next)))
            .collect();
    };
    let mut index = BTreeMap::<RdfTerm, Vec<&Binding>>::new();
    for row in &right {
        index.entry(row[&shared_key].clone()).or_default().push(row);
    }
    left.into_iter()
        .flat_map(|row| {
            index
                .get(&row[&shared_key])
                .into_iter()
                .flatten()
                .filter_map(move |next| join(&row, next))
        })
        .collect()
}

/// 选择下一个 BGP relation：优先选择能与当前 bindings 共享最多变量的候选；无共享
/// 时才按 relation 大小选择。这样避免先进行不相关的笛卡尔积，随后才施加连接键。
fn next_bgp_candidate(rows: &[Binding], candidates: &[Vec<Binding>]) -> usize {
    let bound_names = rows
        .iter()
        .flat_map(|row| row.keys())
        .collect::<std::collections::BTreeSet<_>>();
    candidates
        .iter()
        .enumerate()
        .min_by_key(|(_, candidate)| {
            let shared = candidate
                .iter()
                .flat_map(|row| row.keys())
                .collect::<std::collections::BTreeSet<_>>()
                .intersection(&bound_names)
                .count();
            (Reverse(shared), candidate.len())
        })
        .map(|(index, _)| index)
        .expect("调用方保证 BGP 候选非空")
}

#[cfg(test)]
mod join_tests {
    use super::{canonical_floating_lexical, join_binding_relations, Binding, RdfTerm};

    fn binding(pairs: &[(&str, &str)]) -> Binding {
        pairs
            .iter()
            .map(|(name, value)| ((*name).into(), RdfTerm::Iri((*value).into())))
            .collect()
    }

    #[test]
    fn indexes_a_shared_bound_variable_without_losing_bag_matches() {
        let left = vec![binding(&[("movie", "m1")]), binding(&[("movie", "m2")])];
        let right = vec![
            binding(&[("movie", "m2"), ("company", "c2")]),
            binding(&[("movie", "m1"), ("company", "c1a")]),
            binding(&[("movie", "m1"), ("company", "c1b")]),
        ];

        let rows = join_binding_relations(left, right);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].get("company"), Some(&RdfTerm::Iri("c1a".into())));
        assert_eq!(rows[1].get("company"), Some(&RdfTerm::Iri("c1b".into())));
        assert_eq!(rows[2].get("company"), Some(&RdfTerm::Iri("c2".into())));
    }

    #[test]
    fn canonicalizes_postgres_double_lexicals_for_direct_mapping() {
        let double = Some("http://www.w3.org/2001/XMLSchema#double");
        assert_eq!(canonical_floating_lexical("30", double), "3.0E1");
        assert_eq!(canonical_floating_lexical("1.25", double), "1.25E0");
        assert_eq!(canonical_floating_lexical("Venus", double), "Venus");
    }
}

fn resolve_mapping_iri(value: &str, base: Option<&str>) -> Result<String, RuntimeError> {
    let resolved = if value.contains(':') {
        value.into()
    } else if let Some(base) = base {
        format!("{base}{value}")
    } else {
        value.into()
    };
    if base.is_some() && oxiri::Iri::parse(resolved.clone()).is_err() {
        return Err(RuntimeError::Mapping(format!(
            "R2RML data error：IRI 值无效：{value}"
        )));
    }
    Ok(resolved)
}

fn fact_matches(query: &TriplePattern, fact: &RdfFact, ontology: &ontology::Ontology) -> bool {
    if query.object.starts_with('?') || rdf_term_matches(&fact.object, &query.object) {
        return true;
    }
    matches!(&fact.object, RdfTerm::Iri(actual)
        if fact.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
        && query.object.starts_with('<')
        && query.object.ends_with('>')
        && ontology.is_subclass_of(actual, query.object.trim_matches(['<', '>'])))
}

fn fact_binding(
    pattern: &TriplePattern,
    fact: &RdfFact,
    ontology: &ontology::Ontology,
) -> Option<Binding> {
    let mut binding = Binding::new();
    for (token, term) in [
        (&pattern.subject, &fact.subject),
        (&pattern.object, &fact.object),
    ] {
        if let Some(name) = token.strip_prefix('?') {
            binding.insert(name.into(), term.clone());
        } else if constant_matches(token, term, pattern, fact, ontology) {
        } else {
            return None;
        }
    }
    Some(binding)
}

fn constant_matches(
    token: &str,
    term: &RdfTerm,
    pattern: &TriplePattern,
    fact: &RdfFact,
    ontology: &ontology::Ontology,
) -> bool {
    rdf_term_matches(term, token)
        || (token == pattern.object
            && fact.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            && token.starts_with('<')
            && token.ends_with('>')
            && matches!(term, RdfTerm::Iri(value) if ontology.is_subclass_of(value, token.trim_matches(['<', '>']))))
}

fn rdf_term_matches(term: &RdfTerm, token: &str) -> bool {
    match term {
        RdfTerm::Iri(value) => token == format!("<{value}>"),
        RdfTerm::BlankNode(value) => token == format!("_:{value}"),
        RdfTerm::Literal {
            value,
            datatype,
            language,
        } => {
            if let Some((expected_value, expected_datatype)) = bare_numeric_literal(token) {
                return value == expected_value
                    && datatype.as_deref() == Some(expected_datatype)
                    && language.is_none();
            }
            let Some(rest) = token.strip_prefix('"') else {
                return false;
            };
            let Some((literal, suffix)) = rest.rsplit_once('"') else {
                return false;
            };
            literal == value
                && match suffix {
                    "" => datatype.is_none() && language.is_none(),
                    suffix if suffix.starts_with('@') => language
                        .as_deref()
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(&suffix[1..])),
                    suffix if suffix.starts_with("^^<") && suffix.ends_with('>') => {
                        let expected = &suffix[3..suffix.len() - 1];
                        // RDF 1.1 的无 language simple literal 是 xsd:string 的简写。
                        // 原生 OBDA 未显式 datatype 的 template 因而可被
                        // `"..."^^xsd:string` 的 SPARQL BGP 常量匹配。
                        language.is_none()
                            && (datatype.as_deref() == Some(expected)
                                || (expected == "http://www.w3.org/2001/XMLSchema#string"
                                    && datatype.is_none()))
                    }
                    _ => false,
                }
        }
    }
}

fn bare_numeric_literal(token: &str) -> Option<(&str, &'static str)> {
    token.parse::<f64>().ok()?;
    let datatype = if token.contains('e') || token.contains('E') {
        "http://www.w3.org/2001/XMLSchema#double"
    } else if token.contains('.') {
        "http://www.w3.org/2001/XMLSchema#decimal"
    } else {
        "http://www.w3.org/2001/XMLSchema#integer"
    };
    Some((token, datatype))
}

fn graph_matches(query_graph: Option<&str>, fact_graph: Option<&RdfTerm>) -> bool {
    match (query_graph, fact_graph) {
        (None, None) => true,
        (Some(expected), Some(RdfTerm::Iri(actual))) => expected == actual,
        (Some(expected), Some(RdfTerm::BlankNode(actual))) => expected == format!("_:{actual}"),
        _ => false,
    }
}

fn variables(pattern: &TriplePattern) -> Vec<String> {
    [
        pattern.subject.as_str(),
        pattern.predicate.as_str(),
        pattern.object.as_str(),
    ]
    .into_iter()
    .filter_map(|value| value.strip_prefix('?').map(str::to_owned))
    .collect()
}

/// Ontop 的 Direct Mapping 将 PostgreSQL float/double 投影为 canonical XSD
/// scientific lexical form；仅在形成 RDF term 时规范化，SQL 参数仍保留服务器文本。
fn canonical_floating_lexical(value: &str, datatype: Option<&str>) -> String {
    let is_floating = matches!(
        datatype,
        Some("http://www.w3.org/2001/XMLSchema#float" | "http://www.w3.org/2001/XMLSchema#double")
    );
    if !is_floating {
        return value.into();
    }
    let Ok(number) = value.parse::<f64>() else {
        return value.into();
    };
    if number.is_nan() {
        return "NaN".into();
    }
    if number == f64::INFINITY {
        return "INF".into();
    }
    if number == f64::NEG_INFINITY {
        return "-INF".into();
    }
    let scientific = format!("{number:e}");
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return value.into();
    };
    let mantissa = if mantissa.contains('.') {
        mantissa.into()
    } else {
        format!("{mantissa}.0")
    };
    let exponent = exponent.parse::<i32>().unwrap_or_default();
    format!("{mantissa}E{exponent}")
}

fn instantiate(pattern: &TriplePattern, binding: &Binding) -> Option<RdfFact> {
    let term = |token: &str| {
        token
            .strip_prefix('?')
            .and_then(|name| binding.get(name).cloned())
            .or_else(|| {
                token
                    .strip_prefix('<')
                    .and_then(|v| v.strip_suffix('>'))
                    .map(|v| RdfTerm::Iri(v.into()))
            })
    };
    let subject = term(&pattern.subject)?;
    let predicate = if let Some(name) = pattern.predicate.strip_prefix('?') {
        match binding.get(name) {
            Some(RdfTerm::Iri(iri)) => iri.clone(),
            _ => return None,
        }
    } else {
        pattern
            .predicate
            .strip_prefix('<')?
            .strip_suffix('>')?
            .into()
    };
    let object = term(&pattern.object)?;
    Some(RdfFact {
        subject,
        predicate,
        object,
        graph: pattern
            .graph
            .as_ref()
            .map(|graph| RdfTerm::Iri(graph.clone())),
    })
}
