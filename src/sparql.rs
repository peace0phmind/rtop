use crate::RuntimeError;

#[derive(Debug, Clone)]
pub struct TriplePattern {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

#[derive(Debug, Clone)]
pub enum Query {
    Select {
        variables: Vec<String>,
        pattern: TriplePattern,
    },
    Ask {
        pattern: TriplePattern,
    },
    Construct {
        template: TriplePattern,
        pattern: TriplePattern,
    },
    Describe {
        resource: String,
    },
}

pub fn parse(input: &str) -> Result<Query, RuntimeError> {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let upper = compact.to_ascii_uppercase();
    if upper.starts_with("SELECT ") {
        let open = compact
            .find('{')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
        let variables = compact[6..open]
            .split_whitespace()
            .map(variable)
            .collect::<Result<Vec<_>, _>>()?;
        if variables.is_empty() {
            return Err(RuntimeError::MalformedSparql("SELECT 需要一个变量".into()));
        }
        return Ok(Query::Select {
            variables,
            pattern: group(&compact[open..])?,
        });
    }
    if upper.starts_with("ASK") {
        let open = compact
            .find('{')
            .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
        return Ok(Query::Ask {
            pattern: group(&compact[open..])?,
        });
    }
    if upper.starts_with("DESCRIBE ") {
        let resource = compact[8..].trim();
        if resource.starts_with('<') && resource.ends_with('>') {
            return Ok(Query::Describe {
                resource: resource.trim_matches(['<', '>']).into(),
            });
        }
        return Err(RuntimeError::UnsupportedSparql(
            "DESCRIBE 仅支持一个 IRI".into(),
        ));
    }
    if upper.starts_with("CONSTRUCT ") {
        let where_at = upper
            .find(" WHERE ")
            .ok_or_else(|| RuntimeError::MalformedSparql("CONSTRUCT 缺少 WHERE".into()))?;
        let template = group(compact[9..where_at].trim())?;
        return Ok(Query::Construct {
            template,
            pattern: group(&compact[where_at + 7..])?,
        });
    }
    Err(RuntimeError::UnsupportedSparql(
        "仅支持 SELECT、ASK、CONSTRUCT 与 DESCRIBE".into(),
    ))
}

fn variable(token: &str) -> Result<String, RuntimeError> {
    token
        .strip_prefix('?')
        .map(str::to_owned)
        .ok_or_else(|| RuntimeError::MalformedSparql("SELECT 变量必须以 ? 开头".into()))
}

fn group(input: &str) -> Result<TriplePattern, RuntimeError> {
    let open = input
        .find('{')
        .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `{`".into()))?;
    let close = input
        .rfind('}')
        .ok_or_else(|| RuntimeError::MalformedSparql("缺少 `}`".into()))?;
    if close <= open {
        return Err(RuntimeError::MalformedSparql("图模式分隔符为空".into()));
    }
    let terms: Vec<_> = input[open + 1..close]
        .trim()
        .trim_end_matches('.')
        .split_whitespace()
        .collect();
    if terms.len() != 3 {
        return Err(RuntimeError::UnsupportedSparql(
            "目前只支持一个三元组模式".into(),
        ));
    }
    if terms[1].starts_with('?') {
        return Err(RuntimeError::UnsupportedSparql("目前不支持谓词变量".into()));
    }
    Ok(TriplePattern {
        subject: terms[0].into(),
        predicate: terms[1].into(),
        object: terms[2].into(),
    })
}
