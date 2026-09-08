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

use bigdecimal::{BigDecimal, RoundingMode, Zero};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use mapping::BindingTerm;
use rand::random;
use regex::{Regex, RegexBuilder};
use sha2::{Digest, Sha256};
use sparql::{
    graph_pattern_bind_variables, graph_pattern_variables, parse as parse_query, Aggregate,
    AggregateKind, ArithmeticOperator, Bind, ComparisonOperator, Expression, Filter, FilterValue,
    GraphPattern, GroupBy, LogicalOperator, OrderByTerm, Query, TriplePattern,
};
use std::collections::HashMap;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Read,
    str::FromStr,
};
use uuid::Uuid;

/// 验证 endpoint 启动时即可判定的本地输入，而不建立 PostgreSQL 连接。
/// Ontop 会在 HTTP listener 就绪前读取 mapping、facts 与 ontology；这里保持同一
/// 失败边界，同时让数据源连接仍按请求惰性创建。
pub fn validate_static_inputs(
    spec: &KnowledgeGraphSpec,
    validate_mapping: bool,
    relaxed_native_source_sql: bool,
) -> Result<(), RuntimeError> {
    if validate_mapping {
        if relaxed_native_source_sql {
            Mapping::parse_file_relaxed_source_sql(&spec.mapping_file)?;
        } else {
            Mapping::parse_file(&spec.mapping_file)?;
        }
    }
    let facts = load_facts(spec)?;
    let ontology = load_ontology(spec)?;
    ontology.validate_facts(&facts)
}

fn load_facts(spec: &KnowledgeGraphSpec) -> Result<Vec<RdfFact>, RuntimeError> {
    let Some(path) = &spec.facts_file else {
        return Ok(Vec::new());
    };
    let content =
        std::fs::read(path).map_err(|e| RuntimeError::Facts(format!("无法读取 facts：{e}")))?;
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
        Some("ttl" | "turtle") => parse_turtle(content, base_iri),
        Some("nq" | "nquads") => parse_nquads(content),
        Some("rdf" | "xml" | "rdfxml") => parse_rdf_xml(content, base_iri),
        _ => Err(RuntimeError::Facts("未提供有效的 facts 文件格式".into())),
    }
}

fn load_ontology(spec: &KnowledgeGraphSpec) -> Result<ontology::Ontology, RuntimeError> {
    match &spec.ontology_file {
        Some(path) => ontology::Ontology::load_with_catalog(path, spec.xml_catalog_file.as_deref()),
        None => Ok(ontology::Ontology::default()),
    }
}

/// VKG 的唯一高层 seam：加载配置并执行查询。
pub struct VkgRuntime<D> {
    spec: KnowledgeGraphSpec,
    mapping_infer_default_datatype: bool,
    mapping_require_absolute_iri_values: bool,
    canonicalize_floating_lexicals: bool,
    source: D,
    mapping: Mapping,
    facts: Vec<RdfFact>,
    /// 空 facts 文件仍声明一个可查询的（但为空的）静态 RDF graph；这不同于未配置
    /// facts 输入时仅能尝试 virtual mapping 的状态。
    has_facts_input: bool,
    ontology: ontology::Ontology,
    buffered_wkts: HashMap<String, (String, String)>,
}

fn mapping_value_marker(variable: &str) -> String {
    format!("__rtop_mapping_value_{variable}")
}

impl<D: DataSource> VkgRuntime<D> {
    pub fn new(spec: KnowledgeGraphSpec, source: D) -> Result<Self, RuntimeError> {
        Self::new_with_mapping_datatype_inference(spec, source, true)
    }

