use crate::sparql::{parse as parse_query, Query};
use crate::{
    format_rdf_term, load_configuration, Binding, PostgresDataSource, QueryResult, RdfFact,
    RdfTerm, RuntimeError, VkgRuntime,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::Duration;
use uuid::Uuid;

/// 启动最小 SPARQL HTTP adapter。每个请求独立创建 adapter，会话类型不会越过内核边界。
pub fn serve(config: &str, bind: &str, development: bool) -> Result<(), RuntimeError> {
    let listener = TcpListener::bind(bind)
        .map_err(|e| RuntimeError::DataSource(format!("无法监听 {bind}：{e}")))?;
    let config: Arc<str> = Arc::from(config);
    // endpoint 的存活和配置有效性是两个可观察状态。先监听使 healthcheck 能
    // 区分进程不可用与配置不可用；后者由查询请求返回稳定诊断。CLI 入口仍在
    // 调用 load_configuration 时严格失败。
    let startup_error: Option<Arc<str>> = match load_configuration(Path::new(&*config)) {
        Ok(loaded) => {
            crate::validate_static_inputs(&loaded.spec, loaded.direct_mapping.is_none(), true)?;
            None
        }
        Err(error) => Some(Arc::from(error.to_string())),
    };
    // 覆盖率 runner 必须让真实 HTTP server 正常结束，LLVM profiler 才会落盘。该
    // 控制变量只编译进 `cargo llvm-cov` 的 cfg(coverage) 二进制，发布 endpoint 不会
    // 读取或暴露它；请求仍完整经过真实 TCP/HTTP handler。
    #[cfg(coverage)]
    let coverage_max_requests = std::env::var("RTOP_COVERAGE_SERVER_MAX_REQUESTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0);
    #[cfg(not(coverage))]
    let coverage_max_requests: Option<usize> = None;
    #[cfg(coverage)]
    let mut coverage_workers = Vec::new();
    let mut accepted_requests = 0usize;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let config = Arc::clone(&config);
                let startup_error = startup_error.clone();
                let worker = std::thread::spawn(move || {
                    let mut stream = stream;
                    let _ = handle(&mut stream, &config, development, startup_error.as_deref());
                });
                #[cfg(coverage)]
                coverage_workers.push(worker);
                #[cfg(not(coverage))]
                drop(worker);
                accepted_requests += 1;
                if coverage_max_requests.is_some_and(|limit| accepted_requests >= limit) {
                    break;
                }
            }
            Err(_) => continue,
        }
    }
    #[cfg(coverage)]
    for worker in coverage_workers {
        let _ = worker.join();
    }
    Ok(())
}

