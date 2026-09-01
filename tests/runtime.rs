use rtop::{DataSource, KnowledgeGraphSpec, RuntimeError, VkgRuntime};

struct FakeSource { sql: String }
impl DataSource for FakeSource { fn execute(&mut self, sql: &str, _: &[String]) -> Result<Vec<Vec<String>>, RuntimeError> { self.sql = sql.into(); Ok(vec![vec!["7".into()]]) } }

#[test]
fn runs_a_select_bgp_through_the_datasource_port() {
    let dir = tempfile::tempdir().unwrap(); let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\nmappingId m\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    let source = FakeSource { sql: String::new() };
    let spec = KnowledgeGraphSpec { mapping_file: mapping };
    let mut runtime = VkgRuntime::new(spec, source).unwrap();
    let result = runtime.query("SELECT ?person { ?person <https://example.com/type> <https://example.com/Person> . }").unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"person\": Iri(\"7\")}])");
}

#[test]
fn distinguishes_invalid_and_unsupported_sparql() {
    let dir = tempfile::tempdir().unwrap(); let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec { mapping_file: mapping };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(matches!(runtime.query("SELECT ?person"), Err(RuntimeError::MalformedSparql(_))));
    assert!(matches!(runtime.query("ASK { ?s ?p ?o }"), Err(RuntimeError::UnsupportedSparql(_))));
}
