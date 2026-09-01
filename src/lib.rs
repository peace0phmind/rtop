//! `rtop` 的稳定内核边界。此模块不公开 PostgreSQL 驱动类型。
mod config;
mod datasource;
mod facts;
mod mapping;
mod model;
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
}

impl<D: DataSource> VkgRuntime<D> {
    pub fn new(spec: KnowledgeGraphSpec, source: D) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse(
            &std::fs::read_to_string(&spec.mapping_file)
                .map_err(|e| RuntimeError::Config(format!("无法读取 mapping：{e}")))?,
        )?;
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
        Ok(Self {
            spec,
            source,
            mapping,
            facts,
        })
    }

    pub fn query(&mut self, sparql: &str) -> Result<QueryResult, RuntimeError> {
        match parse_query(sparql)? {
            Query::Select { variables, pattern } => {
                Ok(QueryResult::Bindings(self.select(&pattern, &variables)?))
            }
            Query::Ask { pattern } => Ok(QueryResult::Boolean(
                !self
                    .select(&pattern, &[pattern.subject.trim_start_matches('?').into()])?
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
                && matches!(&fact.object, RdfTerm::Iri(value) if query.object == format!("<{value}>"))
            {
                fact_binding(query, fact)
            } else {
                None
            }
        }));
        Ok(bindings)
    }

    pub fn spec(&self) -> &KnowledgeGraphSpec {
        &self.spec
    }
}

fn fact_binding(pattern: &TriplePattern, fact: &RdfFact) -> Option<Binding> {
    let mut binding = Binding::new();
    for (token, term) in [
        (&pattern.subject, &fact.subject),
        (&pattern.object, &fact.object),
    ] {
        if let Some(name) = token.strip_prefix('?') {
            binding.insert(name.into(), term.clone());
        } else if matches!(term, RdfTerm::Iri(value) if token == &format!("<{value}>")) {
        } else {
            return None;
        }
    }
    Some(binding)
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
