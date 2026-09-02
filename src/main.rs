use rtop::{
    format_rdf_term, load_configuration, serve, PostgresDataSource, QueryResult, RdfTerm,
    VkgRuntime,
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
    if config.is_empty() {
        eprintln!("用法：rtop <query|validate|endpoint> <config.toml> [query-file|-|bind-address]");
        std::process::exit(64);
    }
    if command == "validate" {
        match load_configuration(&config)
            .and_then(|loaded| VkgRuntime::new(loaded.spec, ValidationSource))
        {
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
    if command != "query" {
        eprintln!("未知命令：{command}");
        std::process::exit(64);
    }
    let query_path = args.next().unwrap_or_else(|| "-".into());
    let query = if query_path == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap();
        s
    } else {
        std::fs::read_to_string(query_path).unwrap_or_default()
    };
    let outcome = (|| {
        let loaded = load_configuration(config)?;
        let source = PostgresDataSource::connect(&loaded.postgres)?;
        let mut runtime = VkgRuntime::new(loaded.spec, source)?;
        runtime.query(&query)
    })();
    match outcome {
        Ok(QueryResult::Bindings(rows)) => {
            for row in rows {
                for (name, term) in row {
                    if let RdfTerm::Iri(value) = term {
                        println!("?{name}=<{value}>");
                    }
                }
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
