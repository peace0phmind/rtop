use rtop::{parse_nquads, parse_turtle, RdfTerm};

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
fn classifies_malformed_turtle_facts() {
    assert!(
        format!("{}", parse_turtle(b"@prefix : <bad", None).unwrap_err())
            .starts_with("invalid-facts:")
    );
}
