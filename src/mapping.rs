use crate::{RuntimeError, sparql::SelectQuery};

pub struct Mapping { subject_template: String, predicate: String, object: String, source: String }
pub struct Plan { pub sql: String, pub parameters: Vec<String>, pub variables: Vec<String> }
impl Mapping {
    pub fn parse(text: &str) -> Result<Self, RuntimeError> {
        if !text.contains("[MappingDeclaration]") { return Err(RuntimeError::Mapping("missing [MappingDeclaration]".into())); }
        let target = text.lines().find_map(|line| line.trim().strip_prefix("target").map(str::trim)).ok_or_else(|| RuntimeError::Mapping("mapping target is required".into()))?;
        let source = text.lines().find_map(|line| line.trim().strip_prefix("source").map(str::trim)).ok_or_else(|| RuntimeError::Mapping("mapping source is required".into()))?;
        if !source.to_ascii_uppercase().starts_with("SELECT ") { return Err(RuntimeError::Mapping("source must be a SELECT statement".into())); }
        let terms: Vec<_> = target.trim_end_matches('.').split_whitespace().collect();
        if terms.len() != 3 { return Err(RuntimeError::Mapping("minimal mapping target needs exactly one triple".into())); }
        Ok(Self { subject_template: terms[0].to_owned(), predicate: terms[1].to_owned(), object: terms[2].to_owned(), source: source.to_owned() })
    }
    pub fn reformulate(&self, query: &SelectQuery) -> Result<Plan, RuntimeError> {
        if self.predicate != query.predicate || self.object != query.object {
            return Err(RuntimeError::NotFullyTranslatable("triple pattern does not match the minimal mapping".into()));
        }
        let column = self.subject_template.trim_matches('<').trim_matches('>').split('{').nth(1).and_then(|s| s.split('}').next())
            .ok_or_else(|| RuntimeError::Mapping("target subject must contain one {column} template".into()))?;
        if query.variables.len() != 1 || query.variables[0] != query.subject.trim_start_matches('?') {
            return Err(RuntimeError::NotFullyTranslatable("minimal slice projects only the subject variable".into()));
        }
        let iri = self.subject_template.trim_matches('<').trim_matches('>');
        let (prefix, suffix) = iri.split_once('{').ok_or_else(|| RuntimeError::Mapping("invalid subject template".into()))?;
        let suffix = suffix.split_once('}').map(|(_, s)| s).ok_or_else(|| RuntimeError::Mapping("invalid subject template".into()))?;
        Ok(Plan { sql: format!("SELECT $1 || CAST({column} AS text) || $2 FROM ({}) AS rtop_mapping", self.source), parameters: vec![prefix.into(), suffix.into()], variables: query.variables.clone() })
    }
}
