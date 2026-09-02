use crate::RuntimeError;
use rio_api::{model::Term, parser::TriplesParser};
use rio_turtle::TurtleParser;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::{Path, PathBuf},
};

const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

/// 已归一化的 OWL 2 QL 最小 TBox：仅保留查询改写需要的 subclass 边。
#[derive(Default)]
pub struct Ontology {
    superclass: BTreeMap<String, BTreeSet<String>>,
    superproperty: BTreeMap<String, BTreeSet<String>>,
}

impl Ontology {
    pub fn load(path: &Path) -> Result<Self, RuntimeError> {
        let mut ontology = Self::default();
        let mut visited = BTreeSet::new();
        ontology.load_closure(path, &mut visited)?;
        Ok(ontology)
    }

    pub fn is_subclass_of(&self, child: &str, parent: &str) -> bool {
        child == parent
            || self
                .superclass
                .get(child)
                .is_some_and(|parents| parents.contains(parent))
    }

    pub fn is_subproperty_of(&self, child: &str, parent: &str) -> bool {
        child == parent
            || self
                .superproperty
                .get(child)
                .is_some_and(|parents| parents.contains(parent))
    }

    fn load_closure(
        &mut self,
        path: &Path,
        visited: &mut BTreeSet<PathBuf>,
    ) -> Result<(), RuntimeError> {
        let path = path
            .canonicalize()
            .map_err(|error| RuntimeError::Ontology(format!("无法读取 ontology：{error}")))?;
        if !visited.insert(path.clone()) {
            return Ok(());
        }
        let content = std::fs::read(&path)
            .map_err(|error| RuntimeError::Ontology(format!("无法读取 ontology：{error}")))?;
        let base = oxiri::Iri::parse(format!("file://{}", path.display()))
            .map_err(|error| RuntimeError::Ontology(format!("无效 ontology base IRI：{error}")))?;
        let mut triples = Vec::new();
        TurtleParser::new(Cursor::new(content), Some(base))
            .parse_all(&mut |triple| {
                triples.push((
                    named(triple.subject.into()),
                    triple.predicate.iri.to_owned(),
                    named(triple.object),
                ));
                Ok(()) as Result<(), rio_turtle::TurtleError>
            })
            .map_err(|error| {
                RuntimeError::Ontology(format!("ontology Turtle 解析失败：{error}"))
            })?;
        for (subject, predicate, object) in triples {
            if predicate == RDFS_SUBCLASS {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.add_subclass(subject, object);
                }
            } else if predicate == RDFS_SUBPROPERTY {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.add_subproperty(subject, object);
                }
            } else if predicate == OWL_IMPORTS {
                let Some(object) = object else { continue };
                let imported = object.strip_prefix("file://").ok_or_else(|| {
                    RuntimeError::Ontology(format!("不支持非文件 ontology import：{object}"))
                })?;
                self.load_closure(Path::new(imported), visited)?;
            }
        }
        Ok(())
    }

    fn add_subclass(&mut self, child: String, parent: String) {
        let inherited = self.superclass.get(&parent).cloned().unwrap_or_default();
        let parents = self.superclass.entry(child.clone()).or_default();
        parents.insert(parent.clone());
        parents.extend(inherited);
        for ancestors in self.superclass.values_mut() {
            if ancestors.contains(&child) {
                ancestors.insert(parent.clone());
            }
        }
    }

    fn add_subproperty(&mut self, child: String, parent: String) {
        let inherited = self.superproperty.get(&parent).cloned().unwrap_or_default();
        let parents = self.superproperty.entry(child.clone()).or_default();
        parents.insert(parent.clone());
        parents.extend(inherited);
        for ancestors in self.superproperty.values_mut() {
            if ancestors.contains(&child) {
                ancestors.insert(parent.clone());
            }
        }
    }
}

fn named(term: Term<'_>) -> Option<String> {
    match term {
        Term::NamedNode(node) => Some(node.iri.to_owned()),
        _ => None,
    }
}
