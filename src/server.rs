use crate::{
    format_rdf_term, load_configuration, Binding, PostgresDataSource, QueryResult, RdfFact,
    RdfTerm, RuntimeError, VkgRuntime,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

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
    let mut content_length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Type:") {
            content_type = value.trim().into();
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    let body = String::from_utf8_lossy(&body);
    let query = if method == "GET" {
        target
            .split_once('?')
            .and_then(|(_, q)| form_value(q, "query"))
    } else if content_type.starts_with("application/sparql-query") {
        Some(body.into_owned())
    } else if content_type.starts_with("application/x-www-form-urlencoded") {
        form_value(&body, "query")
    } else {
        None
    };
    let response = match (target.split('?').next(), query) {
        (Some("/healthz"), _) => Ok(("200 OK", "text/plain", "ok".into())),
        (Some("/ontop/reformulate"), Some(query)) if development => Ok((
            "200 OK",
            "text/plain",
            format!("诊断请求已接受：{}", query.replace('\n', " ")),
        )),
        (Some("/sparql"), Some(query)) => execute(config, &query),
        (Some("/sparql"), None) => Err(RuntimeError::MalformedSparql("请求缺少 query 参数".into())),
        _ => Ok(("404 Not Found", "text/plain", "not found".into())),
    };
    let (status, content_type, body) = match response {
        Ok(response) => response,
        Err(
            error @ RuntimeError::MalformedSparql(_) | error @ RuntimeError::UnsupportedSparql(_),
        ) => ("400 Bad Request", "text/plain", error.to_string()),
        Err(error) => ("500 Internal Server Error", "text/plain", error.to_string()),
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

fn execute(
    config: &str,
    query: &str,
) -> Result<(&'static str, &'static str, String), RuntimeError> {
    let loaded = load_configuration(config)?;
    let source = PostgresDataSource::connect(&loaded.postgres)?;
    let mut runtime = VkgRuntime::new(loaded.spec, source)?;
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
