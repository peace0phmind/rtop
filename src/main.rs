use regex::Regex;
use rio_api::{formatter::TriplesFormatter, parser::TriplesParser};
use rio_turtle::{TurtleFormatter, TurtleParser};
use rtop::{
    format_rdf_term, load_configuration, serve, Mapping, PostgresDataSource, QueryResult, RdfFact,
    VkgRuntime,
};
use sqlparser::{
    ast::{SelectItem, SetExpr, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};
use std::io::Read;

struct ValidationSource;
impl rtop::DataSource for ValidationSource {
    fn execute(
        &mut self,
        _: &str,
        _: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, rtop::RuntimeError> {
        Err(rtop::RuntimeError::DataSource("validate 不执行查询".into()))
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let config = args.next().unwrap_or_default();
    if command == "mapping" {
        let input = args.next().unwrap_or_default();
        let output = args.next().unwrap_or_default();
        let option = args.next().unwrap_or_default();
        if input.is_empty() || output.is_empty() {
            eprintln!("用法：rtop mapping <pretty-r2rml|v1-to-v3> <input> <output>");
            std::process::exit(64);
        }
        let result = match config.as_str() {
            "pretty-r2rml" => prettify_r2rml(&input, &output),
            "v1-to-v3" if option.is_empty() => migrate_v1_mapping(&input, &output, false),
            "v1-to-v3" if option == "--simplify-projection" => {
                migrate_v1_mapping(&input, &output, true)
            }
            "to-obda" if option.is_empty() => r2rml_to_obda(&input, &output),
            "to-r2rml" if option == "--force" => obda_to_r2rml(&input, &output),
            "to-r2rml" => Err(rtop::RuntimeError::Config(
                "to-r2rml 默认需要 PostgreSQL metadata；当前 CLI 请显式传入 --force".into(),
            )),
            _ => {
                eprintln!("用法：rtop mapping <pretty-r2rml|to-obda|v1-to-v3> <input> <output>");
                std::process::exit(64);
            }
        };
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    if config.is_empty() {
        eprintln!("用法：rtop <query|validate|compile|endpoint|materialize|extract-db-metadata|bootstrap> <config.toml> [query-file|-|bind-address|output-file] [turtle|nquads]");
        std::process::exit(64);
    }
    if command == "validate" || command == "compile" {
        match load_configuration(&config).and_then(|loaded| {
            if loaded.direct_mapping.is_some() {
                Ok(())
            } else {
                VkgRuntime::new(loaded.spec, ValidationSource).map(|_| ())
            }
        }) {
            Ok(_) => return,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }
    if command == "endpoint" {
        let bind = args.next().unwrap_or_else(|| "0.0.0.0:8080".into());
        let development = std::env::var_os("RTOP_DEVELOPMENT").is_some();
        if let Err(error) = serve(&config, &bind, development) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    if command == "materialize" {
        let output = args.next().unwrap_or_else(|| "-".into());
        let format = args.next().unwrap_or_else(|| "turtle".into());
        if let Err(error) = materialize(&config, &output, &format) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    if command == "extract-db-metadata" {
        let output = args.next().unwrap_or_else(|| "-".into());
        if let Err(error) = extract_db_metadata(&config, &output) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    if command == "bootstrap" {
        let base_iri = args.next().unwrap_or_default();
        let mapping_output = args.next().unwrap_or_default();
        let ontology_output = args.next().unwrap_or_default();
        if base_iri.is_empty() || mapping_output.is_empty() || ontology_output.is_empty() {
            eprintln!(
                "用法：rtop bootstrap <config.toml> <base-iri> <mapping.obda> <ontology.ttl>"
            );
            std::process::exit(64);
        }
        if let Err(error) = bootstrap(&config, &base_iri, &mapping_output, &ontology_output) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    if command != "query" {
        eprintln!("未知命令：{command}");
        std::process::exit(64);
    }
    let query_path = args.next().unwrap_or_else(|| "-".into());
    let query = if query_path == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap();
        Ok(s)
    } else {
        std::fs::read_to_string(&query_path)
            .map_err(|error| rtop::RuntimeError::Config(format!("无法读取查询文件：{error}")))
    };
    let outcome = (|| {
        let query = query?;
        let loaded = load_configuration(config)?;
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
        runtime.query(&query)
    })();
    match outcome {
        Ok(QueryResult::Bindings(rows)) => {
            for row in rows {
                println!(
                    "{}",
                    row.into_iter()
                        .map(|(name, term)| format!("?{name}={}", format_rdf_term(&term)))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
        Ok(QueryResult::Boolean(value)) => println!("{value}"),
        Ok(QueryResult::Graph(facts)) => {
            for fact in facts {
                let subject = format_rdf_term(&fact.subject);
                let object = format_rdf_term(&fact.object);
                println!("{subject} <{}> {object} .", fact.predicate);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

/// 将当前 PostgreSQL VKG 的默认图以可重新加载的 RDF 文件交付。
fn materialize(config: &str, output: &str, format: &str) -> Result<(), rtop::RuntimeError> {
    let loaded = load_configuration(config)?;
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
    let QueryResult::Graph(facts) = runtime.query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")?
    else {
        return Err(rtop::RuntimeError::UnsupportedSparql(
            "materialize 只接受图结果".into(),
        ));
    };
    let text = match format {
        "turtle" | "ttl" => facts.iter().map(turtle).collect::<Vec<_>>().join("\n"),
        "nquads" | "nq" => facts.iter().map(nquad).collect::<Vec<_>>().join("\n"),
        other => {
            return Err(rtop::RuntimeError::Config(format!(
                "materialize 不支持的输出格式：{other}"
            )))
        }
    };
    if output == "-" {
        println!("{text}");
    } else {
        std::fs::write(output, format!("{text}\n")).map_err(|error| {
            rtop::RuntimeError::Config(format!("无法写入 materialize 输出：{error}"))
        })?;
        println!("NR of TRIPLES: {}", facts.len());
    }
    Ok(())
}

fn extract_db_metadata(config: &str, output: &str) -> Result<(), rtop::RuntimeError> {
    let loaded = load_configuration(config)?;
    let mut source = PostgresDataSource::connect(&loaded.postgres)?;
    let metadata = source.database_metadata()?;
    let payload = serde_json::to_string_pretty(&metadata)
        .map_err(|error| rtop::RuntimeError::Config(format!("无法序列化数据库元数据：{error}")))?;
    if output == "-" {
        println!("{payload}");
    } else {
        std::fs::write(output, format!("{payload}\n")).map_err(|error| {
            rtop::RuntimeError::Config(format!("无法写入 metadata 输出：{error}"))
        })?;
    }
    Ok(())
}

fn bootstrap(
    config: &str,
    base_iri: &str,
    mapping_output: &str,
    ontology_output: &str,
) -> Result<(), rtop::RuntimeError> {
    if base_iri.contains('#') || !base_iri.contains(':') {
        return Err(rtop::RuntimeError::Config(
            "bootstrap base-iri 必须为不含 # 的绝对 IRI".into(),
        ));
    }
    let loaded = load_configuration(config)?;
    let mut source = PostgresDataSource::connect(&loaded.postgres)?;
    let metadata = source.database_metadata()?;
    let base = base_iri.trim_end_matches('/');
    let mut mapping = String::from("[MappingDeclaration] @collection [[\n");
    let mut ontology = String::from("@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\n");
    for relation in metadata.relations {
        let table = unquote_identifier(&relation.name[0]);
        let columns = relation
            .columns
            .iter()
            .map(|column| unquote_identifier(&column.name))
            .collect::<Vec<_>>();
        if columns.is_empty() {
            continue;
        }
        let primary_key = relation
            .unique_constraints
            .iter()
            .find(|constraint| constraint.is_primary_key)
            .and_then(|constraint| constraint.determinants.first())
            .map(|column| unquote_identifier(column));
        let subject = primary_key
            .map(|column| format!("<{base}/{table}/{column}={{{column}}}>"))
            .unwrap_or_else(|| {
                format!(
                    "_:bootstrap-{table}-{}",
                    columns
                        .iter()
                        .map(|column| format!("{{{column}}}"))
                        .collect::<Vec<_>>()
                        .join("-")
                )
            });
        let target = columns
            .iter()
            .map(|column| format!("<{base}/{table}#{column}> {{{column}}}"))
            .collect::<Vec<_>>()
            .join(" ; ");
        let source_columns = columns
            .iter()
            .map(|column| format!("\"{column}\" AS \"{column}\""))
            .collect::<Vec<_>>()
            .join(", ");
        mapping.push_str(&format!("mappingId bootstrap-{table}\ntarget {subject} a <{base}/{table}> ; {target} .\nsource SELECT {source_columns} FROM \"{table}\"\n\n"));
        ontology.push_str(&format!("<{base}/{table}> a owl:Class .\n"));
        for column in columns {
            ontology.push_str(&format!(
                "<{base}/{table}#{column}> a owl:DatatypeProperty .\n"
            ));
        }
    }
    mapping.push_str("]]\n");
    std::fs::write(mapping_output, mapping).map_err(|error| {
        rtop::RuntimeError::Config(format!("无法写入 bootstrap mapping：{error}"))
    })?;
    std::fs::write(ontology_output, ontology).map_err(|error| {
        rtop::RuntimeError::Config(format!("无法写入 bootstrap ontology：{error}"))
    })?;
    Ok(())
}

fn unquote_identifier(value: &str) -> String {
    value.trim_matches('"').replace("\"\"", "\"")
}

fn prettify_r2rml(input: &str, output: &str) -> Result<(), rtop::RuntimeError> {
    let text = std::fs::read_to_string(input)
        .map_err(|error| rtop::RuntimeError::Config(format!("无法读取 R2RML：{error}")))?;
    let mut formatter = TurtleFormatter::new(Vec::new());
    let base = oxiri::Iri::parse("https://rtop.invalid/r2rml/".to_owned())
        .map_err(|error| rtop::RuntimeError::Config(format!("无效 R2RML base IRI：{error}")))?;
    TurtleParser::new(text.as_bytes(), Some(base))
        .parse_all(&mut |triple| {
            let _ = formatter.format(&triple);
            Ok::<(), rio_turtle::TurtleError>(())
        })
        .map_err(|error| rtop::RuntimeError::Mapping(format!("R2RML RDF 语法错误：{error}")))?;
    let formatted = formatter
        .finish()
        .map_err(|error| rtop::RuntimeError::Config(format!("无法格式化 R2RML：{error}")))?;
    std::fs::write(output, formatted)
        .map_err(|error| rtop::RuntimeError::Config(format!("无法写入 prettify 输出：{error}")))?;
    Ok(())
}

fn r2rml_to_obda(input: &str, output: &str) -> Result<(), rtop::RuntimeError> {
    let text = std::fs::read_to_string(input)
        .map_err(|error| rtop::RuntimeError::Config(format!("无法读取 R2RML：{error}")))?;
    let base = oxiri::Iri::parse("https://rtop.invalid/r2rml/".to_owned())
        .map_err(|error| rtop::RuntimeError::Config(format!("无效 R2RML base IRI：{error}")))?;
    let mapping = Mapping::parse_r2rml_reader(text.as_bytes(), &base.to_string())?;
    std::fs::write(output, mapping.to_native_obda())
        .map_err(|error| rtop::RuntimeError::Config(format!("无法写入 native OBDA：{error}")))?;
    Ok(())
}

fn obda_to_r2rml(input: &str, output: &str) -> Result<(), rtop::RuntimeError> {
    let text = std::fs::read_to_string(input)
        .map_err(|error| rtop::RuntimeError::Config(format!("无法读取 native OBDA：{error}")))?;
    let mapping = Mapping::parse(&text)?;
    std::fs::write(output, mapping.to_r2rml())
        .map_err(|error| rtop::RuntimeError::Config(format!("无法写入 R2RML：{error}")))?;
    Ok(())
}

fn migrate_v1_mapping(
    input: &str,
    output: &str,
    simplify_projection: bool,
) -> Result<(), rtop::RuntimeError> {
    if input.ends_with(".ttl") {
        let text = std::fs::read_to_string(input)
            .map_err(|error| rtop::RuntimeError::Config(format!("无法读取旧版 R2RML：{error}")))?;
        let base = oxiri::Iri::parse("https://rtop.invalid/r2rml/".to_owned())
            .map_err(|error| rtop::RuntimeError::Config(format!("无效 R2RML base IRI：{error}")))?;
        let mapping = Mapping::parse_r2rml_reader(text.as_bytes(), &base.to_string())?;
        let (native, _) = migrate_v1_native_text(&mapping.to_native_obda(), simplify_projection)?;
        let mapping = Mapping::parse(&native)?;
        std::fs::write(output, mapping.to_r2rml()).map_err(|error| {
            rtop::RuntimeError::Config(format!("无法写入 v1-to-v3 R2RML：{error}"))
        })?;
        return Ok(());
    }
    let text = std::fs::read_to_string(input)
        .map_err(|error| rtop::RuntimeError::Config(format!("无法读取旧版 mapping：{error}")))?;
    let (converted, properties) = migrate_v1_native_text(&text, simplify_projection)?;
    std::fs::write(output, format!("{converted}\n")).map_err(|error| {
        rtop::RuntimeError::Config(format!("无法写入 v1-to-v3 mapping：{error}"))
    })?;
    if !properties.is_empty() {
        let properties_path = std::path::Path::new(output).with_extension("properties");
        std::fs::write(&properties_path, format!("{}\n", properties.join("\n"))).map_err(
            |error| {
                rtop::RuntimeError::Config(format!("无法写入旧 datasource properties：{error}"))
            },
        )?;
    }
    Ok(())
}

fn migrate_v1_native_text(
    text: &str,
    simplify_projection: bool,
) -> Result<(String, Vec<String>), rtop::RuntimeError> {
    let mut properties = Vec::new();
    let mut converted: Vec<String> = Vec::new();
    let mut in_source_declaration = false;
    let mut target_index = None;
    for line in text.lines() {
        if line.trim() == "[SourceDeclaration]" {
            in_source_declaration = true;
            continue;
        }
        if in_source_declaration {
            if line.trim().is_empty() {
                in_source_declaration = false;
            } else {
                let mut fields = line.splitn(2, char::is_whitespace);
                let key = fields.next().unwrap_or_default().trim();
                let value = fields.next().unwrap_or_default().trim();
                let property = match key {
                    "connectionUrl" => "jdbc.url",
                    "username" => "jdbc.user",
                    "password" => "jdbc.password",
                    "driverClass" => "jdbc.driver",
                    "sourceUri" => continue,
                    _ => {
                        return Err(rtop::RuntimeError::Mapping(format!(
                            "不支持的旧 datasource 字段：{key}"
                        )))
                    }
                };
                properties.push(format!("{property}={value}"));
                continue;
            }
            continue;
        }
        if line.trim_start().starts_with("target") {
            target_index = Some(converted.len());
        }
        if line.trim_start().starts_with("source") {
            if let Some(index) = target_index {
                let (target, source) =
                    migrate_v1_rule(&converted[index], line, simplify_projection)?;
                converted[index] = target;
                converted.push(source);
                continue;
            }
        }
        converted.push(line.to_owned());
    }
    if in_source_declaration {
        return Err(rtop::RuntimeError::Mapping(
            "旧版 [SourceDeclaration] 缺少结尾空行".into(),
        ));
    }
    Ok((converted.join("\n"), properties))
}

fn migrate_v1_rule(
    target: &str,
    source: &str,
    simplify_projection: bool,
) -> Result<(String, String), rtop::RuntimeError> {
    let placeholder =
        Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*)\}").map_err(|error| {
            rtop::RuntimeError::Mapping(format!("无法建立旧 mapping placeholder 规则：{error}"))
        })?;
    let columns = placeholder
        .captures_iter(target)
        .map(|capture| capture[1].to_owned())
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Ok((target.to_owned(), source.to_owned()));
    }
    let mut aliases = std::collections::BTreeMap::new();
    for column in &columns {
        let name = column.rsplit('.').next().unwrap_or(column);
        let count = columns
            .iter()
            .filter(|candidate| candidate.ends_with(&format!(".{name}")))
            .count();
        let ordinal = columns
            .iter()
            .filter(|candidate| candidate.rsplit('.').next() == Some(name))
            .position(|candidate| candidate == column)
            .unwrap_or_default();
        aliases.insert(
            column.clone(),
            if count == 1 {
                name.to_owned()
            } else {
                format!("{name}{}", ordinal + 1)
            },
        );
    }
    let target = placeholder
        .replace_all(target, |capture: &regex::Captures<'_>| {
            format!("{{{}}}", aliases[&capture[1]])
        })
        .into_owned();
    let query = source
        .trim_start()
        .strip_prefix("source")
        .unwrap_or(source)
        .trim_start();
    let select = Regex::new(r"(?is)^select\s+(?P<projection>.*?)\s+from\b").map_err(|error| {
        rtop::RuntimeError::Mapping(format!("无法建立旧 mapping SQL 规则：{error}"))
    })?;
    let captures = select.captures(query).ok_or_else(|| {
        rtop::RuntimeError::Mapping("v1-to-v3 仅支持含 SELECT ... FROM 的 source".into())
    })?;
    let projection = captures.name("projection").expect("具名捕获存在").as_str();
    let mut rewritten = projection.to_owned();
    for (column, alias) in aliases {
        rewritten = rewritten.replace(&column, &format!("{column} AS {alias}"));
    }
    let start = captures.get(0).expect("完整匹配存在");
    let prefix = &query[..start.start()];
    let suffix = &query[start.end() - 4..];
    let rewritten_query = format!("{prefix}SELECT {rewritten} {suffix}");
    let rewritten_query = if simplify_projection {
        simplify_v1_projection(&rewritten_query)
    } else {
        rewritten_query
    };
    Ok((target, format!("source\t\t{rewritten_query}")))
}

/// 仅在 PostgreSQL parser 确认 projection 是无别名表达式且 source 没有 join 时才改成 `*`。
/// 与 Ontop v1-to-v3 相同，这保留 WHERE 等 tail；带 alias 的全限定列重命名绝不会被简化。
fn simplify_v1_projection(query: &str) -> String {
    let Ok(statements) = Parser::parse_sql(&PostgreSqlDialect {}, query) else {
        return query.to_owned();
    };
    let [Statement::Query(parsed_query)] = statements.as_slice() else {
        return query.to_owned();
    };
    let SetExpr::Select(select) = parsed_query.body.as_ref() else {
        return query.to_owned();
    };
    if select.from.iter().any(|table| !table.joins.is_empty())
        || !select
            .projection
            .iter()
            .all(|item| matches!(item, SelectItem::UnnamedExpr(_)))
    {
        return query.to_owned();
    }
    let Ok(select_pattern) = Regex::new(r"(?is)^(?P<prefix>\s*select)\s+.*?\s+(?P<from>from\b)")
    else {
        return query.to_owned();
    };
    select_pattern
        .replace(query, "${prefix} * ${from}")
        .into_owned()
}

fn turtle(fact: &RdfFact) -> String {
    format!(
        "{} <{}> {} .",
        format_rdf_term(&fact.subject),
        fact.predicate,
        format_rdf_term(&fact.object)
    )
}

fn nquad(fact: &RdfFact) -> String {
    let graph = fact
        .graph
        .as_ref()
        .map(|graph| format!(" {}", format_rdf_term(graph)))
        .unwrap_or_default();
    format!(
        "{} <{}> {}{} .",
        format_rdf_term(&fact.subject),
        fact.predicate,
        format_rdf_term(&fact.object),
        graph
    )
}
