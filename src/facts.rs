use crate::{RdfFact, RdfTerm, RuntimeError};
use rio_api::{
    model::{GraphName, Literal, Subject, Term},
    parser::{QuadsParser, TriplesParser},
};
use rio_turtle::{NQuadsParser, TurtleParser};
use std::io::Cursor;

/// 从 Turtle reader 读取 RDF facts。调用者负责提供显式 base IRI（如需要）。
pub fn parse_turtle(
    reader: impl AsRef<[u8]>,
    base_iri: Option<oxiri::Iri<String>>,
) -> Result<Vec<RdfFact>, RuntimeError> {
    let mut facts = Vec::new();
    TurtleParser::new(Cursor::new(reader.as_ref()), base_iri)
        .parse_all(&mut |triple| {
            facts.push(RdfFact {
                subject: subject(triple.subject),
                predicate: triple.predicate.iri.to_owned(),
                object: term(triple.object),
                graph: None,
            });
            Ok(()) as Result<(), rio_turtle::TurtleError>
        })
        .map_err(|error| RuntimeError::Facts(format!("Turtle facts 解析失败：{error}")))?;
    Ok(facts)
}

/// 从 N-Quads reader 读取 RDF facts，并保留具名图。
pub fn parse_nquads(reader: impl AsRef<[u8]>) -> Result<Vec<RdfFact>, RuntimeError> {
    let mut facts = Vec::new();
    NQuadsParser::new(Cursor::new(reader.as_ref()))
        .parse_all(&mut |quad| {
            facts.push(RdfFact {
                subject: subject(quad.subject),
                predicate: quad.predicate.iri.to_owned(),
                object: term(quad.object),
                graph: match quad.graph_name {
                    Some(GraphName::NamedNode(node)) => Some(RdfTerm::Iri(node.iri.to_owned())),
                    Some(GraphName::BlankNode(node)) => {
                        Some(RdfTerm::BlankNode(node.id.to_owned()))
                    }
                    None => None,
                },
            });
            Ok(()) as Result<(), rio_turtle::TurtleError>
        })
        .map_err(|error| RuntimeError::Facts(format!("N-Quads facts 解析失败：{error}")))?;
    Ok(facts)
}

fn subject(value: Subject<'_>) -> RdfTerm {
    match value {
        Subject::NamedNode(node) => RdfTerm::Iri(node.iri.into()),
        Subject::BlankNode(node) => RdfTerm::BlankNode(node.id.into()),
        Subject::Triple(_) => RdfTerm::BlankNode("rdf-star-triple-not-supported".into()),
    }
}

fn term(value: Term<'_>) -> RdfTerm {
    match value {
        Term::NamedNode(node) => RdfTerm::Iri(node.iri.into()),
        Term::BlankNode(node) => RdfTerm::BlankNode(node.id.into()),
        Term::Literal(Literal::Simple { value }) => RdfTerm::Literal {
            value: value.into(),
            datatype: None,
            language: None,
        },
        Term::Literal(Literal::LanguageTaggedString { value, language }) => RdfTerm::Literal {
            value: value.into(),
            datatype: None,
            language: Some(language.into()),
        },
        Term::Literal(Literal::Typed { value, datatype }) => RdfTerm::Literal {
            value: value.into(),
            datatype: Some(datatype.iri.into()),
            language: None,
        },
        Term::Triple(_) => RdfTerm::BlankNode("rdf-star-triple-not-supported".into()),
    }
}
