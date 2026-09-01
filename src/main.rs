use rtop::{load_spec, PostgresDataSource, QueryResult, RdfTerm, VkgRuntime};
use std::io::Read;

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let config = args.next().unwrap_or_default();
    if command != "query" || config.is_empty() { eprintln!("usage: rtop query <config.toml> [query-file|-]"); std::process::exit(64); }
    let query_path = args.next().unwrap_or_else(|| "-".into());
    let query = if query_path == "-" { let mut s = String::new(); std::io::stdin().read_to_string(&mut s).unwrap(); s } else { std::fs::read_to_string(query_path).unwrap_or_default() };
    let outcome = (|| { let spec = load_spec(config)?; let source = PostgresDataSource::connect(&spec.database_url)?; let mut runtime = VkgRuntime::new(spec, source)?; runtime.query(&query) })();
    match outcome { Ok(QueryResult::Bindings(rows)) => for row in rows { for (name, term) in row { if let RdfTerm::Iri(value) = term { println!("?{name}=<{value}>"); } } }, Err(error) => { eprintln!("{error}"); std::process::exit(2); } }
}