    /// 显式控制没有 rr:datatype/语言标签的映射 object 是否采用 PostgreSQL 服务器类型。
    pub fn new_with_mapping_datatype_inference(
        spec: KnowledgeGraphSpec,
        source: D,
        mapping_infer_default_datatype: bool,
    ) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse_file(&spec.mapping_file)?;
        Self::from_mapping(
            spec,
            source,
            mapping,
            mapping_infer_default_datatype,
            false,
            false,
        )
    }

    pub fn new_with_mapping_options(
        spec: KnowledgeGraphSpec,
        source: D,
        mapping_infer_default_datatype: bool,
        mapping_require_absolute_iri_values: bool,
    ) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse_file(&spec.mapping_file)?;
        Self::from_mapping(
            spec,
            source,
            mapping,
            mapping_infer_default_datatype,
            mapping_require_absolute_iri_values,
            false,
        )
    }

    /// 与 Ontop endpoint 一样，native source SQL 可延迟到 PostgreSQL 查询期解析；
    /// 仅 endpoint 调用此路径，CLI 仍保留严格 source-SQL validation。
    pub fn new_with_mapping_options_relaxed_source_sql(
        spec: KnowledgeGraphSpec,
        source: D,
        mapping_infer_default_datatype: bool,
        mapping_require_absolute_iri_values: bool,
    ) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse_file_relaxed_source_sql(&spec.mapping_file)?;
        Self::from_mapping(
            spec,
            source,
            mapping,
            mapping_infer_default_datatype,
            mapping_require_absolute_iri_values,
            false,
        )
    }

    /// 从 reader 加载 Turtle R2RML；显式 base IRI 使相对 IRI 语义不依赖临时文件路径。
    pub fn new_with_r2rml_reader<R: Read>(
        spec: KnowledgeGraphSpec,
        reader: R,
        base_iri: &str,
        source: D,
    ) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse_r2rml_reader(reader, base_iri)?;
        Self::from_mapping(spec, source, mapping, true, false, false)
    }

    fn from_mapping(
        spec: KnowledgeGraphSpec,
        source: D,
        mapping: Mapping,
        mapping_infer_default_datatype: bool,
        mapping_require_absolute_iri_values: bool,
        canonicalize_floating_lexicals: bool,
    ) -> Result<Self, RuntimeError> {
        let has_facts_input = spec.facts_file.is_some();
        let facts = load_facts(&spec)?;
        let ontology = load_ontology(&spec)?;
        ontology.validate_facts(&facts)?;
        Ok(Self {
            spec,
            mapping_infer_default_datatype,
            mapping_require_absolute_iri_values,
            canonicalize_floating_lexicals,
            source,
            mapping,
            facts,
            has_facts_input,
            ontology,
            buffered_wkts: HashMap::new(),
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
                having,
                distinct,
                order_by,
                offset,
                limit,
            } => {
                let mut rows = self.evaluate_graph_pattern(&pattern, vec![Binding::new()])?;
                if !aggregates.is_empty()
                    || !having.is_empty()
                    || projection_binds
                        .iter()
                        .any(|bind| expression_contains_aggregate(&bind.expression))
                    || order_by
                        .iter()
                        .any(|term| expression_contains_aggregate(&term.expression))
                {
                    rows = aggregate_bindings(
                        rows,
                        &aggregates,
                        &group_by,
                        &projection_binds,
                        &having,
                        &order_by,
                        self.has_facts_input,
                    );
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
            Query::Ask { pattern } => Ok(QueryResult::Boolean(
                !self
                    .evaluate_graph_pattern(&pattern, vec![Binding::new()])?
                    .is_empty(),
            )),
            Query::Construct { template, pattern } => {
                let rows = self.evaluate_graph_pattern(&pattern, vec![Binding::new()])?;
                // CONSTRUCT 的输出是 RDF graph，不是 SELECT solution bag；不同
                // solution mapping 即使实例化出同一 triple 也只能在结果图中出现
                // 一次（DAWG sq14 的多 homepage 行会重复 type/name/mbox）。
                let mut graph = Vec::new();
                for (row_index, row) in rows.into_iter().enumerate() {
                    // CONSTRUCT template 的 blank node label 和 `[]` property list
                    // 每个 solution mapping 都必须新鲜，但同一 mapping 展开的多条
                    // template triple 必须共享它。不能把它们当作 WHERE 未绑定变量。
                    let mut template_blank_nodes = HashMap::new();
                    for fact in template.iter().filter_map(|pattern| {
                        instantiate(pattern, &row, row_index, &mut template_blank_nodes)
                    }) {
                        if !graph.contains(&fact) {
                            graph.push(fact);
                        }
                    }
                }
                Ok(QueryResult::Graph(graph))
            }
            Query::Describe { resources, pattern } => {
                let mut resources = resources
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>();
                if let Some(pattern) = pattern {
                    for row in self.evaluate_graph_pattern(&pattern, vec![Binding::new()])? {
                        for resource in resources.clone() {
                            if let Some(variable) = resource.strip_prefix('?') {
                                if let Some(RdfTerm::Iri(iri)) = row.get(variable) {
                                    resources.insert(iri.clone());
                                }
                            }
                        }
                    }
                }
                resources.retain(|resource| !resource.starts_with('?'));
                let mut graph = self
                    .facts
                    .iter()
                    .filter(
                        |fact| matches!(&fact.subject, RdfTerm::Iri(value) if resources.contains(value)),
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
                            .filter(|subject| resources.contains(subject)),
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
                Ok(mut plans)
                    if query.predicate != "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>" =>
                {
                    // 一个超属性自身已有 mapping 时，也仍必须并入其子属性和反向
                    // 属性的 mapping。LUBM 的 memberOf 直接映射学生，而 worksFor
                    // 是它的子属性并映射教授；只在 direct mapping 缺失时展开会遗漏
                    // 合法的 superclass/property entailment。
                    let predicate = query.predicate.trim_matches(['<', '>']);
                    // inverseOf 的两边若都有 mapping，会把同一 RDF triple 的直接
                    // 与反向蕴含各加入一次；虚拟图不是 derivation bag。既有 fallback
                    // 分支会在没有直接 mapping 时处理 inverse，故这里仅补子属性。
                    for property in self.ontology.subproperties_of(predicate) {
                        if property == predicate {
                            continue;
                        }
                        let mut candidate = query.clone();
                        candidate.predicate = format!("<{property}>");
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
        if plans.is_empty()
            && !self.has_facts_input
            && self.facts.is_empty()
            && self.ontology.facts().is_empty()
        {
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
                                            self.mapping_require_absolute_iri_values,
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
                                                (*infer_datatype
                                                    && self.mapping_infer_default_datatype)
                                                    .then(|| value.datatype.clone())
                                                    .flatten()
                                            });
                                            // Ontop 的 PostgreSQL endpoint 将 native target 中
                                            // 显式 xsd:string 序列化为 simple literal。
                                            let datatype = (datatype.as_deref()
                                                != Some("http://www.w3.org/2001/XMLSchema#string"))
                                            .then_some(datatype)
                                            .flatten();
                                            RdfTerm::Literal {
                                                value: mapping_floating_lexical(
                                                    &value.value,
                                                    datatype.as_deref(),
                                                    self.canonicalize_floating_lexicals,
                                                ),
                                                datatype,
                                                language: language.clone(),
                                            }
                                        }
                                    };
                                    binding.insert(name.clone(), term);
                                    binding.insert(
                                        mapping_value_marker(name),
                                        boolean_term(true).expect("boolean literal"),
                                    );
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
                            // 变量谓词由 `fact_matches` 绑定；它不是要参与 TBox
                            // 子属性比较的常量 IRI。否则 `GRAPH { ?s ?p ?o }`
                            // 会错误丢弃所有 facts（含 N-Quads 的具名图）。
                            && (query.predicate.starts_with('?')
                                || self.ontology.is_subproperty_of(
                                    &candidate.predicate,
                                    query.predicate.trim_matches(['<', '>']),
                                ))
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
            // 其 mapping-value 标记也必须一并移除；否则标记会使后续的 type
            // 去重把同一资源误认为来自不同解映射。
            for binding in &mut bindings {
                binding.retain(|name, _| {
                    !name.starts_with("__rtop_type_")
                        && !name
                            .strip_prefix("__rtop_mapping_value_")
                            .is_some_and(|variable| variable.starts_with("__rtop_type_"))
                });
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
                    let mut projected = projections
                        .iter()
                        .filter_map(|name| {
                            row.get(name).cloned().map(|value| (name.clone(), value))
                        })
                        .collect::<Binding>();
                    for name in projections {
                        let marker = mapping_value_marker(name);
                        if let Some(value) = row.get(&marker) {
                            projected.insert(marker, value.clone());
                        }
                    }
                    projected
                })
                .collect());
        }
        if let Some(rows) = self.select_bgp_postgres_sql(patterns, projections)? {
            return Ok(rows);
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
                let mut projected = projections
                    .iter()
                    .filter_map(|name| row.get(name).cloned().map(|value| (name.clone(), value)))
                    .collect::<Binding>();
                for name in projections {
                    let marker = mapping_value_marker(name);
                    if let Some(value) = row.get(&marker) {
                        projected.insert(marker, value.clone());
                    }
                }
                projected
            })
            .collect())
    }

    /// #55 的最小 SQL 改写纵切。每个 triple 都先由 native mapping 改写为一个
    /// PostgreSQL 子查询，再以共享 SPARQL 变量的 RDF-term 列在数据库内 JOIN。
    ///
    /// 只有在不存在 facts/TBox 扩展、每个 triple 有唯一 mapping plan，且重复变量的
    /// term kind/base 一致时才进入该路径；否则由既有通用 evaluator 保持语义。这样不
    /// 会把受限 SQL 下推错误扩大为对 GRAPH、推理或混合事实图的支持声明。
    fn select_bgp_postgres_sql(
        &mut self,
        patterns: &[TriplePattern],
        projections: &[String],
    ) -> Result<Option<Vec<Binding>>, RuntimeError> {
        if !self.source.supports_postgres_bgp_pushdown()
            || !self.facts.is_empty()
            || self.spec.ontology_file.is_some()
        {
            return Ok(None);
        }
        let mut plans = Vec::with_capacity(patterns.len());
        for pattern in patterns {
            if pattern.predicate.starts_with('?') {
                return Ok(None);
            }
            let Ok(candidate_plans) = self.mapping.reformulate(pattern, &variables(pattern)) else {
                return Ok(None);
            };
            if candidate_plans.len() != 1 || candidate_plans[0].object_validation.is_some() {
                return Ok(None);
            }
            // 完全绑定的 triple plan 不投影 RDF term。最小纵切不把它与可投影
            // plan 混合成 SELECT 空列；此类 ASK/约束 BGP 继续走通用 evaluator。
            if candidate_plans[0].variables.is_empty() {
                return Ok(None);
            }
            plans.push(candidate_plans.into_iter().next().expect("single plan"));
        }

        let mut parameters = Vec::new();
        let mut from = String::new();
        let mut select_columns = Vec::new();
        let mut variable_order = Vec::new();
        let mut bindings: BTreeMap<String, (BindingTerm, Option<String>, String)> = BTreeMap::new();
        for (plan_index, plan) in plans.iter().enumerate() {
            let alias = format!("p{plan_index}");
            let columns = (0..plan.variables.len())
                .map(|index| format!("c{index}"))
                .collect::<Vec<_>>();
            let sql = renumber_postgres_parameters(&plan.sql, parameters.len());
            parameters.extend(plan.parameters.iter().cloned());
            let relation = format!("({sql}) AS {alias}({})", columns.join(", "));
            if plan_index == 0 {
                from.push_str(&relation);
            } else {
                let mut conditions = Vec::new();
                for (column_index, variable) in plan.variables.iter().enumerate() {
                    if let Some((term, iri_base, reference)) = bindings.get(variable) {
                        if term != &plan.terms[column_index]
                            || iri_base != &plan.iri_bases[column_index]
                        {
                            return Ok(None);
                        }
                        conditions.push(format!("{reference} = {alias}.{}", columns[column_index]));
                    }
                }
                from.push_str(&format!(
                    " JOIN {relation} ON {}",
                    if conditions.is_empty() {
                        "TRUE".into()
                    } else {
                        conditions.join(" AND ")
                    }
                ));
            }
            for (column_index, variable) in plan.variables.iter().enumerate() {
                let reference = format!("{alias}.{}", columns[column_index]);
                if !bindings.contains_key(variable) {
                    let output = format!("__rtop_bgp_{}", bindings.len());
                    select_columns.push(format!("{reference} AS {output}"));
                    variable_order.push(variable.clone());
                    bindings.insert(
                        variable.clone(),
                        (
                            plan.terms[column_index].clone(),
                            plan.iri_bases[column_index].clone(),
                            reference,
                        ),
                    );
                }
            }
        }
        let projected = projections
            .iter()
            .filter_map(|variable| bindings.get(variable).map(|_| variable))
            .collect::<Vec<_>>();
        let sql = format!("SELECT {} FROM {from}", select_columns.join(", "));
        let rows = self.source.execute_typed(&sql, &parameters)?;
        let mut decoded = Vec::with_capacity(rows.len());
        for row in rows {
            if row.iter().any(Option::is_none) {
                continue;
            }
            let mut binding = Binding::new();
            for (index, variable) in variable_order.iter().enumerate() {
                let (term, iri_base, _) = bindings.get(variable).expect("binding metadata");
                binding.insert(
                    variable.clone(),
                    decode_mapping_term(
                        row[index].as_ref().expect("NULL rows are filtered"),
                        term,
                        iri_base.as_deref(),
                        self.mapping_infer_default_datatype,
                        self.mapping_require_absolute_iri_values,
                        self.canonicalize_floating_lexicals,
                    )?,
                );
                binding.insert(
                    mapping_value_marker(variable),
                    boolean_term(true).expect("boolean literal"),
                );
            }
            let mut projected_binding = projected
                .iter()
                .filter_map(|variable| {
                    binding
                        .get(*variable)
                        .cloned()
                        .map(|term| ((*variable).clone(), term))
                })
                .collect::<Binding>();
            for variable in &projected {
                let marker = mapping_value_marker(variable);
                if let Some(value) = binding.get(&marker) {
                    projected_binding.insert(marker, value.clone());
                }
            }
            decoded.push(projected_binding);
        }
        Ok(Some(decoded))
    }

    fn evaluate_graph_pattern(
        &mut self,
        pattern: &GraphPattern,
        input: Vec<Binding>,
    ) -> Result<Vec<Binding>, RuntimeError> {
        match pattern {
            GraphPattern::Empty => Ok(input),
            GraphPattern::Bgp(patterns) => {
                // 顶层 SELECT 的空 binding BGP 与 ASK/CONSTRUCT 使用同一个执行
                // seam。这样可让可安全下推的 native mapping BGP 在 PostgreSQL 内
                // JOIN，同时保留后续带输入 binding 的 OPTIONAL/UNION 通用路径。
                if input.len() == 1 && input[0].is_empty() && patterns.len() > 1 {
                    let mut projections = Vec::new();
                    for triple in patterns {
                        for variable in variables(triple) {
                            if !projections.contains(&variable) {
                                projections.push(variable);
                            }
                        }
                    }
                    return self.select_bgp(patterns, &projections);
                }
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
            GraphPattern::ZeroOrMorePath(pattern) => {
                let rows = self.select_zero_or_more_path(pattern)?;
                Ok(join_binding_relations(input, rows))
            }
            GraphPattern::ZeroOrMoreSequencePath {
                pattern,
                predicates,
            } => {
                let rows = self.select_zero_or_more_sequence_path(pattern, predicates)?;
                Ok(join_binding_relations(input, rows))
            }
            GraphPattern::ZeroLengthPath(pattern) => {
                let rows = self.select_zero_length_path(pattern)?;
                Ok(join_binding_relations(input, rows))
            }
            GraphPattern::Join(left, right) => {
                let rows = self.evaluate_graph_pattern(left, input)?;
                self.evaluate_graph_pattern(right, rows)
            }
            GraphPattern::LeftJoin(left, right) => {
                let left_rows = self.evaluate_graph_pattern(left, input)?;
                let mut rows = Vec::new();
                for left in left_rows {
                    // 嵌套 OPTIONAL 的内层左操作数是独立 algebra group，不能把
                    // 外层 LeftJoin 已绑定、但该内层左操作数未引用的变量直接灌入。
                    // 否则 DAWG nested-opt-1 会将 ?v=1 提前带入 `?x2 :p ?v`，把
                    // 本应生成 ?v=2 后再与外层不兼容的分支误改写为右侧无匹配。
                    let mut right_input = left.clone();
                    let isolate_nested_optional = nested_optional_initial_variables(right);
                    if let Some(visible) = &isolate_nested_optional {
                        right_input.retain(|variable, _| visible.contains(variable));
                    }
                    let right_rows = match self.evaluate_graph_pattern(right, vec![right_input]) {
                        Ok(rows) => rows,
                        // 对 virtual graph 中没有 mapping rule 的 optional property，SPARQL
                        // 语义是空右侧而非整个查询失败。
                        Err(RuntimeError::NotFullyTranslatable(_)) => Vec::new(),
                        Err(error) => return Err(error),
                    };
                    let right_rows = if isolate_nested_optional.is_some() {
                        right_rows
                            .into_iter()
                            .filter_map(|right| join(&left, &right))
                            .collect()
                    } else {
                        right_rows
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
                for row in &mut rows {
                    for variable in graph_pattern_bind_variables(left) {
                        row.remove(&variable);
                    }
                }
                let mut right_rows = self.evaluate_graph_pattern(right, input)?;
                for row in &mut right_rows {
                    for variable in graph_pattern_bind_variables(right) {
                        row.remove(&variable);
                    }
                }
                rows.extend(right_rows);
                Ok(rows)
            }
            GraphPattern::Subquery {
                pattern,
                variables,
                aggregates,
                projection_binds,
                group_by,
                having,
                distinct,
                order_by,
                offset,
                limit,
            } => {
                let mut inner = self.evaluate_graph_pattern(pattern, vec![Binding::new()])?;
                if !aggregates.is_empty()
                    || !having.is_empty()
                    || projection_binds
                        .iter()
                        .any(|bind| expression_contains_aggregate(&bind.expression))
                    || order_by
                        .iter()
                        .any(|term| expression_contains_aggregate(&term.expression))
                {
                    inner = aggregate_bindings(
                        inner,
                        aggregates,
                        group_by,
                        projection_binds,
                        having,
                        order_by,
                        self.has_facts_input,
                    );
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
            GraphPattern::DatasetGraphBind {
                pattern,
                variable,
                graph,
            } => {
                let mut rows = self.evaluate_graph_pattern(pattern, input)?;
                for row in &mut rows {
                    row.insert(variable.clone(), RdfTerm::Iri(graph.clone()));
                }
                Ok(rows)
            }
            GraphPattern::Scoped { pattern, hidden } => {
                let mut output = Vec::new();
                for outer in input {
                    let mut inner_input = outer.clone();
                    for variable in hidden {
                        inner_input.remove(variable);
                    }
                    let inner = self.evaluate_graph_pattern(pattern, vec![inner_input])?;
                    output.extend(
                        inner
                            .into_iter()
                            .filter_map(|binding| join(&outer, &binding)),
                    );
                }
                Ok(output)
            }
            GraphPattern::Filter(pattern, filters) => {
                let mut rows = self.evaluate_graph_pattern(pattern, input)?;
                for filter in filters {
                    rows.retain(|row| matches_filter(row, filter));
                }
                Ok(rows)
            }
            GraphPattern::Exists {
                pattern,
                exists,
                negated,
            } => {
                let rows = self.evaluate_graph_pattern(pattern, input)?;
                let mut output = Vec::new();
                for row in rows {
                    let matches = match self.evaluate_graph_pattern(exists, vec![row.clone()]) {
                        Ok(matches) => !matches.is_empty(),
                        // 虚拟图中没有对应 mapping rule 的 EXISTS pattern 等价于空图，
                        // 而非令整个 outer query 出错。
                        Err(RuntimeError::NotFullyTranslatable(_)) => false,
                        Err(error) => return Err(error),
                    };
                    if matches != *negated {
                        output.push(row);
                    }
                }
                Ok(output)
            }
            GraphPattern::FilterOrExists {
                pattern,
                filter,
                exists,
                negated,
            } => {
                let rows = self.evaluate_graph_pattern(pattern, input)?;
                let mut output = Vec::new();
                for row in rows {
                    if matches_filter(&row, filter) {
                        output.push(row);
                        continue;
                    }
                    let matches = match self.evaluate_graph_pattern(exists, vec![row.clone()]) {
                        Ok(matches) => !matches.is_empty(),
                        Err(RuntimeError::NotFullyTranslatable(_)) => false,
                        Err(error) => return Err(error),
                    };
                    if matches != *negated {
                        output.push(row);
                    }
                }
                Ok(output)
            }
        }
    }

    /// 在已物化的一跳结果上计算 `predicate*`。每一个 (起点, 终点, 图) 只产生
    /// 一次 solution mapping；这是 SPARQL property path 的集合语义，而不是把环路
    /// 或平行边的可达路径数变成 bag 重数。外层 JOIN 仍保留其自身的 bag 语义。
    fn select_zero_or_more_path(
        &mut self,
        pattern: &TriplePattern,
    ) -> Result<Vec<Binding>, RuntimeError> {
        const START: &str = "__rtop_path_star_start";
        const END: &str = "__rtop_path_star_end";
        if let Some(graphs) = dataset_default_path_graphs(pattern) {
            let mut edges = Vec::new();
            for graph in &graphs {
                let edge_pattern = TriplePattern {
                    subject: format!("?{START}"),
                    predicate: pattern.predicate.clone(),
                    object: format!("?{END}"),
                    graph: Some(graph.clone()),
                };
                edges.extend(self.select(&edge_pattern, &[START.into(), END.into()])?);
            }
            return self.close_zero_or_more_path(pattern, edges, Some(&graphs));
        }
        let edge_pattern = TriplePattern {
            subject: format!("?{START}"),
            predicate: pattern.predicate.clone(),
            object: format!("?{END}"),
            graph: pattern.graph.clone(),
        };
        let graph_variable = pattern
            .graph
            .as_deref()
            .and_then(|graph| graph.strip_prefix('?'));
        let mut projections = vec![START.into(), END.into()];
        if let Some(variable) = graph_variable {
            projections.push(variable.into());
        }
        let edges = self.select(&edge_pattern, &projections)?;
        self.close_zero_or_more_path(pattern, edges, None)
    }

    /// 将 sequence 的一跳关系独立物化后交给单一闭包实现。这样平行 sequence
    /// 实例不会改变 path relation 的 set 语义，且不会泄漏中间辅助变量。
    fn select_zero_or_more_sequence_path(
        &mut self,
        pattern: &TriplePattern,
        predicates: &[String],
    ) -> Result<Vec<Binding>, RuntimeError> {
        const START: &str = "__rtop_path_star_start";
        const END: &str = "__rtop_path_star_end";
        let dataset_graphs = dataset_default_path_graphs(pattern);
        // 不能借用 BGP 的 PostgreSQL fast path：它以 mapping SQL 的完整 join 为
        // 优化目标，而 sequence 一次关系还必须合并 facts 输入。逐段 select 后以
        // 普通 solution join 组合。FROM 默认图的每一段均先 merge 所列图，故
        // sequence 可以跨图连接，而不是要求整条 sequence 落在同一个具名图。
        let mut edges = vec![Binding::new()];
        let mut subject = format!("?{START}");
        for (index, predicate) in predicates.iter().enumerate() {
            let object = if index + 1 == predicates.len() {
                format!("?{END}")
            } else {
                format!("?__rtop_path_star_sequence_{index}")
            };
            let triple = TriplePattern {
                subject,
                predicate: predicate.clone(),
                object: object.clone(),
                graph: pattern.graph.clone(),
            };
            let projections = variables(&triple);
            let matches = if let Some(graphs) = &dataset_graphs {
                let mut matches = Vec::new();
                for graph in graphs {
                    let mut graph_triple = triple.clone();
                    graph_triple.graph = Some(graph.clone());
                    matches.extend(self.select(&graph_triple, &projections)?);
                }
                matches
            } else {
                self.select(&triple, &projections)?
            };
            edges = join_binding_relations(edges, matches);
            subject = object;
        }
        self.close_zero_or_more_path(pattern, edges, dataset_graphs.as_deref())
    }

    fn close_zero_or_more_path(
        &mut self,
        pattern: &TriplePattern,
        edges: Vec<Binding>,
        dataset_default_graphs: Option<&[String]>,
    ) -> Result<Vec<Binding>, RuntimeError> {
        const START: &str = "__rtop_path_star_start";
        const END: &str = "__rtop_path_star_end";
        const NODE_SUBJECT: &str = "__rtop_path_star_node_subject";
        const NODE_PREDICATE: &str = "__rtop_path_star_node_predicate";
        const NODE_OBJECT: &str = "__rtop_path_star_node_object";
        let graph_variable = if dataset_default_graphs.is_some() {
            None
        } else {
            pattern
                .graph
                .as_deref()
                .and_then(|graph| graph.strip_prefix('?'))
        };
        let mut graphs: BTreeMap<
            Option<RdfTerm>,
            (BTreeSet<RdfTerm>, BTreeMap<RdfTerm, BTreeSet<RdfTerm>>),
        > = BTreeMap::new();
        for edge in edges {
            let Some(start) = edge.get(START).cloned() else {
                continue;
            };
            let Some(end) = edge.get(END).cloned() else {
                continue;
            };
            let graph = graph_variable.and_then(|variable| edge.get(variable).cloned());
            let (nodes, adjacency) = graphs.entry(graph).or_default();
            nodes.insert(start.clone());
            nodes.insert(end.clone());
            adjacency.entry(start).or_default().insert(end);
        }
        // `p*` 的零长度 identity 域是活动图的全部 RDF terms，不能只从 p 边
        // 推导节点。固定 DAWG pp16 的 :h 和 "test" 分别只出现于其他 predicate
        // 的 object，却仍必须产生 (?x, ?x) 结果。
        let node_pattern = TriplePattern {
            subject: format!("?{NODE_SUBJECT}"),
            predicate: format!("?{NODE_PREDICATE}"),
            object: format!("?{NODE_OBJECT}"),
            graph: pattern.graph.clone(),
        };
        let mut node_projections = vec![NODE_SUBJECT.into(), NODE_OBJECT.into()];
        if let Some(variable) = graph_variable {
            node_projections.push(variable.into());
        }
        let node_patterns = dataset_default_graphs
            .map(|graphs| graphs.iter().cloned().map(Some).collect())
            .unwrap_or_else(|| vec![node_pattern.graph.clone()]);
        for graph in node_patterns {
            let mut node_pattern = node_pattern.clone();
            node_pattern.graph = graph;
            for node in self.select(&node_pattern, &node_projections)? {
                let graph = graph_variable.and_then(|variable| node.get(variable).cloned());
                let (nodes, _) = graphs.entry(graph).or_default();
                if let Some(subject) = node.get(NODE_SUBJECT) {
                    nodes.insert(subject.clone());
                }
                if let Some(object) = node.get(NODE_OBJECT) {
                    nodes.insert(object.clone());
                }
            }
        }

        let mut output = BTreeSet::new();
        for (graph, (nodes, adjacency)) in graphs {
            for start in &nodes {
                let mut reachable = BTreeSet::from([start.clone()]);
                let mut queue = VecDeque::from([start.clone()]);
                while let Some(current) = queue.pop_front() {
                    if let Some(next) = adjacency.get(&current) {
                        for end in next {
                            if reachable.insert(end.clone()) {
                                queue.push_back(end.clone());
                            }
                        }
                    }
                }
                for end in reachable {
                    let mut binding = Binding::new();
                    if !bind_path_term(&mut binding, &pattern.subject, start)
                        || !bind_path_term(&mut binding, &pattern.object, &end)
                    {
                        continue;
                    }
                    if let Some(variable) = graph_variable {
                        let Some(graph) = graph.as_ref() else {
                            continue;
                        };
                        if !bind_path_term(&mut binding, &format!("?{variable}"), graph) {
                            continue;
                        }
                    }
                    output.insert(binding);
                }
            }
        }
        Ok(output.into_iter().collect())
    }

    fn select_zero_length_path(
        &mut self,
        pattern: &TriplePattern,
    ) -> Result<Vec<Binding>, RuntimeError> {
        // 复用 `p*` 的 node-domain 收集，但用一个不可能出现在用户 facts 的
        // predicate 令闭包只含零长度 identity；这也保持 named graph 的边界。
        let identity = TriplePattern {
            predicate: "<urn:rtop:zero-length-sentinel>".into(),
            ..pattern.clone()
        };
        let mut rows = self.select_zero_or_more_path(&identity)?;
        if let Some(term) =
            zero_length_constant(&pattern.subject).or_else(|| zero_length_constant(&pattern.object))
        {
            let mut binding = Binding::new();
            if bind_path_term(&mut binding, &pattern.subject, &term)
                && bind_path_term(&mut binding, &pattern.object, &term)
            {
                rows.push(binding);
            }
        }
        rows.sort();
        rows.dedup();
        Ok(rows)
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
        let is_intersection = matches!(
            name.trim().to_ascii_uppercase().as_str(),
            "GEOF:INTERSECTION" | "<HTTP://WWW.OPENGIS.NET/DEF/FUNCTION/GEOSPARQL/INTERSECTION>"
        );
        let value = if is_intersection {
            let buffered_intersection = arguments
                .first()
                .and_then(|wkt| self.buffered_wkts.get(wkt))
                .map(|(buffer_wkt, distance)| {
                    self.source.geospatial_intersection_with_buffer(
                        buffer_wkt,
                        distance,
                        arguments
                            .get(1)
                            .expect("intersection arity checked by adapter"),
                    )
                })
                .transpose()?
                .flatten();
            match buffered_intersection {
                Some(value) => Some(value),
                None => self.source.geospatial(name, &arguments)?,
            }
        } else {
            self.source.geospatial(name, &arguments)?
        };
        Ok(value.map(|value| match value {
            GeospatialValue::Boolean(value) => boolean_term(value).expect("boolean term"),
            GeospatialValue::Wkt(value) => {
                if matches!(
                    name.trim().to_ascii_uppercase().as_str(),
                    "GEOF:BUFFER" | "<HTTP://WWW.OPENGIS.NET/DEF/FUNCTION/GEOSPARQL/BUFFER>"
                ) && arguments.len() == 3
                {
                    self.buffered_wkts
                        .insert(value.clone(), (arguments[0].clone(), arguments[1].clone()));
                }
                RdfTerm::Literal {
                    value,
                    datatype: Some("http://www.opengis.net/ont/geosparql#wktLiteral".into()),
                    language: None,
                }
            }
        }))
    }

    pub fn spec(&self) -> &KnowledgeGraphSpec {
        &self.spec
    }
}

/// 对一个作为 OPTIONAL 右操作数的 nested LeftJoin，返回进入其最外层左操作数时
/// 可见的变量。普通 OPTIONAL 仍保留相关 FILTER 的 outer binding；只有嵌套
/// LeftJoin 才需要这个 algebra 边界。
fn nested_optional_initial_variables(pattern: &GraphPattern) -> Option<Vec<String>> {
    match pattern {
        GraphPattern::LeftJoin(left, _) => Some(graph_pattern_variables(left)),
        GraphPattern::Filter(pattern, _)
        | GraphPattern::Bind(pattern, _)
        | GraphPattern::Scoped { pattern, .. } => nested_optional_initial_variables(pattern),
        _ => None,
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
        preserve_physical_rows: bool,
    ) -> Result<Self, RuntimeError> {
        let metadata = relations
            .iter()
            .map(|relation| {
                source
                    .relation_metadata(relation)
                    .map(|metadata| (relation.clone(), metadata))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mapping = Mapping::from_direct_mapping(base_iri, &metadata, preserve_physical_rows)?;
        Self::from_mapping(spec, source, mapping, true, false, true)
    }
}

fn sort_bindings_by(rows: &mut [Binding], order_by: &[OrderByTerm]) {
    use std::cmp::Ordering;
    if order_by.is_empty() {
        return;
    }
    rows.sort_by(|left, right| {
        for term in order_by {
            let left_value = evaluate_expression(left, &term.expression);
            let right_value = evaluate_expression(right, &term.expression);
            let ordering = match (&left_value, &right_value) {
                (Some(left), Some(right)) => sparql_value_ordering(left, right)
                    .unwrap_or_else(|| sparql_term_ordering(left, right)),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
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

/// SPARQL 对无法按数值、时间等 value space 比较的 RDF terms 仍定义稳定的
/// ORDER BY 类别顺序：blank node、IRI、literal。同类退回其完整 RDF 序列化，
/// 保留既有的语言、datatype 与 lexical 区分。
fn sparql_term_ordering(left: &RdfTerm, right: &RdfTerm) -> std::cmp::Ordering {
    let category = |term: &RdfTerm| match term {
        RdfTerm::BlankNode(_) => 0,
        RdfTerm::Iri(_) => 1,
        RdfTerm::Literal { .. } => 2,
    };
    category(left)
        .cmp(&category(right))
        .then_with(|| format_rdf_term(left).cmp(&format_rdf_term(right)))
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
fn materialize_group_bindings(rows: Vec<Binding>, group_by: &[GroupBy]) -> Vec<Binding> {
    rows.into_iter()
        .filter_map(|mut row| {
            for group in group_by {
                if let GroupBy::Expression {
                    expression,
                    variable,
                } = group
                {
                    row.insert(variable.clone(), evaluate_expression(&row, expression)?);
                }
            }
            Some(row)
        })
        .collect()
}

fn group_variable_names(group_by: &[GroupBy]) -> Vec<String> {
    group_by
        .iter()
        .map(|group| match group {
            GroupBy::Variable(variable) => variable.clone(),
            GroupBy::Expression { variable, .. } => variable.clone(),
        })
        .collect()
}

fn group_binding_representatives(rows: Vec<Binding>, group_by: &[GroupBy]) -> Vec<Binding> {
    let rows = materialize_group_bindings(rows, group_by);
    let group_by = group_variable_names(group_by);
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
        Expression::In {
            value, candidates, ..
        } => {
            expression_contains_aggregate(value)
                || candidates.iter().any(expression_contains_aggregate)
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
    group_by: &[GroupBy],
    projection_binds: &[Bind],
    having: &[Expression],
    order_by: &[OrderByTerm],
    canonical_facts_avg_lexical: bool,
) -> Vec<Binding> {
    let rows = materialize_group_bindings(rows, group_by);
    let group_by = group_variable_names(group_by);
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
    let mut aggregated =
        groups
            .into_iter()
            .filter_map(|(mut key, members)| {
                for aggregate in aggregates {
                    if let Some(value) =
                        evaluate_aggregate(&members, aggregate, canonical_facts_avg_lexical)
                            .or_else(|| {
                                aggregate.fallback.as_ref().and_then(|fallback| {
                                    evaluate_expression(&Binding::new(), fallback)
                                })
                            })
                    {
                        key.insert(aggregate.variable.clone(), value);
                    }
                }
                for bind in projection_binds {
                    if let Some(value) = evaluate_expression_with_aggregates(
                        &key,
                        &bind.expression,
                        &members,
                        canonical_facts_avg_lexical,
                    ) {
                        key.insert(bind.variable.clone(), value);
                    }
                }
                having
                    .iter()
                    .all(|expression| {
                        let mut bindings = key.clone();
                        let mut next = 0usize;
                        materialize_aggregates(
                            expression,
                            &members,
                            &mut bindings,
                            &mut next,
                            canonical_facts_avg_lexical,
                        )
                        .and_then(|expression| expression_boolean(&bindings, &expression))
                            == Some(true)
                    })
                    .then_some((key, members))
            })
            .collect::<Vec<_>>();
    // 聚合只在 group 的成员集合上有值；若等到调用方的普通排序器再处理，
    // `COUNT(?x)` 会成为未绑定 expression，导致 LIMIT 取到输入首组。
    aggregated.sort_by(|(left, left_members), (right, right_members)| {
        use std::cmp::Ordering;
        for term in order_by {
            let left_value = evaluate_expression_with_aggregates(
                left,
                &term.expression,
                left_members,
                canonical_facts_avg_lexical,
            );
            let right_value = evaluate_expression_with_aggregates(
                right,
                &term.expression,
                right_members,
                canonical_facts_avg_lexical,
            );
            let ordering = match (&left_value, &right_value) {
                (Some(left), Some(right)) => sparql_value_ordering(left, right)
                    .unwrap_or_else(|| sparql_term_ordering(left, right)),
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            };
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
    aggregated.into_iter().map(|(key, _)| key).collect()
}

fn evaluate_expression_with_aggregates(
    row: &Binding,
    expression: &Expression,
    members: &[Binding],
    canonical_facts_avg_lexical: bool,
) -> Option<RdfTerm> {
    let mut bindings = row.clone();
    let mut next = 0usize;
    let expression = materialize_aggregates(
        expression,
        members,
        &mut bindings,
        &mut next,
        canonical_facts_avg_lexical,
    )?;
    evaluate_expression(&bindings, &expression)
}

fn materialize_aggregates(
    expression: &Expression,
    members: &[Binding],
    bindings: &mut Binding,
    next: &mut usize,
    canonical_facts_avg_lexical: bool,
) -> Option<Expression> {
    let recurse = |expression, bindings: &mut Binding, next: &mut usize| {
        materialize_aggregates(
            expression,
            members,
            bindings,
            next,
            canonical_facts_avg_lexical,
        )
    };
    Some(match expression {
        Expression::Aggregate {
            kind,
            expression,
            count_all,
            distinct,
        } => {
            let aggregate = Aggregate {
                variable: String::new(),
                kind: *kind,
                expression: (*expression.clone()),
                count_all: *count_all,
                distinct: *distinct,
                separator: None,
                fallback: None,
            };
            let value = evaluate_aggregate(members, &aggregate, canonical_facts_avg_lexical)?;
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
        Expression::In {
            value,
            candidates,
            negated,
        } => Expression::In {
            value: Box::new(recurse(value, bindings, next)?),
            candidates: candidates
                .iter()
                .map(|candidate| recurse(candidate, bindings, next))
                .collect::<Option<Vec<_>>>()?,
            negated: *negated,
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
                        ..
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

fn evaluate_aggregate(
    rows: &[Binding],
    aggregate: &Aggregate,
    canonical_facts_avg_lexical: bool,
) -> Option<RdfTerm> {
    if matches!(aggregate.kind, AggregateKind::Count) && aggregate.count_all {
        return Some(RdfTerm::Literal {
            value: rows.len().to_string(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        });
    }
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
        // SPARQL SAMPLE 可从 group 中任选一个非 error value；固定输入顺序的 runtime
        // 选择第一个，保持可重放，同时不把该实现选择承诺为跨环境排序规则。
        AggregateKind::Sample => values.into_iter().next(),
        AggregateKind::Min | AggregateKind::Max => {
            // 当前 Rust 聚合层只实现了数值 MIN/MAX 排序。与 SUM/AVG 一致，已绑定
            // 的非数值 term 是 aggregate expression error，不能被静默跳过后把其余
            // 数值错误地投影出来（DAWG agg-err-01 的 blank node group）。
            if values
                .iter()
                .any(|value| term_numeric_value(value).is_none())
            {
                return None;
            }
            values
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
                .map(|(_, term)| {
                    if canonical_facts_avg_lexical {
                        canonicalize_facts_floating_term(term)
                    } else {
                        term
                    }
                })
        }
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
            // 精确 decimal AVG 不能经由 f64：除整数得到的结果仍是 xsd:decimal，
            // 且 DAWG 的 `2.0` lexical 是可观察结果，不能被浮点 serializer 缩为 `2`。
            // 含 float/double 时仍走下方路径，以保持 SPARQL 的数值提升规则。
            if matches!(aggregate.kind, AggregateKind::Avg) {
                let exact = values
                    .iter()
                    .map(exact_decimal_from_term)
                    .collect::<Option<Vec<_>>>();
                if let Some(exact) = exact {
                    let sum = exact
                        .into_iter()
                        .map(|(value, _)| value)
                        .fold(BigDecimal::zero(), |sum, value| sum + value);
                    return exact_average_result_term(
                        sum / BigDecimal::from(numeric.len() as u64),
                        canonical_facts_avg_lexical,
                    );
                }
            }
            // SUM 直接消费 PostgreSQL numeric 时也必须避开 f64。否则聚合前/后
            // ROUND 的值会受二进制误差影响，即使单行 BIND 已经是精确 decimal。
            if matches!(aggregate.kind, AggregateKind::Sum) {
                let exact = values
                    .iter()
                    .map(exact_decimal_from_term)
                    .collect::<Option<Vec<_>>>();
                if let Some(exact) = exact {
                    let all_integers = exact
                        .iter()
                        .all(|(_, datatype)| is_integer_datatype(datatype));
                    let sum = exact
                        .into_iter()
                        .map(|(value, _)| value)
                        .fold(BigDecimal::zero(), |sum, value| sum + value);
                    return exact_numeric_result_term(
                        sum,
                        if all_integers {
                            "http://www.w3.org/2001/XMLSchema#integer"
                        } else {
                            "http://www.w3.org/2001/XMLSchema#decimal"
                        },
                    );
                }
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
            let datatype = numeric.iter().fold(
                "http://www.w3.org/2001/XMLSchema#decimal",
                |promoted, (_, datatype)| promoted_numeric_datatype(promoted, datatype),
            );
            numeric_result_term(value, datatype)
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
        Expression::LanguageLiteral { value, language } => Some(RdfTerm::Literal {
            value: value.clone(),
            datatype: None,
            language: Some(language.clone()),
        }),
        Expression::TypedLiteral { value, datatype } => Some(RdfTerm::Literal {
            value: value.clone(),
            datatype: Some(datatype.clone()),
            language: None,
        }),
        Expression::Iri(value) => Some(RdfTerm::Iri(value.clone())),
        Expression::Number(value) => Some(RdfTerm::Literal {
            value: value.clone(),
            datatype: Some(
                value
                    .parse::<i64>()
                    .map(|_| "http://www.w3.org/2001/XMLSchema#integer")
                    .unwrap_or("http://www.w3.org/2001/XMLSchema#decimal")
                    .into(),
            ),
            language: None,
        }),
        Expression::Aggregate { .. } => None,
        Expression::Binary {
            operator,
            left,
            right,
        } => {
            // PostgreSQL numeric 与 xsd:decimal 不能先落到 f64：金额的 0.1 +
            // 0.2、边界 2.5 及长小数会在二进制浮点中失真。float/double 则仍由
            // 既有 IEEE 754 路径处理。
            if let (Some((left, left_datatype)), Some((right, right_datatype))) = (
                exact_decimal_value(row, left),
                exact_decimal_value(row, right),
            ) {
                let value = match operator {
                    ArithmeticOperator::Add => left + right,
                    ArithmeticOperator::Subtract => left - right,
                    ArithmeticOperator::Multiply => left * right,
                    ArithmeticOperator::Divide if !right.is_zero() => left / right,
                    ArithmeticOperator::Divide => return None,
                };
                let result_datatype =
                    arithmetic_result_datatype(*operator, &left_datatype, &right_datatype);
                let mut result = exact_numeric_result_term(value, result_datatype)?;
                // DAWG coalesce01 的 integer / integer 除法具有 xsd:decimal
                // value space，且 SRX 保留至少一位小数（0.0、2.0）。
                if matches!(operator, ArithmeticOperator::Divide)
                    && result_datatype == "http://www.w3.org/2001/XMLSchema#decimal"
                    && matches!(&result, RdfTerm::Literal { value, .. } if !value.contains('.'))
                {
                    let RdfTerm::Literal { value, .. } = &mut result else {
                        unreachable!()
                    };
                    value.push_str(".0");
                }
                return Some(result);
            }
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
                ComparisonOperator::Equal => {
                    sparql_value_equal_defined(row, left, right, &left_value, &right_value)?
                }
                ComparisonOperator::NotEqual => {
                    !sparql_value_equal_defined(row, left, right, &left_value, &right_value)?
                }
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
        Expression::In {
            value,
            candidates,
            negated,
        } => {
            let value = evaluate_expression(row, value)?;
            let mut expression_error = false;
            let mut matched = false;
            for candidate in candidates {
                match evaluate_expression(row, candidate) {
                    Some(candidate) if sparql_value_equal(&value, &candidate) => {
                        // 固定 Ontop PostgreSQL endpoint 会继续计算 IN 列表的剩余
                        // expression；后续的错误不能被前面的匹配短路吞掉。
                        matched = true;
                    }
                    Some(_) => {}
                    None => expression_error = true,
                }
            }
            (!expression_error).then(|| boolean_term(if *negated { !matched } else { matched }))?
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
            let (string, datatype, language) = string_literal_argument(row, value)?;
            let pattern = expression_string(row, pattern)?;
            let replacement = expression_string(row, replacement)?;
            let regex = Regex::new(&pattern).ok()?;
            Some(RdfTerm::Literal {
                value: regex
                    .replace_all(&string, replacement.as_str())
                    .into_owned(),
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
                value: left_value,
                language: Some(left_language),
                ..
            },
            RdfTerm::Literal {
                value: right_value,
                language: Some(right_language),
                ..
            },
        ) => left_value == right_value && left_language.eq_ignore_ascii_case(right_language),
        _ => left == right,
    }
}

/// SPARQL 的 `=`/`!=` 先按数值与 XSD temporal value space 比较；其余 RDF term
/// 保留 term equality。这让 PostgreSQL integer/numeric/date/timestamp 的自然 datatype
/// 能与 query 中的 xsd:decimal/date/dateTime 常量正确比较。
fn sparql_value_equal(left: &RdfTerm, right: &RdfTerm) -> bool {
    sparql_value_ordering(left, right).is_some_and(std::cmp::Ordering::is_eq)
        || sparql_term_equal(left, right)
}

/// `=` 与 `!=` 遇到不能解释 value space 的 datatype 时会产生 expression error，
/// 而非把不同 lexical form 当作可确定的不等。相同 RDF term 仍可确定相等；XSD
/// namespace 内的 datatype 则交给既有 value/term equality 规则处理。
fn sparql_value_equal_defined(
    row: &Binding,
    left_expression: &Expression,
    right_expression: &Expression,
    left: &RdfTerm,
    right: &RdfTerm,
) -> Option<bool> {
    // 固定 Ontop PostgreSQL 基线将相同 language tag（忽略大小写）的 language
    // literal 比较下推为 RDFTermEqual：即使 lexical form 不同，普通 `=` 仍为
    // true、`!=` 为 false。此处刻意只影响 comparison；sameTerm 继续使用严格
    // RDF term identity，STR 也会先移除 language metadata 再比较。
    if expression_is_mapping_value(row, left_expression)
        && expression_is_mapping_value(row, right_expression)
        && matches!(
            (left, right),
            (
                RdfTerm::Literal { language: Some(left_language), .. },
                RdfTerm::Literal { language: Some(right_language), .. },
            ) if left_language.eq_ignore_ascii_case(right_language)
        )
    {
        return Some(true);
    }
    if sparql_term_equal(left, right) {
        return Some(true);
    }
    // Literal 与 IRI/blank node 是可判定互异的 RDF term category；language literal
    // 也与不带 language 的 literal 互异。这些情形不需要解释 custom datatype
    // 或 ill-formed XSD lexical，因而 `!=` 应能成立而不是产生 error。
    match (left, right) {
        (RdfTerm::Literal { .. }, RdfTerm::Iri(_) | RdfTerm::BlankNode(_))
        | (RdfTerm::Iri(_) | RdfTerm::BlankNode(_), RdfTerm::Literal { .. }) => {
            return Some(false);
        }
        (
            RdfTerm::Literal {
                language: Some(_), ..
            },
            RdfTerm::Literal { language: None, .. },
        )
        | (
            RdfTerm::Literal { language: None, .. },
            RdfTerm::Literal {
                language: Some(_), ..
            },
        ) => return Some(false),
        _ => {}
    }
    let has_unknown_datatype = |term: &RdfTerm| {
        matches!(term, RdfTerm::Literal { datatype: Some(datatype), .. }
            if !datatype.starts_with("http://www.w3.org/2001/XMLSchema#")
                && datatype != "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString")
    };
    if has_unknown_datatype(left) || has_unknown_datatype(right) {
        return None;
    }
    let has_ill_formed_xsd_literal = |term: &RdfTerm| {
        matches!(term, RdfTerm::Literal { value, datatype: Some(datatype), language: None }
            if is_numeric_datatype(datatype) && BigDecimal::from_str(value).is_err())
    };
    (!has_ill_formed_xsd_literal(left) && !has_ill_formed_xsd_literal(right))
        .then(|| sparql_value_equal(left, right))
}

fn expression_is_mapping_value(row: &Binding, expression: &Expression) -> bool {
    matches!(expression, Expression::Variable(variable) if row.contains_key(&mapping_value_marker(variable)))
}

fn sparql_value_ordering(left: &RdfTerm, right: &RdfTerm) -> Option<std::cmp::Ordering> {
    if let (Some(left), Some(right)) = (numeric_term(left), numeric_term(right)) {
        return left.partial_cmp(&right);
    }
    if let (
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
    ) = (left, right)
    {
        let string_datatype = |datatype: &Option<String>| {
            datatype
                .as_deref()
                .is_none_or(|datatype| datatype == "http://www.w3.org/2001/XMLSchema#string")
        };
        if string_datatype(left_datatype) && string_datatype(right_datatype) {
            return Some(left_value.cmp(right_value));
        }
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
    (left_datatype == right_datatype
        && matches!(
            left_datatype.as_str(),
            "http://www.w3.org/2001/XMLSchema#date"
                | "http://www.w3.org/2001/XMLSchema#dateTime"
                | "http://www.w3.org/2001/XMLSchema#dateTimeStamp"
        ))
    .then(|| temporal_datetime(left_value)?.partial_cmp(&temporal_datetime(right_value)?))?
}

fn expression_boolean(row: &Binding, expression: &Expression) -> Option<bool> {
    let term = evaluate_expression(row, expression)?;
    match &term {
        RdfTerm::Literal {
            value,
            datatype: Some(datatype),
            language: None,
        } if datatype == "http://www.w3.org/2001/XMLSchema#boolean" => match value.as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        RdfTerm::Literal {
            value, datatype, ..
        } if datatype
            .as_deref()
            .is_none_or(|datatype| datatype == "http://www.w3.org/2001/XMLSchema#string") =>
        {
            Some(!value.is_empty())
        }
        _ => term_numeric_value(&term).map(|(number, _)| number != 0.0 && !number.is_nan()),
    }
}

fn string_term(value: &str) -> Option<RdfTerm> {
    Some(RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
        language: None,
    })
}

fn plain_string_term(value: &str) -> Option<RdfTerm> {
    Some(RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    })
}

/// CONCAT 的结果不是一律 `xsd:string`：全为同一语言标签时保留语言标签，
/// 全为显式 xsd:string 时保留 datatype，其他有效字符串组合返回 simple literal。
fn concat_terms(row: &Binding, arguments: &[Expression]) -> Option<RdfTerm> {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    let literals = arguments
        .iter()
        .map(|argument| match evaluate_expression(row, argument)? {
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } if language.is_some()
                || datatype.as_deref().is_none_or(|kind| kind == XSD_STRING) =>
            {
                Some((value, datatype, language))
            }
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let all_xsd_string = literals.iter().all(|(_, datatype, language)| {
        datatype.as_deref() == Some(XSD_STRING) && language.is_none()
    });
    let language = literals
        .first()
        .and_then(|(_, _, language)| language.clone());
    let common_language = language.is_some()
        && literals
            .iter()
            .all(|(_, _, candidate)| candidate.as_ref() == language.as_ref());
    Some(RdfTerm::Literal {
        value: literals.into_iter().map(|(value, _, _)| value).collect(),
        datatype: all_xsd_string.then(|| XSD_STRING.into()),
        language: common_language.then(|| language.unwrap()),
    })
}

/// 取得可用于 SPARQL 字符串函数的 literal。数值、IRI、blank node 以及其他
/// datatype 都是 expression error，而不是把词法形式偷偷转换为字符串。
fn string_literal_argument(
    row: &Binding,
    expression: &Expression,
) -> Option<(String, Option<String>, Option<String>)> {
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    match evaluate_expression(row, expression)? {
        RdfTerm::Literal {
            value,
            datatype,
            language,
        } if language.is_some() || datatype.as_deref().is_none_or(|kind| kind == XSD_STRING) => {
            Some((value, datatype, language))
        }
        _ => None,
    }
}

/// STRBEFORE/STRAFTER 要求第二个字符串没有语言标签，或与第一个参数标签相同。
/// 这也使 data4.ttl 中故意给出的 `@cy` 不匹配参数保留为未绑定变量。
fn compatible_string_arguments(
    row: &Binding,
    arguments: &[Expression],
) -> Option<(String, Option<String>, Option<String>)> {
    let first = string_literal_argument(row, &arguments[0])?;
    let (_, _, second_language) = string_literal_argument(row, &arguments[1])?;
    (second_language.is_none() || second_language == first.2).then_some(first)
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
            Some(RdfTerm::Iri(if language.is_some() {
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into()
            } else {
                datatype.unwrap_or_else(|| "http://www.w3.org/2001/XMLSchema#string".into())
            }))
        }
        // STRDT/STRLANG 只接受 simple literal 或 xsd:string；语言 literal、数值、
        // IRI 与 blank node 均为 expression error。不能复用 STR() 的宽松转换，
        // 否则会错误绑定 DAWG strdt01/03 与 strlang01/03 的 error rows。
        "STRDT" if arguments.len() == 2 => {
            const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
            let RdfTerm::Literal {
                value,
                datatype,
                language: None,
            } = evaluate_expression(row, &arguments[0])?
            else {
                return None;
            };
            if datatype
                .as_deref()
                .is_some_and(|datatype| datatype != XSD_STRING)
            {
                return None;
            }
            let RdfTerm::Iri(datatype) = evaluate_expression(row, &arguments[1])? else {
                return None;
            };
            Some(RdfTerm::Literal {
                value,
                datatype: Some(datatype),
                language: None,
            })
        }
        "STRLANG" if arguments.len() == 2 => {
            const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
            let RdfTerm::Literal {
                value,
                datatype,
                language: None,
            } = evaluate_expression(row, &arguments[0])?
            else {
                return None;
            };
            if datatype
                .as_deref()
                .is_some_and(|datatype| datatype != XSD_STRING)
            {
                return None;
            }
            let RdfTerm::Literal {
                value: language,
                datatype: language_datatype,
                language: None,
            } = evaluate_expression(row, &arguments[1])?
            else {
                return None;
            };
            if language_datatype
                .as_deref()
                .is_some_and(|datatype| datatype != XSD_STRING)
                || language.is_empty()
                || !language
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return None;
            }
            Some(RdfTerm::Literal {
                value,
                datatype: None,
                language: Some(language.to_ascii_lowercase()),
            })
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
        "ABS" if arguments.len() == 1 => exact_decimal_value(row, &arguments[0])
            .and_then(|(value, datatype)| canonical_decimal_function_term(value.abs(), &datatype))
            .or_else(|| decimal_term(numeric(row, &arguments[0])?.abs())),
        "CEIL" if arguments.len() == 1 => exact_decimal_value(row, &arguments[0])
            .and_then(|(value, datatype)| {
                exact_numeric_result_term(
                    value.with_scale_round(0, RoundingMode::Ceiling),
                    &datatype,
                )
            })
            .or_else(|| decimal_term(numeric(row, &arguments[0])?.ceil())),
        "FLOOR" if arguments.len() == 1 => exact_decimal_value(row, &arguments[0])
            .and_then(|(value, datatype)| {
                exact_numeric_result_term(value.with_scale_round(0, RoundingMode::Floor), &datatype)
            })
            .or_else(|| decimal_term(numeric(row, &arguments[0])?.floor())),
        "ROUND" if arguments.len() == 1 => exact_decimal_value(row, &arguments[0])
            .and_then(|(value, datatype)| {
                // 固定 Ontop PostgreSQL endpoint 把 decimal 半值远离零：2.5 -> 3，
                // -2.5 -> -3。此处服从可观察基线，而不是采用其他 SPARQL 实现的规则。
                exact_numeric_result_term(
                    value.with_scale_round(0, RoundingMode::HalfUp),
                    &datatype,
                )
            })
            .or_else(|| decimal_term(numeric(row, &arguments[0])?.round())),
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
                .map(|timezone| {
                    (timezone == "+00:00")
                        .then_some("Z".into())
                        .unwrap_or_else(|| timezone.trim_start_matches('+').to_owned())
                })
                .unwrap_or_default();
            // DAWG tz-01.srx 指定 TZ() 为 simple literal，不是 STR() 所返回的
            // xsd:string。
            plain_string_term(&timezone)
        }
        "TIMEZONE" if arguments.len() == 1 => {
            let value = expression_string(row, &arguments[0])?;
            let seconds = chrono::DateTime::parse_from_rfc3339(&value)
                .ok()?
                .offset()
                .local_minus_utc();
            Some(RdfTerm::Literal {
                value: day_time_duration_lexical(seconds),
                datatype: Some("http://www.w3.org/2001/XMLSchema#dayTimeDuration".into()),
                language: None,
            })
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
        // DAWG rand01.srx 要求 RAND() 的结果为 xsd:double，而不是 decimal。
        "RAND" if arguments.is_empty() => {
            numeric_result_term(random::<f64>(), "http://www.w3.org/2001/XMLSchema#double")
        }
        "SHA256" if arguments.len() == 1 => {
            let digest = Sha256::digest(string(0)?.as_bytes());
            // DAWG sha256-01/02 指定 hash 函数返回 simple literal，而不是
            // xsd:string；这与 STR() 的显式字符串结果不同。
            plain_string_term(&format!("{digest:x}"))
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
        "ENCODE_FOR_URI" if arguments.len() == 1 => plain_string_term(&encode_for_uri(&string(0)?)),
        "UCASE" if arguments.len() == 1 => map_literal_case(row, &arguments[0], str::to_uppercase),
        "LCASE" if arguments.len() == 1 => map_literal_case(row, &arguments[0], str::to_lowercase),
        "STRLEN" if arguments.len() == 1 => Some(RdfTerm::Literal {
            value: string(0)?.chars().count().to_string(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        }),
        "CONCAT" if !arguments.is_empty() => concat_terms(row, arguments),
        "CONTAINS" if arguments.len() == 2 => boolean_term(string(0)?.contains(&string(1)?)),
        "STRSTARTS" if arguments.len() == 2 => boolean_term(string(0)?.starts_with(&string(1)?)),
        "STRENDS" if arguments.len() == 2 => boolean_term(string(0)?.ends_with(&string(1)?)),
        "STRBEFORE" if arguments.len() == 2 => {
            let (value, datatype, language) = compatible_string_arguments(row, arguments)?;
            let needle = expression_string(row, &arguments[1])?;
            match value.split_once(&needle) {
                Some((before, _)) => Some(RdfTerm::Literal {
                    value: before.into(),
                    datatype,
                    language,
                }),
                // DAWG strbefore01a/02：未命中时始终是 simple literal；不能
                // 从第一个参数继承语言标签或 xsd:string datatype。
                None => plain_string_term(""),
            }
        }
        "STRAFTER" if arguments.len() == 2 => {
            let (value, datatype, language) = compatible_string_arguments(row, arguments)?;
            let needle = expression_string(row, &arguments[1])?;
            match value.split_once(&needle) {
                Some((_, after)) => Some(RdfTerm::Literal {
                    value: after.into(),
                    datatype,
                    language,
                }),
                None => plain_string_term(""),
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
        } if source == "http://www.w3.org/2001/XMLSchema#string" => (datatype != "decimal"
            || is_xsd_decimal_lexical(value))
        .then(|| value.parse().ok())??,
        // Turtle facts 的 plain literal 在内部模型中可保留为无显式 datatype；按
        // SPARQL 1.1 它等同 xsd:string，故 xsd:double("2") 必须可转换。
        RdfTerm::Literal {
            value,
            datatype: None,
            language: None,
        } => (datatype != "decimal" || is_xsd_decimal_lexical(value))
            .then(|| value.parse().ok())??,
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

/// xsd:decimal 不接受 scientific notation；例如 `-10.2E3` 可转换为
/// float/double，却必须使 `xsd:decimal()` 产生 expression error。
fn is_xsd_decimal_lexical(value: &str) -> bool {
    let value = value
        .strip_prefix('+')
        .or_else(|| value.strip_prefix('-'))
        .unwrap_or(value);
    let Some((whole, fractional)) = value.split_once('.') else {
        return !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    };
    !fractional.contains('.')
        && ((!whole.is_empty() && whole.bytes().all(|byte| byte.is_ascii_digit()))
            || (!fractional.is_empty() && fractional.bytes().all(|byte| byte.is_ascii_digit())))
        && fractional.bytes().all(|byte| byte.is_ascii_digit())
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
        } if source == "http://www.w3.org/2001/XMLSchema#string" => {
            value.parse::<i64>().ok()? as f64
        }
        RdfTerm::Literal {
            value,
            datatype: None,
            language: None,
        } => value.parse::<i64>().ok()? as f64,
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
        RdfTerm::Literal {
            value,
            datatype: None,
            language: None,
        } => match value.as_str() {
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

/// 将 PostgreSQL numeric 映射出的 xsd:decimal 以及 SPARQL 整数提升为精确十进制。
/// float/double 必须留在 IEEE 754 语义，不能借此改变 NaN、INF 或浮点 promotion。
fn exact_decimal_value(row: &Binding, expression: &Expression) -> Option<(BigDecimal, String)> {
    exact_decimal_from_term(&evaluate_expression(row, expression)?)
}

fn exact_decimal_from_term(term: &RdfTerm) -> Option<(BigDecimal, String)> {
    let RdfTerm::Literal {
        value, datatype, ..
    } = term
    else {
        return None;
    };
    let datatype = datatype.clone()?;
    if datatype.ends_with("#float")
        || datatype.ends_with("#double")
        || !is_numeric_datatype(&datatype)
    {
        return None;
    }
    Some((BigDecimal::from_str(value).ok()?, datatype))
}

fn arithmetic_result_datatype(
    operator: ArithmeticOperator,
    left: &str,
    right: &str,
) -> &'static str {
    if matches!(operator, ArithmeticOperator::Divide) {
        "http://www.w3.org/2001/XMLSchema#decimal"
    } else if is_integer_datatype(left) && is_integer_datatype(right) {
        "http://www.w3.org/2001/XMLSchema#integer"
    } else {
        "http://www.w3.org/2001/XMLSchema#decimal"
    }
}

fn exact_numeric_result_term(value: BigDecimal, datatype: &str) -> Option<RdfTerm> {
    // PostgreSQL numeric 与 Ontop 的结果协议保留有效小数 scale，例如 0.10 +
    // 0.20 必须是 0.30，而不能在 JSON serializer 前归一为 0.3。
    let value = value.to_string();
    Some(RdfTerm::Literal {
        value: if value == "-0" { "0".into() } else { value },
        datatype: Some(datatype.into()),
        language: None,
    })
}

fn exact_average_result_term(
    value: BigDecimal,
    canonical_facts_avg_lexical: bool,
) -> Option<RdfTerm> {
    let mut value = value.to_string();
    if canonical_facts_avg_lexical && !value.contains(['.', 'e', 'E']) {
        value.push_str(".0");
    }
    Some(RdfTerm::Literal {
        value,
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    })
}

/// Ontop PostgreSQL 的 ABS() 将 decimal function result 以其规范 lexical form
/// 返回（例如 `ABS(-1.50)` 为 `1.5`）。这不同于映射值及 SUM 的 scale-preserving
/// 契约；因此不能在通用精确算术 serializer 中截断尾零。
fn canonical_decimal_function_term(value: BigDecimal, datatype: &str) -> Option<RdfTerm> {
    let lexical = value.to_string();
    let lexical = lexical
        .strip_suffix(".0")
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if lexical.contains('.') {
                lexical
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_owned()
            } else {
                lexical
            }
        });
    Some(RdfTerm::Literal {
        value: if lexical == "-0" { "0".into() } else { lexical },
        datatype: Some(datatype.into()),
        language: None,
    })
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
        value: if datatype.ends_with("#float") || datatype.ends_with("#double") {
            canonical_floating_lexical(&value.to_string(), Some(datatype))
        } else if decimal {
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

fn canonicalize_facts_floating_term(term: RdfTerm) -> RdfTerm {
    match term {
        RdfTerm::Literal {
            value,
            datatype: Some(datatype),
            language,
        } if datatype.ends_with("#float") || datatype.ends_with("#double") => RdfTerm::Literal {
            value: canonical_floating_lexical(&value, Some(&datatype)),
            datatype: Some(datatype),
            language,
        },
        term => term,
    }
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
        .is_some_and(is_numeric_datatype)
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

fn is_integer_datatype(datatype: &str) -> bool {
    matches!(
        datatype,
        "http://www.w3.org/2001/XMLSchema#byte"
            | "http://www.w3.org/2001/XMLSchema#short"
            | "http://www.w3.org/2001/XMLSchema#int"
            | "http://www.w3.org/2001/XMLSchema#integer"
            | "http://www.w3.org/2001/XMLSchema#long"
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

/// TIMEZONE() 返回的 offset 是 `xsd:dayTimeDuration`，不是 TZ() 的字符串。
/// 以 canonical 的 day/hour/minute/second 顺序格式化；无 timezone 的 dateTime
/// 不能调用此 helper，因为调用方会在 RFC 3339 解析失败时传播 expression error。
fn day_time_duration_lexical(seconds: i32) -> String {
    if seconds == 0 {
        return "PT0S".into();
    }
    let sign = if seconds.is_negative() { "-" } else { "" };
    let mut remaining = seconds.unsigned_abs();
    let days = remaining / 86_400;
    remaining %= 86_400;
    let hours = remaining / 3_600;
    remaining %= 3_600;
    let minutes = remaining / 60;
    let seconds = remaining % 60;
    let mut result = format!("{sign}P");
    if days != 0 {
        result.push_str(&format!("{days}D"));
    }
    if hours != 0 || minutes != 0 || seconds != 0 {
        result.push('T');
        if hours != 0 {
            result.push_str(&format!("{hours}H"));
        }
        if minutes != 0 {
            result.push_str(&format!("{minutes}M"));
        }
        if seconds != 0 {
            result.push_str(&format!("{seconds}S"));
        }
    }
    result
}

fn temporal_value(value: &str) -> Option<(NaiveDate, NaiveTime)> {
    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some((datetime.date_naive(), datetime.time()));
    }
    if let Some(datetime) = date_with_timezone_datetime(value) {
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
    if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(datetime.naive_utc());
    }
    if let Some(datetime) = date_with_timezone_datetime(value) {
        return Some(datetime.naive_utc());
    }
    let (date, time) = temporal_value(value)?;
    Some(NaiveDateTime::new(date, time))
}

/// RFC 3339 不接受 bare xsd:date timezone lexical（如 `2006-08-23Z`），
/// 因而为其补上午夜 time component 后按 offset 解析。无 timezone 的 xsd:date
/// 不经过此路径，仍保留 SPARQL 的未指定时区 value。
fn date_with_timezone_datetime(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    matches!(value.as_bytes().get(10), Some(b'Z' | b'+' | b'-')).then(|| {
        chrono::DateTime::parse_from_rfc3339(&format!("{}T00:00:00{}", &value[..10], &value[10..]))
            .ok()
    })?
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
                FilterValue::Numeric(expected) => {
                    datatype.as_deref().is_some_and(is_numeric_datatype)
                        && actual
                            .parse::<f64>()
                            .is_ok_and(|actual| actual == *expected)
                }
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
                        && datatype.as_deref().is_none_or(|actual| {
                            actual == "http://www.w3.org/2001/XMLSchema#string"
                        })
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
                            Some(expected) => {
                                datatype.as_ref() == Some(expected)
                                    || (expected == "http://www.w3.org/2001/XMLSchema#string"
                                        && datatype.is_none())
                            }
                            // SPARQL simple literal 只与 simple/xsd:string literal 相等，
                            // 不可因词法相同而与 date、time 等有类型值相等。
                            None => datatype.as_deref().is_none_or(|actual| {
                                actual == "http://www.w3.org/2001/XMLSchema#string"
                            }),
                        }
                }
            }
        }
        Filter::Expression(expression) => expression_boolean(row, expression) == Some(true),
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
    use super::{
        canonical_floating_lexical, join_binding_relations, mapping_floating_lexical, Binding,
        DataSource, KnowledgeGraphSpec, RdfTerm, RuntimeError, TriplePattern, VkgRuntime,
    };

    struct PushdownSource;

    impl DataSource for PushdownSource {
        fn supports_postgres_bgp_pushdown(&self) -> bool {
            true
        }

        fn execute(
            &mut self,
            sql: &str,
            parameters: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            assert!(sql.contains(" JOIN "), "应生成一条 PostgreSQL JOIN: {sql}");
            assert!(
                sql.contains("p0.c0 = p1.c0"),
                "应按共享 subject JOIN: {sql}"
            );
            assert_eq!(
                parameters,
                [
                    "https://example.test/person/",
                    "",
                    "https://example.test/person/",
                    "",
                ]
            );
            Ok(vec![vec![
                Some("https://example.test/person/7".into()),
                Some("Ada".into()),
                Some("42".into()),
            ]])
        }
    }

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

    #[test]
    fn preserves_postgres_floating_lexicals_for_native_mappings() {
        let double = Some("http://www.w3.org/2001/XMLSchema#double");
        assert_eq!(mapping_floating_lexical("1", double, false), "1");
    }

    #[test]
    fn pushes_unique_mapping_plans_into_a_postgres_bgp_join() {
        let directory = tempfile::tempdir().expect("temporary mapping directory");
        let mapping = directory.path().join("pushdown.obda");
        std::fs::write(
            &mapping,
            "[MappingDeclaration]\n\
             mappingId label\n\
             target <https://example.test/person/{id}> <https://example.test/label> {label} .\n\
             source SELECT id, label FROM people\n\n\
             mappingId rank\n\
             target <https://example.test/person/{id}> <https://example.test/rank> {rank} .\n\
             source SELECT id, rank FROM people\n",
        )
        .expect("write mapping");
        let spec = KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: None,
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        };
        let mut runtime = VkgRuntime::new(spec, PushdownSource).expect("create runtime");
        let patterns = [
            TriplePattern {
                subject: "?person".into(),
                predicate: "<https://example.test/label>".into(),
                object: "?label".into(),
                graph: None,
            },
            TriplePattern {
                subject: "?person".into(),
                predicate: "<https://example.test/rank>".into(),
                object: "?rank".into(),
                graph: None,
            },
        ];

        let rows = runtime
            .select_bgp_postgres_sql(&patterns, &["person".into(), "label".into(), "rank".into()])
            .expect("pushdown execution")
            .expect("safe BGP should be pushed down");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("person"),
            Some(&RdfTerm::Iri("https://example.test/person/7".into()))
        );
        assert!(matches!(
            rows[0].get("label"),
            Some(RdfTerm::Literal { value, .. }) if value == "Ada"
        ));
        assert!(matches!(
            rows[0].get("rank"),
            Some(RdfTerm::Literal { value, .. }) if value == "42"
        ));
    }
}

fn resolve_mapping_iri(
    value: &str,
    base: Option<&str>,
    require_absolute: bool,
) -> Result<String, RuntimeError> {
    if require_absolute && !value.contains(':') {
        return Err(RuntimeError::Mapping(format!(
            "Not a valid (absolute) IRI: {value}"
        )));
    }
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
            if binding.get(name).is_some_and(|existing| existing != term) {
                return None;
            }
            binding.insert(name.into(), term.clone());
        } else if constant_matches(token, term, pattern, fact, ontology) {
        } else {
            return None;
        }
    }
    if let Some(name) = pattern.predicate.strip_prefix('?') {
        let term = RdfTerm::Iri(fact.predicate.clone());
        if binding.get(name).is_some_and(|existing| existing != &term) {
            return None;
        }
        binding.insert(name.into(), term);
    }
    if let Some(name) = pattern
        .graph
        .as_deref()
        .and_then(|graph| graph.strip_prefix('?'))
    {
        let term = fact.graph.clone()?;
        if binding.get(name).is_some_and(|existing| existing != &term) {
            return None;
        }
        binding.insert(name.into(), term);
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

/// 把 property path 两端的 term 统一为 solution mapping。重复出现的变量必须绑定到
/// 同一个 RDF term；常量则只接受完全相同的 RDF term。
fn bind_path_term(binding: &mut Binding, token: &str, term: &RdfTerm) -> bool {
    if let Some(variable) = token.strip_prefix('?') {
        return binding
            .get(variable)
            .is_none_or(|existing| existing == term)
            && {
                binding.insert(variable.into(), term.clone());
                true
            };
    }
    rdf_term_matches(term, token)
}

/// `{0}` 的端点常量本身属于零长度 identity domain，即使活动图为空。
/// 当前 property-path parser 已完成 PREFIX 展开，故只需处理其可产生的 IRI 和
/// plain literal 词法；其他 literal 仍由 facts-domain 的统一收集覆盖。
fn zero_length_constant(token: &str) -> Option<RdfTerm> {
    if token.starts_with('<') && token.ends_with('>') {
        return Some(RdfTerm::Iri(token[1..token.len() - 1].into()));
    }
    let value = token.strip_prefix('"')?.strip_suffix('"')?;
    Some(RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    })
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
        (Some(variable), Some(_)) if variable.starts_with('?') => true,
        (Some(expected), Some(RdfTerm::Iri(actual))) => expected == actual,
        (Some(expected), Some(RdfTerm::BlankNode(actual))) => expected == format!("_:{actual}"),
        _ => false,
    }
}

fn dataset_default_path_graphs(pattern: &TriplePattern) -> Option<Vec<String>> {
    const PREFIX: &str = "__rtop_dataset_default_path_graphs__";
    pattern
        .graph
        .as_deref()?
        .strip_prefix(PREFIX)
        .map(|graphs| graphs.split('\u{1f}').map(str::to_owned).collect())
}

fn variables(pattern: &TriplePattern) -> Vec<String> {
    [
        pattern.subject.as_str(),
        pattern.predicate.as_str(),
        pattern.object.as_str(),
        pattern.graph.as_deref().unwrap_or_default(),
    ]
    .into_iter()
    .filter_map(|value| value.strip_prefix('?').map(str::to_owned))
    .collect()
}

fn decode_mapping_term(
    value: &DataValue,
    term: &BindingTerm,
    iri_base: Option<&str>,
    mapping_infer_default_datatype: bool,
    mapping_require_absolute_iri_values: bool,
    canonicalize_floating_lexicals: bool,
) -> Result<RdfTerm, RuntimeError> {
    Ok(match term {
        BindingTerm::Iri => RdfTerm::Iri(resolve_mapping_iri(
            &value.value,
            iri_base,
            mapping_require_absolute_iri_values,
        )?),
        BindingTerm::BlankNode => RdfTerm::BlankNode(value.value.clone()),
        BindingTerm::Literal {
            datatype,
            language,
            infer_datatype,
        } => RdfTerm::Literal {
            value: mapping_floating_lexical(
                &value.value,
                datatype.as_deref().or_else(|| {
                    (*infer_datatype && mapping_infer_default_datatype)
                        .then(|| value.datatype.as_deref())
                        .flatten()
                }),
                canonicalize_floating_lexicals,
            ),
            datatype: datatype.clone().or_else(|| {
                (*infer_datatype && mapping_infer_default_datatype)
                    .then(|| value.datatype.clone())
                    .flatten()
            }),
            language: language.clone(),
        },
    })
}

/// 将一个 mapping plan 放入组合 SQL 时重编号其 PostgreSQL 参数。mapping source
/// 本身不接受运行时参数；此处只改写 parser 生成、位于 SQL 字符串/标识符之外的 `$n`。
fn renumber_postgres_parameters(sql: &str, offset: usize) -> String {
    let bytes = sql.as_bytes();
    let mut output = String::with_capacity(sql.len());
    let mut index = 0;
    let mut quote = None;
    while index < bytes.len() {
        let byte = bytes[index];
        let character = sql[index..]
            .chars()
            .next()
            .expect("index always points to a UTF-8 boundary");
        if let Some(delimiter) = quote {
            output.push(character);
            if byte == delimiter {
                if index + 1 < bytes.len() && bytes[index + 1] == delimiter {
                    output.push(delimiter as char);
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += character.len_utf8();
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            output.push(character);
            index += 1;
            continue;
        }
        if byte == b'$' {
            let end = bytes[index + 1..]
                .iter()
                .position(|byte| !byte.is_ascii_digit())
                .map(|length| index + 1 + length)
                .unwrap_or(bytes.len());
            if end > index + 1 {
                let parameter = sql[index + 1..end].parse::<usize>().expect("digits");
                output.push('$');
                output.push_str(&(parameter + offset).to_string());
                index = end;
                continue;
            }
        }
        output.push(character);
        index += character.len_utf8();
    }
    output
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

fn mapping_floating_lexical(
    value: &str,
    datatype: Option<&str>,
    canonicalize_floating_lexicals: bool,
) -> String {
    canonicalize_floating_lexicals
        .then(|| canonical_floating_lexical(value, datatype))
        .unwrap_or_else(|| value.into())
}

fn instantiate(
    pattern: &TriplePattern,
    binding: &Binding,
    row_index: usize,
    blank_nodes: &mut HashMap<String, RdfTerm>,
) -> Option<RdfFact> {
    let mut term = |token: &str| {
        token
            .strip_prefix('?')
            .and_then(|name| {
                binding.get(name).cloned().or_else(|| {
                    name.starts_with("__rtop_blank_").then(|| {
                        blank_nodes
                            .entry(token.into())
                            .or_insert_with(|| {
                                RdfTerm::BlankNode(format!("construct-{row_index}-{name}"))
                            })
                            .clone()
                    })
                })
            })
            .or_else(|| {
                token.strip_prefix("_:").map(|label| {
                    blank_nodes
                        .entry(token.into())
                        .or_insert_with(|| {
                            RdfTerm::BlankNode(format!("construct-{row_index}-{label}"))
                        })
                        .clone()
                })
            })
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
