use rtop::{format_rdf_term, RdfTerm};

#[test]
fn formats_every_public_rdf_term_for_turtle_and_ntriples_adapters() {
    assert_eq!(
        format_rdf_term(&RdfTerm::Iri("https://example.test/resource".into())),
        "<https://example.test/resource>"
    );
    assert_eq!(
        format_rdf_term(&RdfTerm::BlankNode("row-7".into())),
        "_:row-7"
    );
    assert_eq!(
        format_rdf_term(&RdfTerm::Literal {
            value: "bonjour".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: Some("fr".into()),
        }),
        "\"bonjour\"@fr"
    );
    assert_eq!(
        format_rdf_term(&RdfTerm::Literal {
            value: "7".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        }),
        "\"7\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
    assert_eq!(
        format_rdf_term(&RdfTerm::Literal {
            value: "plain".into(),
            datatype: None,
            language: None,
        }),
        "\"plain\""
    );
}
