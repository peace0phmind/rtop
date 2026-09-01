use crate::RdfTerm;

/// 将 RDF 术语编码为 Turtle/N-Triples 术语文本，供 CLI 与图结果 adapter 共用。
pub fn format_rdf_term(value: &RdfTerm) -> String {
    match value {
        RdfTerm::Iri(value) => format!("<{value}>"),
        RdfTerm::BlankNode(value) => format!("_:{value}"),
        RdfTerm::Literal {
            value,
            language: Some(language),
            ..
        } => format!("\"{value}\"@{language}"),
        RdfTerm::Literal {
            value,
            datatype: Some(datatype),
            ..
        } => format!("\"{value}\"^^<{datatype}>"),
        RdfTerm::Literal { value, .. } => format!("\"{value}\""),
    }
}
