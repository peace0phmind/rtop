use crate::{sparql::SelectQuery, RuntimeError};

pub struct Mapping {
    subject_template: String,
    predicate: String,
    object: String,
    source: String,
}
pub struct Plan {
    pub sql: String,
    pub parameters: Vec<String>,
    pub variables: Vec<String>,
}
impl Mapping {
    pub fn parse(text: &str) -> Result<Self, RuntimeError> {
        if !text.contains("[MappingDeclaration]") {
            return Err(RuntimeError::Mapping("缺少 [MappingDeclaration]".into()));
        }
        let target = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("target").map(str::trim))
            .ok_or_else(|| RuntimeError::Mapping("mapping 必须包含 target".into()))?;
        let source = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("source").map(str::trim))
            .ok_or_else(|| RuntimeError::Mapping("mapping 必须包含 source".into()))?;
        if !source.to_ascii_uppercase().starts_with("SELECT ") {
            return Err(RuntimeError::Mapping("source 必须是 SELECT 语句".into()));
        }
        let terms: Vec<_> = target.trim_end_matches('.').split_whitespace().collect();
        if terms.len() != 3 {
            return Err(RuntimeError::Mapping(
                "最小 mapping target 必须恰好包含一个三元组".into(),
            ));
        }
        Ok(Self {
            subject_template: terms[0].to_owned(),
            predicate: terms[1].to_owned(),
            object: terms[2].to_owned(),
            source: source.to_owned(),
        })
    }
    pub fn reformulate(&self, query: &SelectQuery) -> Result<Plan, RuntimeError> {
        if self.predicate != query.predicate || self.object != query.object {
            return Err(RuntimeError::NotFullyTranslatable(
                "三元组模式不匹配最小 mapping".into(),
            ));
        }
        let column = self
            .subject_template
            .trim_matches('<')
            .trim_matches('>')
            .split('{')
            .nth(1)
            .and_then(|s| s.split('}').next())
            .ok_or_else(|| {
                RuntimeError::Mapping("target subject 必须包含一个 {column} 模板".into())
            })?;
        if query.variables.len() != 1 || query.variables[0] != query.subject.trim_start_matches('?')
        {
            return Err(RuntimeError::NotFullyTranslatable(
                "最小切片只投影 subject 变量".into(),
            ));
        }
        let iri = self.subject_template.trim_matches('<').trim_matches('>');
        let (prefix, suffix) = iri
            .split_once('{')
            .ok_or_else(|| RuntimeError::Mapping("无效的 subject 模板".into()))?;
        let suffix = suffix
            .split_once('}')
            .map(|(_, s)| s)
            .ok_or_else(|| RuntimeError::Mapping("无效的 subject 模板".into()))?;
        Ok(Plan {
            sql: format!(
                "SELECT $1 || CAST({column} AS text) || $2 FROM ({}) AS rtop_mapping",
                self.source
            ),
            parameters: vec![prefix.into(), suffix.into()],
            variables: query.variables.clone(),
        })
    }
}
