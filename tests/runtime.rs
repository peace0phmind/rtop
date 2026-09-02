use rtop::{DataSource, KnowledgeGraphSpec, RuntimeError, VkgRuntime};

struct FakeSource {
    sql: String,
}
impl DataSource for FakeSource {
    fn execute(&mut self, sql: &str, _: &[String]) -> Result<Vec<Vec<String>>, RuntimeError> {
        self.sql = sql.into();
        Ok(vec![vec!["7".into()]])
    }
}

#[test]
fn runs_a_select_bgp_through_the_datasource_port() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\nmappingId m\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    let source = FakeSource { sql: String::new() };
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, source).unwrap();
    let result = runtime
        .query(
            "SELECT ?person { ?person <https://example.com/type> <https://example.com/Person> . }",
        )
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"person\": Iri(\"7\")}])"
    );
}

#[test]
fn distinguishes_invalid_and_unsupported_sparql() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(matches!(
        runtime.query("SELECT ?person"),
        Err(RuntimeError::MalformedSparql(_))
    ));
    assert!(matches!(
        runtime.query("ASK { ?s ?p ?o }"),
        Err(RuntimeError::UnsupportedSparql(_))
    ));
}

#[test]
fn queries_facts_together_with_mapping_results() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("extra.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "<https://example.com/person/8> <https://example.com/type> <https://example.com/Person> .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query(
            "SELECT ?person { ?person <https://example.com/type> <https://example.com/Person> . }",
        )
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("expected bindings")
    };
    assert_eq!(rows.len(), 2);
}

#[test]
fn evaluates_ask_construct_and_describe_against_facts() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("extra.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "<https://example.com/person/8> <https://example.com/type> <https://example.com/Person> .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert_eq!(
        runtime
            .query("ASK { ?person <https://example.com/type> <https://example.com/Person> }")
            .unwrap(),
        rtop::QueryResult::Boolean(true)
    );
    let rtop::QueryResult::Graph(graph) = runtime.query("CONSTRUCT { ?person <https://example.com/type> <https://example.com/Person> } WHERE { ?person <https://example.com/type> <https://example.com/Person> }").unwrap() else { panic!("expected graph") };
    assert_eq!(graph.len(), 2);
    let rtop::QueryResult::Graph(graph) = runtime
        .query("DESCRIBE <https://example.com/person/8>")
        .unwrap()
    else {
        panic!("expected graph")
    };
    assert_eq!(graph.len(), 1);
}

#[test]
fn loads_prefixed_and_continued_native_obda_mappings() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "# 参考输入中的注释不应成为 mapping 内容\n[PrefixDeclaration]\nex: https://example.com/\n\n[MappingDeclaration]\nmappingId people\ntarget ex:person/{id} ex:type ex:Person .\nsource SELECT id\n       FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query(
            "SELECT ?person { ?person <https://example.com/type> <https://example.com/Person> . }",
        )
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"person\": Iri(\"7\")}])"
    );
}

#[test]
fn rejects_deprecated_obda_source_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[SourceDeclaration]\nconnectionUrl jdbc:postgresql://example/db\n[MappingDeclaration]\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(spec, FakeSource { sql: String::new() }), Err(RuntimeError::Mapping(message)) if message.contains("SourceDeclaration"))
    );
}

#[test]
fn loads_a_turtle_r2rml_mapping_from_the_same_runtime_seam() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> .\n[] rr:logicalTable [ rr:sqlQuery \"SELECT id FROM people\" ];\n   rr:subjectMap [ rr:template \"https://example.com/person/{id}\" ];\n   rr:predicateObjectMap [ rr:predicate <https://example.com/type>; rr:objectMap [ rr:constant <https://example.com/Person> ] ] .\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?person { ?person <https://example.com/type> <https://example.com/Person> . }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn applies_imported_subclass_axioms_when_querying_facts() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    let imported = dir.path().join("imported.ttl");
    let ontology = dir.path().join("ontology.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Student> .").unwrap();
    std::fs::write(&imported, "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . <https://example.test/Student> rdfs:subClassOf <https://example.test/Person> .").unwrap();
    std::fs::write(
        &ontology,
        "@prefix owl: <http://www.w3.org/2002/07/owl#> . <urn:root> owl:imports <imported.ttl> .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: Some(ontology),
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime.query("SELECT ?person { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Person> . }");
    assert!(
        matches!(result, Ok(rtop::QueryResult::Bindings(ref rows)) if rows.len() == 1),
        "{result:?}"
    );
}

#[test]
fn queries_literal_and_blank_node_facts_without_losing_term_identity() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "@prefix ex: <https://example.test/> . _:b ex:label \"bonjour\"@fr .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query("SELECT ?subject { ?subject <https://example.test/label> \"bonjour\"@fr . }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"subject\": BlankNode(\"b\")}])"
    );
}

#[test]
fn joins_basic_graph_patterns_on_shared_variables() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "@prefix ex: <https://example.test/> . ex:a ex:knows ex:b . ex:b ex:label \"B\" . ex:c ex:label \"C\" .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime.query("SELECT ?person ?label { ?person <https://example.test/knows> ?friend . ?friend <https://example.test/label> ?label . }").unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"label\": Literal { value: \"B\", datatype: None, language: None }, \"person\": Iri(\"https://example.test/a\")}])");
}
