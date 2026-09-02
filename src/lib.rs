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

pub use config::{load_configuration, KnowledgeGraphSpec, LoadedConfiguration};
pub use datasource::{DataSource, PostgresConnectionConfig, PostgresDataSource};
pub use facts::{parse_nquads, parse_turtle};
pub use model::{Binding, QueryResult, RdfFact, RdfTerm, RuntimeError};
pub use rdf::format_rdf_term;
pub use server::serve;

use mapping::Mapping;
use sparql::{parse as parse_query, Query, TriplePattern};

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
        let facts = match &spec.facts_file {
            None => Vec::new(),
            Some(path) => {
                let content = std::fs::read(path)
                    .map_err(|e| RuntimeError::Facts(format!("无法读取 facts：{e}")))?;
                let format = spec
                    .facts_format
                    .as_deref()
                    .or_else(|| path.extension().and_then(|extension| extension.to_str()));
                match format {
                    Some("ttl" | "turtle") => parse_turtle(
                        content,
                        spec.facts_base_iri
                            .as_ref()
                            .map(|iri| oxiri::Iri::parse(iri.clone()))
                            .transpose()
                            .map_err(|e| {
                                RuntimeError::Facts(format!("无效 facts_base_iri：{e}"))
                            })?,
                    )?,
                    Some("nq" | "nquads") => parse_nquads(content)?,
                    _ => return Err(RuntimeError::Facts("未提供有效的 facts 文件格式".into())),
                }
            }
        };
        let ontology = match &spec.ontology_file {
            Some(path) => ontology::Ontology::load(path)?,
            None => ontology::Ontology::default(),
        };
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
                patterns,
            } => Ok(QueryResult::Bindings(
                self.select_bgp(&patterns, &variables)?,
            )),
            Query::Ask { patterns } => Ok(QueryResult::Boolean(
                !self
                    .select_bgp(&patterns, &variables(&patterns[0]))?
                    .is_empty(),
            )),
            Query::Construct { template, pattern } => {
                let variables = variables(&pattern);
                let rows = self.select(&pattern, &variables)?;
                Ok(QueryResult::Graph(
                    rows.into_iter()
                        .filter_map(|row| instantiate(&template, &row))
                        .collect(),
                ))
            }
            Query::Describe { resource } => Ok(QueryResult::Graph(
                self.facts
                    .iter()
                    .filter(
                        |fact| matches!(&fact.subject, RdfTerm::Iri(value) if value == &resource),
                    )
                    .cloned()
                    .collect(),
            )),
        }
    }

    /// 开发诊断：只暴露当前方言的 SQL 快照，不把它作为跨方言稳定契约。
    pub fn reformulate(&self, sparql: &str) -> Result<String, RuntimeError> {
        match parse_query(sparql)? {
            Query::Select {
                variables,
                patterns,
            } if patterns.len() == 1 => Ok(self.mapping.reformulate(&patterns[0], &variables)?.sql),
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
        variables: &[String],
    ) -> Result<Vec<Binding>, RuntimeError> {
        let plan = self.mapping.reformulate(query, variables);
        let mut bindings = match plan {
            Ok(plan) => self
                .source
                .execute(&plan.sql, &plan.parameters)?
                .into_iter()
                .map(|row| {
                    plan.variables
                        .iter()
                        .enumerate()
                        .map(|(index, name)| (name.clone(), RdfTerm::Iri(row[index].clone())))
                        .collect::<Binding>()
                })
                .collect::<Vec<_>>(),
            Err(RuntimeError::NotFullyTranslatable(_)) if !self.facts.is_empty() => Vec::new(),
            Err(error) => return Err(error),
        };
        bindings.extend(self.facts.iter().filter_map(|fact| {
            if fact.graph.is_none()
                && fact.predicate == query.predicate.trim_matches(['<', '>'])
                && fact_matches(query, fact, &self.ontology)
            {
                fact_binding(query, fact, &self.ontology)
            } else {
                None
            }
        }));
        Ok(bindings)
    }

    fn select_bgp(
        &mut self,
        patterns: &[TriplePattern],
        variables: &[String],
    ) -> Result<Vec<Binding>, RuntimeError> {
        if patterns.len() == 1 {
            return self.select(&patterns[0], variables);
        }
        let mut rows = vec![Binding::new()];
        for pattern in patterns {
            let matches = self
                .facts
                .iter()
                .filter(|fact| {
                    fact.graph.is_none()
                        && fact.predicate == pattern.predicate.trim_matches(['<', '>'])
                        && fact_matches(pattern, fact, &self.ontology)
                })
                .filter_map(|fact| fact_binding(pattern, fact, &self.ontology))
                .collect::<Vec<_>>();
            rows = rows
                .into_iter()
                .flat_map(|row| matches.iter().filter_map(move |next| join(&row, next)))
                .collect();
        }
        Ok(rows
            .into_iter()
            .map(|row| {
                variables
                    .iter()
                    .filter_map(|name| row.get(name).cloned().map(|value| (name.clone(), value)))
                    .collect()
            })
            .collect())
    }

    pub fn spec(&self) -> &KnowledgeGraphSpec {
        &self.spec
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
            let Some(rest) = token.strip_prefix('"') else {
                return false;
            };
            let Some((literal, suffix)) = rest.rsplit_once('"') else {
                return false;
            };
            literal == value
                && match suffix {
                    "" => datatype.is_none() && language.is_none(),
                    suffix if suffix.starts_with('@') => language.as_deref() == Some(&suffix[1..]),
                    suffix if suffix.starts_with("^^<") && suffix.ends_with('>') => {
                        datatype.as_deref() == Some(&suffix[3..suffix.len() - 1])
                    }
                    _ => false,
                }
        }
    }
}

fn variables(pattern: &TriplePattern) -> Vec<String> {
    [pattern.subject.as_str(), pattern.object.as_str()]
        .into_iter()
        .filter_map(|value| value.strip_prefix('?').map(str::to_owned))
        .collect()
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
    let predicate = pattern
        .predicate
        .strip_prefix('<')?
        .strip_suffix('>')?
        .into();
    let object = term(&pattern.object)?;
    Some(RdfFact {
        subject,
        predicate,
        object,
        graph: None,
    })
}
