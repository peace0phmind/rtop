use crate::{
    format_rdf_term, load_configuration, Binding, PostgresDataSource, QueryResult, RdfFact,
    RdfTerm, RuntimeError, VkgRuntime,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use uuid::Uuid;

/// 启动最小 SPARQL HTTP adapter。每个请求独立创建 adapter，会话类型不会越过内核边界。
pub fn serve(config: &str, bind: &str, development: bool) -> Result<(), RuntimeError> {
    let listener = TcpListener::bind(bind)
        .map_err(|e| RuntimeError::DataSource(format!("无法监听 {bind}：{e}")))?;
    let config: Arc<str> = Arc::from(config);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let config = Arc::clone(&config);
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let _ = handle(&mut stream, &config, development);
                });
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
        ("/sparql", Some(query)) if method == "GET" || method == "POST" => {
            execute_until_disconnect(
                config.to_owned(),
                query,
                accept.clone(),
                stream.try_clone()?,
            )
        }
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

/// 查询在独立工作线程执行；连接关闭时由持有 cancel token 的监控方中断 PostgreSQL。
/// 这样慢请求、失败请求和断开的请求都不会占用 listener 或其他请求的连接。
fn execute_until_disconnect(
    config: String,
    query: String,
    accept: String,
    monitor: TcpStream,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    let (cancel_sender, cancel_receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = execute_cancellable(&config, &query, &accept, cancel_sender);
        let _ = result_sender.send(result);
    });

    monitor
        .set_nonblocking(true)
        .map_err(|error| RuntimeError::DataSource(error.to_string()))?;
    let cancellation = loop {
        match result_receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(RuntimeError::DataSource("query-worker-disconnected".into()))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let mut byte = [0_u8; 1];
        match monitor.peek(&mut byte) {
            Ok(0) => break cancel_receiver.recv_timeout(Duration::from_secs(1)).ok(),
            Ok(_) => continue,
            Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
            Err(error) => return Err(RuntimeError::DataSource(error.to_string())),
        }
    };
    if let Some(cancellation) = cancellation {
        let _ = cancellation.cancel();
    }
    // 客户端已不能接收响应；仍等待查询线程清理连接，避免遗留 PostgreSQL 执行单元。
    result_receiver
        .recv()
        .unwrap_or_else(|_| Err(RuntimeError::DataSource("query-worker-disconnected".into())))
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

fn execute_cancellable(
    config: &str,
    query: &str,
    accept: &str,
    cancellation_sender: mpsc::SyncSender<crate::QueryCancellation>,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let loaded = load_configuration(config)?;
    execute_loaded_cancellable(loaded, query, accept, cancellation_sender)
}

fn execute_loaded(
    loaded: crate::LoadedConfiguration,
    query: &str,
    accept: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let source = PostgresDataSource::connect(&loaded.postgres)?;
    let mut runtime = runtime_from_loaded(loaded, source)?;
    render_query_result(runtime.query(query)?, accept)
}

fn execute_loaded_cancellable(
    loaded: crate::LoadedConfiguration,
    query: &str,
    accept: &str,
    cancellation_sender: mpsc::SyncSender<crate::QueryCancellation>,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let source = PostgresDataSource::connect(&loaded.postgres)?;
    // 无接收者表示客户端已经在建立连接期间断开；继续执行没有可观察价值。
    if cancellation_sender.send(source.cancellation()).is_err() {
        return Err(RuntimeError::DataSource("client-disconnected".into()));
    }
    let mut runtime = runtime_from_loaded(loaded, source)?;
    render_query_result(runtime.query(query)?, accept)
}

/// 服务器入口唯一的运行时构造模块。它把 Direct Mapping 的分支留在内部，确保
/// `/sparql`、预定义查询和 reformulation 共享同一配置语义。
fn runtime_from_loaded(
    loaded: crate::LoadedConfiguration,
    source: PostgresDataSource,
) -> Result<VkgRuntime<PostgresDataSource>, RuntimeError> {
    match loaded.direct_mapping {
        Some(direct) => VkgRuntime::new_with_direct_mapping(
            loaded.spec,
            source,
            &direct.base_iri,
            &direct.relations,
        ),
        None => VkgRuntime::new(loaded.spec, source),
    }
}

fn render_query_result(
    result: QueryResult,
    accept: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    Ok(match result {
        QueryResult::Bindings(rows) => match select_media(
            accept,
            &[
                "application/sparql-results+json",
                "application/sparql-results+xml",
                "text/csv",
                "text/tab-separated-values",
            ],
            "application/sparql-results+json",
        )? {
            "application/sparql-results+json" => (
                "200 OK",
                "application/sparql-results+json",
                bindings_json(&rows),
            ),
            "application/sparql-results+xml" => (
                "200 OK",
                "application/sparql-results+xml",
                bindings_xml(&rows),
            ),
            "text/csv" => ("200 OK", "text/csv", bindings_delimited(&rows, ',', false)),
            "text/tab-separated-values" => (
                "200 OK",
                "text/tab-separated-values",
                bindings_delimited(&rows, '\t', true),
            ),
            _ => unreachable!(),
        },
        QueryResult::Boolean(value) => match select_media(
            accept,
            &[
                "application/sparql-results+json",
                "application/sparql-results+xml",
                "text/csv",
                "text/tab-separated-values",
            ],
            "application/sparql-results+json",
        )? {
            "application/sparql-results+json" => (
                "200 OK",
                "application/sparql-results+json",
                format!("{{\"head\":{{}},\"boolean\":{value}}}"),
            ),
            "application/sparql-results+xml" => (
                "200 OK",
                "application/sparql-results+xml",
                format!("<?xml version=\"1.0\"?><sparql xmlns=\"http://www.w3.org/2005/sparql-results#\"><head/><boolean>{value}</boolean></sparql>"),
            ),
            "text/csv" => ("200 OK", "text/csv", format!("boolean\n{value}\n")),
            "text/tab-separated-values" => (
                "200 OK",
                "text/tab-separated-values",
                format!("?boolean\n{value}\n"),
            ),
            _ => unreachable!(),
        },
        QueryResult::Graph(facts) => match select_media(
            accept,
            &["text/turtle", "application/n-triples"],
            "text/turtle",
        )? {
            "text/turtle" => (
                "200 OK",
                "text/turtle",
                facts.iter().map(turtle).collect::<Vec<_>>().join("\n"),
            ),
            "application/n-triples" => (
                "200 OK",
                "application/n-triples",
                facts.iter().map(turtle).collect::<Vec<_>>().join("\n"),
            ),
            _ => unreachable!(),
        },
    })
}