fn handle(
    stream: &mut TcpStream,
    config: &str,
    development: bool,
    startup_error: Option<&str>,
) -> std::io::Result<()> {
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
    let response = if path != "/healthz" {
        if let Some(error) = startup_error {
            Ok(("500 Internal Server Error", "text/plain", error.into()))
        } else {
            handle_route(
                path,
                query,
                method,
                config,
                development,
                target,
                &form_parameters,
                &accept,
                stream,
            )
        }
    } else {
        Ok(("200 OK", "text/plain", "ok".into()))
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

fn handle_route(
    path: &str,
    query: Option<String>,
    method: &str,
    config: &str,
    development: bool,
    target: &str,
    form_parameters: &BTreeMap<String, String>,
    accept: &str,
    stream: &TcpStream,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    match (path, query) {
        ("/ontop/reformulate", Some(query)) if development => reformulate(config, &query),
        ("/sparql", Some(query)) if method == "GET" || method == "POST" => {
            execute_until_disconnect(
                config.to_owned(),
                query,
                accept.to_owned(),
                stream
                    .try_clone()
                    .map_err(|error| RuntimeError::DataSource(error.to_string()))?,
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
    }
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
    let loaded = load_configuration(Path::new(config))?;
    execute_loaded_cancellable(loaded, query, accept, cancellation_sender)
}

fn execute_loaded(
    loaded: crate::LoadedConfiguration,
    query: &str,
    accept: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let source = PostgresDataSource::connect(&loaded.postgres)?;
    let mut runtime = runtime_from_loaded(loaded, source)?;
    let variables = select_projection_variables(query)?;
    render_query_result(runtime.query(query)?, accept, &variables)
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
    let variables = select_projection_variables(query)?;
    render_query_result(runtime.query(query)?, accept, &variables)
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
            direct.preserve_physical_rows,
        ),
        None => VkgRuntime::new_with_mapping_options_relaxed_source_sql(
            loaded.spec,
            source,
            loaded.mapping_infer_default_datatype,
            loaded.mapping_require_absolute_iri_values,
        ),
    }
}

fn render_query_result(
    result: QueryResult,
    accept: &str,
    variables: &[String],
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
                bindings_json(&rows, variables),
            ),
            "application/sparql-results+xml" => (
                "200 OK",
                "application/sparql-results+xml",
                bindings_xml(&rows, variables),
            ),
            "text/csv" => ("200 OK", "text/csv", bindings_delimited(&rows, variables, ',', false)),
            "text/tab-separated-values" => (
                "200 OK",
                "text/tab-separated-values",
                bindings_delimited(&rows, variables, '\t', true),
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
    let loaded = load_configuration(Path::new(config))?;
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
    let loaded = load_configuration(Path::new(config))?;
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
                // Keep this distinct from malformed user SPARQL: fixed Ontop's
                // generic predefined-query controller turns this binding failure
                // into HTTP 500 before query evaluation.
                RuntimeError::InvalidPredefinedIri(value.clone())
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
    let loaded = load_configuration(Path::new(config))?;
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

fn select_projection_variables(query: &str) -> Result<Vec<String>, RuntimeError> {
    match parse_query(query)? {
        Query::Select { variables, .. } => Ok(variables),
        _ => Ok(Vec::new()),
    }
}

fn bindings_json(rows: &[Binding], projection_variables: &[String]) -> String {
    let variables = binding_variables(rows, projection_variables);
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

fn binding_variables(rows: &[Binding], projection_variables: &[String]) -> Vec<String> {
    if projection_variables.is_empty() {
        rows.first()
            .map(|row| row.keys().cloned().collect())
            .unwrap_or_default()
    } else {
        projection_variables.to_vec()
    }
}

fn bindings_xml(rows: &[Binding], projection_variables: &[String]) -> String {
    let variables = binding_variables(rows, projection_variables);
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

fn bindings_delimited(
    rows: &[Binding],
    projection_variables: &[String],
    separator: char,
    tsv: bool,
) -> String {
    let variables = binding_variables(rows, projection_variables);
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
                        .as_deref()
                        // Ontop 的 SPARQL JSON endpoint 把 RDF 1.1 中与 simple
                        // literal 等价的显式 xsd:string 序列化为无 datatype 的 term。
                        // 内部术语仍保留 datatype，避免丢失 facts 的原始信息。
                        .filter(|value| *value != "http://www.w3.org/2001/XMLSchema#string")
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
    // query 参数可以以 `%23`（SPARQL 的首行注释）开头。此前以已输出文本是否为空
    // 来区分 split 的首段，会把这个首个转义序列当作普通文本，令 `PREFIX :` 未被
    // 读取为声明。按 URL 字节流逐项解码，同时保留不完整或无效的 `%` 序列。
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'+' {
            output.push(b' ');
            index += 1;
        } else if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or_default();
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                output.push(byte);
                index += 3;
            } else {
                output.push(b'%');
                index += 1;
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        bind_predefined_query, binding_variables, bindings_delimited, bindings_json, bindings_xml,
        form_values, handle, negotiate, percent_decode, PredefinedParameter,
    };
    use crate::{Binding, RdfTerm};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    #[cfg(coverage)]
    #[test]
    fn coverage_server_entrypoint_accepts_healthcheck_and_exits_normally() {
        let directory = tempfile::tempdir().unwrap();
        let mapping = directory.path().join("mapping.obda");
        let config = directory.path().join("rtop.toml");
        std::fs::write(
            &mapping,
            "[MappingDeclaration] @collection [[\nmappingId health\ntarget <https://example.test/s> <https://example.test/p> <https://example.test/o> .\nsource SELECT 1\n]]\n",
        )
        .unwrap();
        std::fs::write(
            &config,
            "mapping = \"mapping.obda\"\n\n[datasource]\nkind = \"postgres\"\nhost = \"127.0.0.1\"\nport = 1\ndatabase = \"rtop\"\nuser = \"rtop\"\npassword = \"rtop\"\n",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        // This test is compiled only when serve reads the coverage control
        // variable.  The full runner executes this test before its delivery
        // endpoint, so one accepted request is sufficient and deterministic.
        unsafe { std::env::set_var("RTOP_COVERAGE_SERVER_MAX_REQUESTS", "1") };
        let config_for_server = config.clone();
        let worker = std::thread::spawn(move || {
            super::serve(
                config_for_server.to_str().unwrap(),
                &address.to_string(),
                false,
            )
        });
        let mut client = loop {
            match TcpStream::connect(address) {
                Ok(client) => break client,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        };
        write!(client, "GET /healthz HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert_eq!(worker.join().unwrap(), Ok(()));
        unsafe { std::env::remove_var("RTOP_COVERAGE_SERVER_MAX_REQUESTS") };
    }

    fn response_with_startup_error(path: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            handle(
                &mut stream,
                "/definitely/missing.toml",
                false,
                Some("invalid-config: datasource 必须指定 password 或 password_file"),
            )
            .unwrap();
        });
        let mut client = TcpStream::connect(address).unwrap();
        write!(client, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        worker.join().unwrap();
        response
    }

    #[test]
    fn keeps_healthcheck_available_and_reports_invalid_startup_configuration_to_queries() {
        let health = response_with_startup_error("/healthz");
        assert!(health.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(health.ends_with("\r\n\r\nok"));

        let query = response_with_startup_error("/sparql?query=ASK%20%7B%7D");
        assert!(query.starts_with("HTTP/1.1 500 Internal Server Error\r\n"));
        assert!(query
            .ends_with("\r\n\r\ninvalid-config: datasource 必须指定 password 或 password_file"));
    }

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
    fn invalid_predefined_iri_uses_ontop_controller_error_category() {
        let definitions = BTreeMap::from([(
            "person".into(),
            PredefinedParameter {
                kind: "IRI".into(),
                required: true,
            },
        )]);
        let values = BTreeMap::from([("person".into(), "not-an-iri".into())]);

        assert_eq!(
            bind_predefined_query("CONSTRUCT {} WHERE {}", &definitions, &values),
            Err(crate::RuntimeError::InvalidPredefinedIri(
                "not-an-iri".into()
            ))
        );
    }

    #[test]
    fn binds_typed_predefined_parameters_and_rejects_missing_required_values() {
        let definitions = BTreeMap::from([
            (
                "label".into(),
                PredefinedParameter {
                    kind: "xsd:string".into(),
                    required: true,
                },
            ),
            (
                "optional".into(),
                PredefinedParameter {
                    kind: "xsd:integer".into(),
                    required: false,
                },
            ),
        ]);
        let values = BTreeMap::from([("label".into(), "A \"quoted\" label".into())]);
        assert_eq!(
            bind_predefined_query("CONSTRUCT { ?item ?p ?label } WHERE { ?item ?p ?label }", &definitions, &values).unwrap(),
            "CONSTRUCT { ?item ?p \"A \\\"quoted\\\" label\"^^<http://www.w3.org/2001/XMLSchema#string> } WHERE { ?item ?p \"A \\\"quoted\\\" label\"^^<http://www.w3.org/2001/XMLSchema#string> }"
        );
        assert!(matches!(
            bind_predefined_query("CONSTRUCT { ?item ?p ?label } WHERE { ?item ?p ?label }", &definitions, &BTreeMap::new()),
            Err(crate::RuntimeError::MalformedSparql(message)) if message == "缺少必填预定义参数：label"
        ));
    }

    #[test]
    fn preserves_custom_predefined_datatype_and_rejects_a_query_without_graph_pattern() {
        let definitions = BTreeMap::from([(
            "code".into(),
            PredefinedParameter {
                kind: "https://example.test/datatype/code".into(),
                required: true,
            },
        )]);
        let values = BTreeMap::from([("code".into(), "A\\B\"C".into())]);
        assert_eq!(
            bind_predefined_query("SELECT ?code", &definitions, &values),
            Err(crate::RuntimeError::MalformedSparql(
                "预定义查询缺少图模式".into()
            ))
        );
    }

    #[test]
    fn decodes_a_leading_escaped_comment_marker_in_a_sparql_query() {
        assert_eq!(
            percent_decode("%23%20comment%0APREFIX%20%3A%20%3Chttps%3A%2F%2Fexample.test%2F%3E"),
            "# comment\nPREFIX : <https://example.test/>"
        );
        assert_eq!(
            form_values("person=https%3A%2F%2Fexample.test%2Fperson%2F1&label=Ada+Lovelace"),
            BTreeMap::from([
                ("person".into(), "https://example.test/person/1".into()),
                ("label".into(), "Ada Lovelace".into()),
            ])
        );
    }

    #[test]
    fn serializes_sparql_json_bindings_with_rdf_terms() {
        let mut row = Binding::new();
        row.insert(
            "person".into(),
            RdfTerm::Iri("https://example.test/p".into()),
        );
        assert_eq!(bindings_json(&[row], &["person".into()]), "{\"head\":{\"vars\":[\"person\"]},\"results\":{\"bindings\":[{\"person\":{\"type\":\"uri\",\"value\":\"https://example.test/p\"}}]}}");
    }

    #[test]
    fn serializes_explicit_xsd_string_as_ontop_simple_literal_in_sparql_json() {
        let mut row = Binding::new();
        row.insert(
            "name".into(),
            RdfTerm::Literal {
                value: "The Fact Company".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
                language: None,
            },
        );
        assert_eq!(
            bindings_json(&[row], &["name".into()]),
            "{\"head\":{\"vars\":[\"name\"]},\"results\":{\"bindings\":[{\"name\":{\"type\":\"literal\",\"value\":\"The Fact Company\"}}]}}"
        );
    }

    #[test]
    fn preserves_select_projection_variables_for_empty_json_results() {
        assert_eq!(
            bindings_json(&[], &["person".into(), "role".into()]),
            "{\"head\":{\"vars\":[\"person\",\"role\"]},\"results\":{\"bindings\":[]}}"
        );
    }

    #[test]
    fn serializes_all_rdf_term_forms_in_sparql_result_protocols() {
        let mut row = Binding::new();
        row.insert("blank".into(), RdfTerm::BlankNode("row-1".into()));
        row.insert(
            "label".into(),
            RdfTerm::Literal {
                value: "A, \"quoted\" label".into(),
                datatype: None,
                language: Some("en".into()),
            },
        );
        row.insert(
            "count".into(),
            RdfTerm::Literal {
                value: "2".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                language: None,
            },
        );

        let variables = vec!["blank".into(), "label".into(), "count".into()];
        let xml = bindings_xml(&[row.clone()], &variables);
        assert!(xml.contains("<bnode>row-1</bnode>"));
        assert!(xml.contains("xml:lang=\"en\""));
        assert!(xml.contains("datatype=\"http://www.w3.org/2001/XMLSchema#integer\""));
        let csv = bindings_delimited(&[row.clone()], &variables, ',', false);
        assert!(csv.contains("\"A, \"\"quoted\"\" label\""));
        let json = bindings_json(&[row], &variables);
        assert!(json.contains("\"type\":\"bnode\""));
        assert!(json.contains("\"xml:lang\":\"en\""));
        assert!(json.contains("\"datatype\":\"http://www.w3.org/2001/XMLSchema#integer\""));
    }

    #[test]
    fn derives_variables_when_a_result_has_no_select_projection() {
        let mut row = Binding::new();
        row.insert(
            "derived".into(),
            RdfTerm::Iri("https://example.test/value".into()),
        );
        assert_eq!(binding_variables(&[row], &[]), vec!["derived"]);
        assert!(binding_variables(&[], &[]).is_empty());
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
