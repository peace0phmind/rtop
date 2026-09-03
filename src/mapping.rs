use crate::{sparql::TriplePattern, RdfFact, RdfTerm, RuntimeError};
use rio_api::parser::TriplesParser;
use rio_turtle::TurtleParser;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

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
    pub terms: Vec<BindingTerm>,
}
#[derive(Clone, Copy)]
pub enum BindingTerm {
    Iri,
    Literal,
}
impl Mapping {
    pub fn describe(&self) -> Result<Plan, RuntimeError> {
        self.reformulate(
            &TriplePattern {
                subject: "?resource".into(),
                predicate: self.predicate.clone(),
                object: self.object.clone(),
            },
            &["resource".into()],
        )
    }

    pub fn mapped_fact(&self, subject: String) -> RdfFact {
        RdfFact {
            subject: RdfTerm::Iri(subject),
            predicate: self.predicate.trim_matches(['<', '>']).into(),
            object: RdfTerm::Iri(self.object.trim_matches(['<', '>']).into()),
            graph: None,
        }
    }
    pub fn parse_file(path: &Path) -> Result<Self, RuntimeError> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| RuntimeError::Mapping(format!("无法读取 mapping：{error}")))?;
        if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("ttl" | "turtle")
        ) {
            Self::parse_r2rml(&text, path)
        } else {
            Self::parse(&text)
        }
    }

    pub fn parse(text: &str) -> Result<Self, RuntimeError> {
        if text.contains("[SourceDeclaration]") {
            return Err(RuntimeError::Mapping(
                "不支持已废弃的 [SourceDeclaration]；请在 rtop 配置中声明 datasource".into(),
            ));
        }
        let lines: Vec<_> = text
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect();
        if !lines
            .iter()
            .any(|line| line.trim() == "[MappingDeclaration]")
        {
            return Err(RuntimeError::Mapping("缺少 [MappingDeclaration]".into()));
        }
        let prefixes = prefixes(&lines)?;
        let target = lines
            .iter()
            .find_map(|line| line.trim().strip_prefix("target").map(str::trim))
            .ok_or_else(|| RuntimeError::Mapping("mapping 必须包含 target".into()))?;
        let source_at = lines
            .iter()
            .position(|line| line.trim().starts_with("source"))
            .ok_or_else(|| RuntimeError::Mapping("mapping 必须包含 source".into()))?;
        let mut source = lines[source_at]
            .trim()
            .strip_prefix("source")
            .unwrap()
            .trim()
            .to_owned();
        for line in lines.iter().skip(source_at + 1) {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with('[')
                || trimmed.starts_with("mappingId")
                || trimmed.starts_with("target")
                || trimmed.starts_with("source")
            {
                break;
            }
            source.push(' ');
            source.push_str(trimmed);
        }
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
            subject_template: expand(terms[0], &prefixes)?,
            predicate: expand(terms[1], &prefixes)?,
            object: expand(terms[2], &prefixes)?,
            source,
        })
    }

    fn parse_r2rml(text: &str, path: &Path) -> Result<Self, RuntimeError> {
        let base_iri = oxiri::Iri::parse(format!(
            "file://{}",
            path.canonicalize()
                .unwrap_or_else(|_| path.to_path_buf())
                .display()
        ))
        .map_err(|error| RuntimeError::Mapping(format!("无效 R2RML base IRI：{error}")))?;
        TurtleParser::new(Cursor::new(text), Some(base_iri))
            .parse_all(&mut |_| Ok(()) as Result<(), rio_turtle::TurtleError>)
            .map_err(|error| RuntimeError::Mapping(format!("R2RML RDF 语法错误：{error}")))?;
        let source = quoted_after(text, "rr:sqlQuery")
            .or_else(|| {
                quoted_after(text, "rr:tableName").map(|table| format!("SELECT * FROM {table}"))
            })
            .ok_or_else(|| {
                RuntimeError::Mapping("R2RML 结构错误：缺少 rr:sqlQuery 或 rr:tableName".into())
            })?;
        let template = quoted_after(text, "rr:template")
            .ok_or_else(|| RuntimeError::Mapping("R2RML 结构错误：缺少 rr:template".into()))?;
        let predicate = iri_or_prefixed_after(text, "rr:predicate ")
            .ok_or_else(|| RuntimeError::Mapping("R2RML 结构错误：缺少 rr:predicate".into()))?;
        let object = iri_or_prefixed_after(text, "rr:constant ")
            .or_else(|| quoted_after(text, "rr:column").map(|column| format!("{{{column}}}")))
            .ok_or_else(|| {
                RuntimeError::Mapping("R2RML 结构错误：缺少 rr:constant 或 rr:column".into())
            })?;
        if !source.to_ascii_uppercase().starts_with("SELECT ") {
            return Err(RuntimeError::Mapping(
                "R2RML rr:sqlQuery 必须是 SELECT 语句".into(),
            ));
        }
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let resolve = |value: String| {
            if value.contains("://") {
                value
            } else {
                format!("file://{}/{}", base.display(), value)
            }
        };
        Ok(Self {
            subject_template: format!("<{}>", resolve(template)),
            predicate: format!("<{}>", resolve(predicate)),
            object: format!("<{}>", resolve(object)),
            source,
        })
    }
    pub fn reformulate(
        &self,
        query: &TriplePattern,
        variables: &[String],
    ) -> Result<Plan, RuntimeError> {
        if self.predicate != query.predicate
            || (!query.object.starts_with('?') && self.object != query.object)
        {
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
        if variables.is_empty()
            || variables.len() > 2
            || variables[0] != query.subject.trim_start_matches('?')
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
        let mut projections = vec![format!("$1 || CAST({column} AS text) || $2")];
        let mut terms = vec![BindingTerm::Iri];
        if variables.len() == 2 {
            let object_column = self.object.trim_matches(['{', '}']);
            projections.push(format!("CAST({object_column} AS text)"));
            terms.push(BindingTerm::Literal);
        }
        Ok(Plan {
            sql: format!(
                "SELECT {} FROM ({}) AS rtop_mapping",
                projections.join(", "),
                self.source
            ),
            parameters: vec![prefix.into(), suffix.into()],
            variables: variables.to_vec(),
            terms,
        })
    }
}

