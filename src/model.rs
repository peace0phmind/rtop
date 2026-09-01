use std::collections::BTreeMap;
use thiserror::Error;

pub type Binding = BTreeMap<String, RdfTerm>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdfTerm { Iri(String), Literal { value: String, datatype: Option<String>, language: Option<String> } }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResult { Bindings(Vec<Binding>) }

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("invalid-config: {0}")]
    Config(String),
    #[error("invalid-mapping: {0}")]
    Mapping(String),
    #[error("malformed-sparql: {0}")]
    MalformedSparql(String),
    #[error("unsupported-sparql: {0}")]
    UnsupportedSparql(String),
    #[error("not-fully-translatable: {0}")]
    NotFullyTranslatable(String),
    #[error("datasource-failure: {0}")]
    DataSource(String),
}
