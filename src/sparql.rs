use crate::RuntimeError;

#[derive(Debug)]
pub struct SelectQuery { pub variables: Vec<String>, pub subject: String, pub predicate: String, pub object: String }
impl SelectQuery {
    pub fn parse(input: &str) -> Result<Self, RuntimeError> {
        let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
        let upper = compact.to_uppercase();
        if !upper.starts_with("SELECT ") { return Err(RuntimeError::UnsupportedSparql("only SELECT basic graph patterns are supported".into())); }
        let open = compact.find('{').ok_or_else(|| RuntimeError::MalformedSparql("expected `{`".into()))?;
        let close = compact.rfind('}').ok_or_else(|| RuntimeError::MalformedSparql("expected `}`".into()))?;
        if close <= open { return Err(RuntimeError::MalformedSparql("empty graph pattern delimiter".into())); }
        let selected = compact[6..open].trim();
        let variables = selected.split_whitespace().map(|v| v.strip_prefix('?').ok_or_else(|| RuntimeError::MalformedSparql("SELECT variables must start with ?".into())).map(str::to_owned)).collect::<Result<Vec<_>, _>>()?;
        if variables.is_empty() { return Err(RuntimeError::MalformedSparql("SELECT needs a variable".into())); }
        let terms: Vec<_> = compact[open + 1..close].trim().trim_end_matches('.').split_whitespace().collect();
        if terms.len() != 3 { return Err(RuntimeError::UnsupportedSparql("only one triple pattern is supported".into())); }
        Ok(Self { variables, subject: terms[0].to_owned(), predicate: terms[1].to_owned(), object: terms[2].to_owned() })
    }
}