fn quoted_after(text: &str, marker: &str) -> Option<String> {
    let tail = text.split_once(marker)?.1.trim_start();
    let tail = tail
        .strip_prefix("\"\"\"")
        .or_else(|| tail.strip_prefix('"'))?;
    let end = if text
        .split_once(marker)?
        .1
        .trim_start()
        .starts_with("\"\"\"")
    {
        "\"\"\""
    } else {
        "\""
    };
    if end == "\"\"\"" {
        return Some(tail.split_once(end)?.0.into());
    }
    let end_at = tail.char_indices().find_map(|(index, character)| {
        (character == '"' && !tail[..index].ends_with('\\')).then_some(index)
    })?;
    Some(tail[..end_at].replace("\\\"", "\""))
}

fn iri_after(text: &str, marker: &str) -> Option<String> {
    let tail = text.split_once(marker)?.1.trim_start();
    let iri = tail.strip_prefix('<')?.split_once('>')?.0;
    Some(iri.into())
}

fn iri_or_prefixed_after(text: &str, marker: &str) -> Option<String> {
    iri_after(text, marker).or_else(|| {
        let term = text.split_once(marker)?.1.split_whitespace().next()?;
        let (prefix, local) = term.trim_end_matches([';', '.']).split_once(':')?;
        let declaration = format!("@prefix {prefix}:");
        let iri = text
            .split_once(&declaration)?
            .1
            .trim_start()
            .strip_prefix('<')?
            .split_once('>')?
            .0;
        Some(format!("{iri}{local}"))
    })
}

fn prefixes(lines: &[&str]) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut result = BTreeMap::new();
    let mut in_prefixes = false;
    for line in lines {
        let line = line.trim();
        if line == "[PrefixDeclaration]" {
            in_prefixes = true;
            continue;
        }
        if line.starts_with('[') {
            in_prefixes = false;
        }
        if in_prefixes && !line.is_empty() {
            let (prefix, iri) = line
                .split_once(char::is_whitespace)
                .ok_or_else(|| RuntimeError::Mapping("无效的 prefix declaration".into()))?;
            result.insert(prefix.into(), iri.trim().trim_matches(['<', '>']).into());
        }
    }
    Ok(result)
}

fn expand(term: &str, prefixes: &BTreeMap<String, String>) -> Result<String, RuntimeError> {
    if term.starts_with('<') || term.starts_with('"') || term.starts_with('_') {
        return Ok(term.into());
    }
    let (prefix, local) = term
        .split_once(':')
        .ok_or_else(|| RuntimeError::Mapping(format!("无效的 mapping RDF term：{term}")))?;
    let key = format!("{prefix}:");
    let iri = prefixes
        .get(&key)
        .ok_or_else(|| RuntimeError::Mapping(format!("未声明的 prefix：{key}")))?;
    Ok(format!("<{iri}{local}>"))
}
