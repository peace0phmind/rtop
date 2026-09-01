use crate::RuntimeError;

#[derive(Debug)]
pub struct SelectQuery {
    pub variables: Vec<String>,
    pub subject: String,
    pub predicate: String,
    pub object: String,
}
impl SelectQuery {
    pub fn parse(input: &str) -> Result<Self, RuntimeError> {
        let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
        let upper = compact.to_uppercase();
        if !upper.starts_with("SELECT ") {
            return Err(RuntimeError::UnsupportedSparql(
                "目前只支持 SELECT 基本图模式".into(),
            ));
        }
        let open = compact
            .find('{')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
        let close = compact
            .rfind('}')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `}`".into()))?;
        if close <= open {
            return Err(RuntimeError::MalformedSparql("图模式分隔符为空".into()));
        }
        let selected = compact[6..open].trim();
        let variables = selected
            .split_whitespace()
            .map(|v| {
                v.strip_prefix('?')
                    .ok_or_else(|| RuntimeError::MalformedSparql("SELECT 变量必须以 ? 开头".into()))
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if variables.is_empty() {
            return Err(RuntimeError::MalformedSparql("SELECT 需要一个变量".into()));
        }
        let terms: Vec<_> = compact[open + 1..close]
            .trim()
            .trim_end_matches('.')
            .split_whitespace()
            .collect();
        if terms.len() != 3 {
            return Err(RuntimeError::UnsupportedSparql(
                "目前只支持一个三元组模式".into(),
            ));
        }
        Ok(Self {
            variables,
            subject: terms[0].to_owned(),
            predicate: terms[1].to_owned(),
            object: terms[2].to_owned(),
        })
    }
}
