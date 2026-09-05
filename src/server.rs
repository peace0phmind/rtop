use crate::{
    format_rdf_term, load_configuration, Binding, PostgresDataSource, QueryResult, RdfFact,
    RdfTerm, RuntimeError, VkgRuntime,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use uuid::Uuid;

/// 启动最小 SPARQL HTTP adapter。每个请求独立创建 adapter，会话类型不会越过内核边界。
pub fn serve(config: &str, bind: &str, development: bool) -> Result<(), RuntimeError> {
    let listener = TcpListener::bind(bind)
        .map_err(|e| RuntimeError::DataSource(format!("无法监听 {bind}：{e}")))?;
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let _ = handle(&mut stream, config, development);
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

fn handle(stream: &mut TcpStream, config: &str, development: bool) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let mut content_type = String::new();
    let mut accept = String::new();
    let mut content_length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-type") {
                content_type = value.trim().into();
            } else if name.eq_ignore_ascii_case("accept") {
                accept = value.trim().into();
            } else if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    let body = String::from_utf8_lossy(&body).into_owned();
    let form_parameters = if content_type.starts_with("application/x-www-form-urlencoded") {
        form_values(&body)
    } else {
        BTreeMap::new()
    };
    let query = if method == "GET" {
        target
            .split_once('?')
            .and_then(|(_, q)| form_value(q, "query"))
    } else if content_type.starts_with("application/sparql-query") {
        Some(body.clone())
    } else if content_type.starts_with("application/x-www-form-urlencoded") {
        form_value(&body, "query")
    } else {
        None
    };
    let path = target.split('?').next().unwrap_or_default();
    let response = match (path, query) {
        ("/healthz", _) => Ok(("200 OK", "text/plain", "ok".into())),
        ("/ontop/reformulate", Some(query)) if development => reformulate(config, &query),
        ("/sparql", Some(query)) if method == "GET" || method == "POST" => execute(config, &query),
        ("/sparql", None) if method == "GET" || method == "POST" => {
            Err(RuntimeError::MalformedSparql("请求缺少 query 参数".into()))
        }
        ("/sparql", _) => Ok((
            "405 Method Not Allowed",
            "text/plain",
            "method not allowed".into(),
        )),
        _ if path == "/ontology" => ontology(config),
        _ if path.starts_with("/predefined/") => {
            predefined(config, &path[12..], target, &form_parameters, &accept)
        }
        _ => Ok(("404 Not Found", "text/plain", "not found".into())),
    };
    let (status, content_type, body) = match response
        .and_then(|response| negotiate(&accept, response))
    {
        Ok(response) => response,
        Err(RuntimeError::NotAcceptable(error)) => ("406 Not Acceptable", "text/plain", error),
        Err(
            error @ RuntimeError::MalformedSparql(_) | error @ RuntimeError::UnsupportedSparql(_),
        ) => ("400 Bad Request", "text/plain", error.to_string()),
        Err(error) => ("500 Internal Server Error", "text/plain", error.to_string()),
    };
    // Ontop 的开发诊断端点在成功 reformulation 上返回关联 ID。该 ID 本身不
    // 是跨进程稳定值，但响应头存在且是 UUID 可让客户端将诊断请求关联到日志。
    let query_id = if path == "/ontop/reformulate" && development && status == "200 OK" {
        format!("X-Query-ID: {}\r\n", Uuid::new_v4())
    } else {
        String::new()
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nCache-Control: no-store\r\n{query_id}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

fn negotiate(
    accept: &str,
    response: (&'static str, &'static str, String),
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    if accept.is_empty()
        || accept.contains("*/*")
        || accept
            .split(',')
            .any(|value| value.trim().split(';').next() == Some(response.1))
    {
        Ok(response)
    } else {
        Err(RuntimeError::NotAcceptable(
            "请求的 Accept 不支持该结果格式".into(),
        ))
    }
}

fn execute(
    config: &str,
    query: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let loaded = load_configuration(config)?;
    execute_loaded(loaded, query)
}

fn execute_loaded(
    loaded: crate::LoadedConfiguration,
    query: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let source = PostgresDataSource::connect(&loaded.postgres)?;
    let mut runtime = match loaded.direct_mapping {
        Some(direct) => VkgRuntime::new_with_direct_mapping(
            loaded.spec,
            source,
            &direct.base_iri,
            &direct.relations,
        )?,
        None => VkgRuntime::new(loaded.spec, source)?,
    };
    Ok(match runtime.query(query)? {
        QueryResult::Bindings(rows) => (
            "200 OK",
            "application/sparql-results+json",
            bindings_json(&rows),
        ),
        QueryResult::Boolean(value) => (
            "200 OK",
            "application/sparql-results+json",
            format!("{{\"head\":{{}},\"boolean\":{value}}}"),
        ),
        QueryResult::Graph(facts) => (
            "200 OK",
            "text/turtle",
            facts.iter().map(turtle).collect::<Vec<_>>().join("\n"),
        ),
    })
}

fn ontology(config: &str) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let loaded = load_configuration(config)?;
    if !loaded.endpoint.enable_download_ontology {
        return Ok(("404 Not Found", "text/plain", "not found".into()));
    }
    let Some(path) = loaded.spec.ontology_file else {
        return Ok(("404 Not Found", "text/plain", "No ontology found".into()));
    };
    let body = std::fs::read_to_string(path)
        .map_err(|error| RuntimeError::Config(format!("无法读取 ontology：{error}")))?;
    Ok(("200 OK", "text/plain", body))
}

#[derive(Debug, Deserialize)]
struct PredefinedFile {
    queries: BTreeMap<String, PredefinedDefinition>,
}

#[derive(Debug, Deserialize)]
struct PredefinedDefinition {
    #[serde(rename = "queryType")]
    query_type: String,
    #[serde(default)]
    parameters: BTreeMap<String, PredefinedParameter>,
}

#[derive(Debug, Deserialize)]
struct PredefinedParameter {
    #[serde(rename = "type")]
    kind: String,
    required: bool,
}

fn predefined(
    config: &str,
    id: &str,
    target: &str,
    form_parameters: &BTreeMap<String, String>,
    accept: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let loaded = load_configuration(config)?;
    let (Some(config_path), Some(queries_path)) = (
        loaded.endpoint.predefined_config.as_ref(),
        loaded.endpoint.predefined_queries.as_ref(),
    ) else {
        return Ok(("404 Not Found", "text/plain", "not found".into()));
    };
    let definitions: PredefinedFile =
        serde_json::from_str(&std::fs::read_to_string(config_path).map_err(|error| {
            RuntimeError::Config(format!("无法读取 predefined_config：{error}"))
        })?)
        .map_err(|error| RuntimeError::Config(format!("无效 predefined_config：{error}")))?;
    let Some(definition) = definitions.queries.get(id) else {
        return Ok(("404 Not Found", "text/plain", "not found".into()));
    };
    if !definition.query_type.eq_ignore_ascii_case("graph") {
        return Err(RuntimeError::UnsupportedSparql(
            "预定义查询目前只支持 GRAPH".into(),
        ));
    }
    let queries: toml::Value =
        toml::from_str(&std::fs::read_to_string(queries_path).map_err(|error| {
            RuntimeError::Config(format!("无法读取 predefined_queries：{error}"))
        })?)
        .map_err(|error| RuntimeError::Config(format!("无效 predefined_queries：{error}")))?;
    let Some(query) = queries
        .get(id)
        .and_then(toml::Value::as_table)
        .and_then(|query| query.get("query"))
        .and_then(toml::Value::as_str)
    else {
        return Err(RuntimeError::Config(format!("预定义查询缺少条目：{id}")));
    };
    let mut params = target
        .split_once('?')
        .map(|(_, value)| form_values(value))
        .unwrap_or_default();
    // 与 Spring 的 request-parameter binding 一致：GET 使用 URL query，
    // application/x-www-form-urlencoded POST 可在 body 提供或覆写参数。
    params.extend(form_parameters.clone());
    let query = bind_predefined_query(query, &definition.parameters, &params)?;
    let response = execute_loaded(loaded, &query)?;
    if accept.is_empty() || accept.contains("*/*") || accept.contains("text/turtle") {
        Ok(response)
    } else {
        Err(RuntimeError::NotAcceptable(
            "请求的 Accept 不支持预定义图查询的 Turtle 结果".into(),
        ))
    }
}

fn bind_predefined_query(
    query: &str,
    definitions: &BTreeMap<String, PredefinedParameter>,
    values: &BTreeMap<String, String>,
) -> Result<String, RuntimeError> {
    let mut bindings = String::new();
    for (name, definition) in definitions {
        let Some(value) = values.get(name) else {
            if definition.required {
                return Err(RuntimeError::MalformedSparql(format!(
                    "缺少必填预定义参数：{name}"
                )));
            }
            continue;
        };
        let term = if definition.kind.eq_ignore_ascii_case("iri") {
            oxiri::Iri::parse(value.clone()).map_err(|_| {
                RuntimeError::MalformedSparql(format!("预定义参数 {name} 不是有效 IRI"))
            })?;
            format!("<{value}>")
        } else {
            let datatype = definition
                .kind
                .strip_prefix("xsd:")
                .map(|local| format!("http://www.w3.org/2001/XMLSchema#{local}"))
                .unwrap_or_else(|| definition.kind.clone());
            format!(
                "\"{}\"^^<{datatype}>",
                value.replace('\\', "\\\\").replace('"', "\\\"")
            )
        };
        bindings.push_str(&format!(" VALUES ?{name} {{ {term} }}"));
    }
    let Some(position) = query.find('{') else {
        return Err(RuntimeError::MalformedSparql("预定义查询缺少图模式".into()));
    };
    Ok(format!(
        "{}{}{}",
        &query[..position + 1],
        bindings,
        &query[position + 1..]
    ))
}

fn reformulate(
    config: &str,
    query: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let loaded = load_configuration(config)?;
    let source = PostgresDataSource::connect(&loaded.postgres)?;
    let runtime = match loaded.direct_mapping {
        Some(direct) => VkgRuntime::new_with_direct_mapping(
            loaded.spec,
            source,
            &direct.base_iri,
            &direct.relations,
        )?,
        None => VkgRuntime::new(loaded.spec, source)?,
    };
    Ok(("200 OK", "text/plain", runtime.reformulate(query)?))
}

fn turtle(fact: &RdfFact) -> String {
    format!(
        "{} <{}> {} .",
        format_rdf_term(&fact.subject),
        fact.predicate,
        format_rdf_term(&fact.object)
    )
}

fn bindings_json(rows: &[Binding]) -> String {
    let variables = rows
        .first()
        .map(|row| row.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let head = variables
        .iter()
        .map(|name| format!("\"{}\"", json(name)))
        .collect::<Vec<_>>()
        .join(",");
    let rows = rows
        .iter()
        .map(|row| {
            let values = row
                .iter()
                .map(|(name, value)| format!("\"{}\":{}", json(name), term_json(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{values}}}")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"head\":{{\"vars\":[{head}]}},\"results\":{{\"bindings\":[{rows}]}}}}")
}

fn term_json(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(value) => format!("{{\"type\":\"uri\",\"value\":\"{}\"}}", json(value)),
        RdfTerm::BlankNode(value) => {
            format!("{{\"type\":\"bnode\",\"value\":\"{}\"}}", json(value))
        }
        RdfTerm::Literal {
            value,
            datatype,
            language,
        } => {
            let suffix = language
                .as_ref()
                .map(|value| format!(",\"xml:lang\":\"{}\"", json(value)))
                .or_else(|| {
                    datatype
                        .as_ref()
                        .map(|value| format!(",\"datatype\":\"{}\"", json(value)))
                })
                .unwrap_or_default();
            format!(
                "{{\"type\":\"literal\",\"value\":\"{}\"{suffix}}}",
                json(value)
            )
        }
    }
}

fn json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
fn form_value(input: &str, key: &str) -> Option<String> {
    input.split('&').find_map(|part| {
        part.split_once('=')
            .filter(|(name, _)| *name == key)
            .map(|(_, value)| percent_decode(value))
    })
}
fn form_values(input: &str) -> BTreeMap<String, String> {
    input
        .split('&')
        .filter_map(|part| part.split_once('='))
        .map(|(key, value)| (percent_decode(key), percent_decode(value)))
        .collect()
}
fn percent_decode(value: &str) -> String {
    value
        .replace('+', " ")
        .split('%')
        .fold(String::new(), |mut out, part| {
            if out.is_empty() {
                out.push_str(part);
            } else if part.len() >= 2 {
                let (hex, rest) = part.split_at(2);
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    out.push_str(rest);
                }
            }
            out
        })
}

#[cfg(test)]
mod tests {
    use super::{bindings_json, negotiate};
    use crate::{Binding, RdfTerm};

    #[test]
    fn serializes_sparql_json_bindings_with_rdf_terms() {
        let mut row = Binding::new();
        row.insert(
            "person".into(),
            RdfTerm::Iri("https://example.test/p".into()),
        );
        assert_eq!(bindings_json(&[row]), "{\"head\":{\"vars\":[\"person\"]},\"results\":{\"bindings\":[{\"person\":{\"type\":\"uri\",\"value\":\"https://example.test/p\"}}]}}");
    }

    #[test]
    fn negotiates_an_accepted_sparql_result_format() {
        assert!(negotiate(
            "application/sparql-results+json",
            ("200 OK", "application/sparql-results+json", "{}".into())
        )
        .is_ok());
        assert!(negotiate(
            "text/turtle",
            ("200 OK", "application/sparql-results+json", "{}".into())
        )
        .is_err());
    }
}
