use std::collections::BTreeMap;
use thiserror::Error;

pub type Binding = BTreeMap<String, RdfTerm>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RdfTerm {
    Iri(String),
    BlankNode(String),
    Literal {
        value: String,
        datatype: Option<String>,
        language: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdfFact {
    pub subject: RdfTerm,
    pub predicate: String,
    pub object: RdfTerm,
    pub graph: Option<RdfTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResult {
    Bindings(Vec<Binding>),
    Boolean(bool),
    Graph(Vec<RdfFact>),
}

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
    #[error("not-acceptable: {0}")]
    NotAcceptable(String),
    #[error("not-fully-translatable: {0}")]
    NotFullyTranslatable(String),
    #[error("type-error: {0}")]
    Type(String),
    #[error("datasource-failure: {0}")]
    DataSource(String),
    #[error("invalid-facts: {0}")]
    Facts(String),
    #[error("invalid-ontology: {0}")]
    Ontology(String),
}