fn select_media<'a>(
    accept: &str,
    supported: &'a [&'a str],
    default: &'a str,
) -> Result<&'a str, RuntimeError> {
    if accept.is_empty() || accept.contains("*/*") {
        return Ok(default);
    }
    for requested in accept
        .split(',')
        .map(|value| value.trim().split(';').next().unwrap_or_default())
    {
        if let Some(supported) = supported.iter().find(|supported| requested == **supported) {
            return Ok(*supported);
        }
    }
    Err(RuntimeError::NotAcceptable(
        "请求的 Accept 不支持该结果格式".into(),
    ))
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
    let response = execute_loaded(loaded, &query, accept)?;
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
    let mut bound_query = query.to_owned();
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
        // 预定义查询的参数是服务端已验证的常量。直接替换变量可同时作用于
        // CONSTRUCT template 与 WHERE，避免把 VALUES 注入 template 边界而改变
        // 查询代数；此 endpoint 的固定参数名不允许变量名前缀歧义。
        bound_query = bound_query.replace(&format!("?{name}"), &term);
    }
    if !bound_query.contains('{') {
        return Err(RuntimeError::MalformedSparql("预定义查询缺少图模式".into()));
    }
    Ok(bound_query)
}

fn reformulate(
    config: &str,
    query: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let loaded = load_configuration(config)?;
    let source = PostgresDataSource::connect(&loaded.postgres)?;
    let runtime = runtime_from_loaded(loaded, source)?;
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

fn binding_variables(rows: &[Binding]) -> Vec<String> {
    rows.first()
        .map(|row| row.keys().cloned().collect())
        .unwrap_or_default()
}

fn bindings_xml(rows: &[Binding]) -> String {
    let variables = binding_variables(rows);
    let head = variables
        .iter()
        .map(|name| format!("<variable name=\"{}\"/>", xml(name)))
        .collect::<String>();
    let results = rows
        .iter()
        .map(|row| {
            let bindings = row
                .iter()
                .map(|(name, term)| {
                    format!(
                        "<binding name=\"{}\">{}</binding>",
                        xml(name),
                        term_xml(term)
                    )
                })
                .collect::<String>();
            if bindings.is_empty() {
                "<result/>".into()
            } else {
                format!("<result>{bindings}</result>")
            }
        })
        .collect::<String>();
    format!("<?xml version=\"1.0\"?><sparql xmlns=\"http://www.w3.org/2005/sparql-results#\"><head>{head}</head><results>{results}</results></sparql>")
}

fn term_xml(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(value) => format!("<uri>{}</uri>", xml(value)),
        RdfTerm::BlankNode(value) => format!("<bnode>{}</bnode>", xml(value)),
        RdfTerm::Literal {
            value,
            datatype,
            language,
        } => match (language, datatype) {
            (Some(language), _) => format!(
                "<literal xml:lang=\"{}\">{}</literal>",
                xml(language),
                xml(value)
            ),
            (_, Some(datatype)) => format!(
                "<literal datatype=\"{}\">{}</literal>",
                xml(datatype),
                xml(value)
            ),
            _ => format!("<literal>{}</literal>", xml(value)),
        },
    }
}

fn bindings_delimited(rows: &[Binding], separator: char, tsv: bool) -> String {
    let variables = binding_variables(rows);
    let header = variables
        .iter()
        .map(|name| if tsv { format!("?{name}") } else { csv(name) })
        .collect::<Vec<_>>()
        .join(&separator.to_string());
    let rows = rows
        .iter()
        .map(|row| {
            variables
                .iter()
                .map(|name| {
                    row.get(name)
                        .map(|term| term_delimited(term, tsv))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(&separator.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{header}\n{rows}\n")
}

fn term_delimited(term: &RdfTerm, tsv: bool) -> String {
    if tsv {
        format_rdf_term(term)
    } else {
        match term {
            RdfTerm::Iri(value) | RdfTerm::BlankNode(value) => csv(value),
            RdfTerm::Literal { value, .. } => csv(value),
        }
    }
}

fn csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.into()
    }
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    use super::{bind_predefined_query, bindings_json, negotiate, PredefinedParameter};
    use crate::{Binding, RdfTerm};
    use std::collections::BTreeMap;

    #[test]
    fn binds_predefined_iri_in_construct_template_and_where() {
        let definitions = BTreeMap::from([(
            "person".into(),
            PredefinedParameter {
                kind: "IRI".into(),
                required: true,
            },
        )]);
        let values = BTreeMap::from([("person".into(), "https://example.test/person/1".into())]);
        let query = bind_predefined_query(
            "CONSTRUCT { ?person <https://example.test/type> <https://example.test/Person> } WHERE { ?person <https://example.test/type> <https://example.test/Person> }",
            &definitions,
            &values,
        )
        .unwrap();
        assert!(!query.contains("?person"));
        assert_eq!(query.matches("<https://example.test/person/1>").count(), 2);
    }

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
