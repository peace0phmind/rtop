use rtop::{
    parse_nquads, parse_rdf_xml, parse_turtle, validate_static_inputs, KnowledgeGraphSpec, RdfTerm,
};

#[test]
fn preserves_blank_nodes_languages_and_datatypes_from_turtle_facts() {
    let facts = parse_turtle(b"@prefix ex: <https://example.test/> . _:a ex:p \"bonjour\"@fr . ex:s ex:p \"7\"^^<https://example.test/type> .", None).unwrap();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].subject, RdfTerm::BlankNode("a".into()));
    assert_eq!(
        facts[0].object,
        RdfTerm::Literal {
            value: "bonjour".into(),
            datatype: None,
            language: Some("fr".into())
        }
    );
    assert_eq!(
        facts[1].object,
        RdfTerm::Literal {
            value: "7".into(),
            datatype: Some("https://example.test/type".into()),
            language: None
        }
    );
}

#[test]
fn preserves_the_named_graph_from_nquads_facts() {
    let facts = parse_nquads(b"<https://example.test/s> <https://example.test/p> <https://example.test/o> <https://example.test/g> .").unwrap();
    assert_eq!(
        facts[0].graph,
        Some(RdfTerm::Iri("https://example.test/g".into()))
    );
}

#[test]
fn preserves_a_blank_node_graph_from_nquads_facts() {
    let facts =
        parse_nquads(b"_:subject <https://example.test/p> \"plain literal\" _:graph .").unwrap();
    assert_eq!(facts[0].subject, RdfTerm::BlankNode("subject".into()));
    assert_eq!(
        facts[0].object,
        RdfTerm::Literal {
            value: "plain literal".into(),
            datatype: None,
            language: None,
        }
    );
    assert_eq!(facts[0].graph, Some(RdfTerm::BlankNode("graph".into())));
}

#[test]
fn classifies_malformed_nquads_facts() {
    assert!(matches!(
        parse_nquads(b"<https://example.test/s> <https://example.test/p> \"unterminated ."),
        Err(rtop::RuntimeError::Facts(_))
    ));
}

#[test]
fn classifies_malformed_turtle_facts() {
    assert!(
        format!("{}", parse_turtle(b"@prefix : <bad", None).unwrap_err())
            .starts_with("invalid-facts:")
    );
}

#[test]
fn rejects_malformed_turtle_facts_during_endpoint_static_input_validation() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration] @collection [[\nmappingId m\ntarget <https://example.test/s> <https://example.test/p> <https://example.test/o> .\nsource SELECT 1\n]]\n").unwrap();
    std::fs::write(
        &facts,
        "@prefix : <https://example.test/> .\n<https://example.test/s> :p \"broken .\n",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: Some("turtle".into()),
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    assert!(matches!(
        validate_static_inputs(&spec, true, false),
        Err(rtop::RuntimeError::Facts(_))
    ));
}

#[test]
fn resolves_rdfxml_relative_iris_against_the_explicit_facts_base_iri() {
    let facts = parse_rdf_xml(
        br#"<?xml version="1.0"?>
        <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
                 xmlns:ex="https://example.test/">
          <rdf:Description rdf:about="person/7"><ex:name>Seven</ex:name></rdf:Description>
        </rdf:RDF>"#,
        Some(
            oxiri::Iri::parse("https://data.example.test/base/")
                .unwrap()
                .into(),
        ),
    )
    .unwrap();
    assert_eq!(
        facts[0].subject,
        RdfTerm::Iri("https://data.example.test/base/person/7".into())
    );
    assert_eq!(facts[0].predicate, "https://example.test/name");
}

#[test]
fn classifies_malformed_rdfxml_facts() {
    assert!(matches!(
        parse_rdf_xml(b"<rdf:RDF>", None),
        Err(rtop::RuntimeError::Facts(_))
    ));
}
