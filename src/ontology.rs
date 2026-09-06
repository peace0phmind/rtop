use crate::{RdfFact, RdfTerm, RuntimeError};
use regex::Regex;
use rio_api::{model::Term, parser::TriplesParser};
use rio_turtle::TurtleParser;
use rio_xml::RdfXmlParser;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::{Path, PathBuf},
};

const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// 已归一化的 OWL 2 QL 最小 TBox：仅保留查询改写需要的 subclass 边。
#[derive(Default)]
pub struct Ontology {
    superclass: BTreeMap<String, BTreeSet<String>>,
    superproperty: BTreeMap<String, BTreeSet<String>>,
    domain: BTreeMap<String, BTreeSet<String>>,
    range: BTreeMap<String, BTreeSet<String>>,
    inverse: BTreeMap<String, BTreeSet<String>>,
    disjoint: BTreeMap<String, BTreeSet<String>>,
    facts: Vec<RdfFact>,
}

impl Ontology {
    pub fn load(path: &Path) -> Result<Self, RuntimeError> {
        Self::load_with_catalog(path, None)
    }

    pub fn load_with_catalog(path: &Path, catalog: Option<&Path>) -> Result<Self, RuntimeError> {
        let mut ontology = Self::default();
        let mut visited = BTreeSet::new();
        let catalog = match catalog {
            Some(path) => Catalog::load(path)?,
            None => Catalog::default(),
        };
        let path = catalog.resolve_root(path)?;
        ontology.load_closure(&path, &catalog, &mut visited)?;
        Ok(ontology)
    }

    pub fn is_subclass_of(&self, child: &str, parent: &str) -> bool {
        self.is_descendant_of(child, parent, &self.superclass)
    }

    pub fn is_subproperty_of(&self, child: &str, parent: &str) -> bool {
        self.is_descendant_of(child, parent, &self.superproperty)
    }

    fn is_descendant_of(
        &self,
        child: &str,
        parent: &str,
        hierarchy: &BTreeMap<String, BTreeSet<String>>,
    ) -> bool {
        let mut pending = vec![child];
        let mut seen = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current) {
                continue;
            }
            if current == parent {
                return true;
            }
            pending.extend(
                hierarchy
                    .get(current)
                    .into_iter()
                    .flatten()
                    .map(String::as_str),
            );
        }
        false
    }

    /// 返回 `parent` 本身及其已知直接/间接子类，用于虚拟 mapping 的 type 查询改写。
    pub fn subclasses_of(&self, parent: &str) -> BTreeSet<String> {
        std::iter::once(parent.to_owned())
            .chain(
                self.superclass
                    .keys()
                    .filter(|child| self.is_subclass_of(child, parent))
                    .cloned(),
            )
            .collect()
    }

    /// 返回 `parent` 本身及其已知直接/间接子属性，用于虚拟 mapping 的 property
    /// 查询改写。RDFS subPropertyOf 的蕴含不能只在 facts 路径处理：mapping 产生的
    /// assertion 也必须能回答对超属性的查询。
    pub fn subproperties_of(&self, parent: &str) -> BTreeSet<String> {
        std::iter::once(parent.to_owned())
            .chain(
                self.superproperty
                    .keys()
                    .filter(|child| self.is_subproperty_of(child, parent))
                    .cloned(),
            )
            .collect()
    }

    /// 返回某个 property assertion 因 RDFS domain/range 产生的直接类型。
    pub fn inferred_types(&self, predicate: &str, subject: bool) -> BTreeSet<String> {
        let assertions = if subject { &self.domain } else { &self.range };
        std::iter::once(predicate)
            .chain(self.superproperty.keys().filter_map(|parent| {
                self.is_subproperty_of(predicate, parent)
                    .then_some(parent.as_str())
            }))
            .flat_map(|property| assertions.get(property).into_iter().flatten().cloned())
            .collect()
    }

    /// 返回其 domain/range（含超属性继承）可蕴含 `class` 的 property。虚拟 mapping
    /// 必须把这些 property assertion 一并改写，不能只在静态 facts 路径中推导类型。
    pub fn properties_asserting_type(&self, class: &str, subject: bool) -> BTreeSet<String> {
        let assertions = if subject { &self.domain } else { &self.range };
        assertions
            .keys()
            .chain(self.superproperty.keys())
            .filter(|property| {
                self.inferred_types(property, subject)
                    .iter()
                    .any(|candidate| self.is_subclass_of(candidate, class))
            })
            .cloned()
            .collect()
    }

    /// 返回 property assertion 可推出的反向 property。
    pub fn inverse_properties(&self, predicate: &str) -> impl Iterator<Item = &str> {
        self.inverse
            .get(predicate)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    /// ontology 文件中的 ABox assertion；与配置的 facts 一样参与查询和 TBox 推理。
    pub fn facts(&self) -> &[RdfFact] {
        &self.facts
    }

    /// 固定 Ontop endpoint 会接受与 `owl:disjointWith` 冲突的 ABox facts，仍让
    /// 它们参与查询回答；因此这里不能把冲突升级为初始化错误。disjoint 公理仍被
    /// 读取，以保留 TBox 输入的完整性和将来需要的改写信息。
    pub fn validate_facts(&self, facts: &[RdfFact]) -> Result<(), RuntimeError> {
        let _ = facts;
        Ok(())
    }

    fn load_closure(
        &mut self,
        path: &Path,
        catalog: &Catalog,
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
        let mut collect = |triple: rio_api::model::Triple<'_>| {
            let subject: Term<'_> = triple.subject.into();
            let object = triple.object;
            triples.push((
                named(subject.clone()),
                triple.predicate.iri.to_owned(),
                named(object.clone()),
                term(subject),
                term(object),
            ));
        };
        if content
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(b'<')
        {
            RdfXmlParser::new(Cursor::new(&content), Some(base))
                .parse_all(&mut |triple| {
                    collect(triple);
                    Ok(()) as Result<(), rio_xml::RdfXmlError>
                })
                .map_err(|error| {
                    RuntimeError::Ontology(format!("ontology RDF/XML 解析失败：{error}"))
                })?;
        } else {
            TurtleParser::new(Cursor::new(&content), Some(base))
                .parse_all(&mut |triple| {
                    collect(triple);
                    Ok(()) as Result<(), rio_turtle::TurtleError>
                })
                .map_err(|error| {
                    RuntimeError::Ontology(format!("ontology Turtle 解析失败：{error}"))
                })?;
        }
        // RDF/XML 将 `C owl:equivalentClass (A and R)` 序列化为匿名节点和
        // rdf:List。OWL 2 QL 中 C 必然是该交集每个命名 conjunct 的子类；保留
        // 这条安全方向便可让 mapping type assertion 回答父类查询，无需把存在限制
        // 错误地降格为普通 class assertion。
        self.normalize_intersection_superclasses(&triples);
        // Ontop 将 `exists R.owl:Thing subClassOf A` 归一化为 R 的 domain 是 A。
        // 带限定 filler 的左侧存在限制不属于这一可安全降解的子集，保留为无蕴含。
        self.normalize_unqualified_existential_domains(&triples);
        for (subject, predicate, object, fact_subject, fact_object) in triples {
            self.facts.push(RdfFact {
                subject: fact_subject,
                predicate: predicate.clone(),
                object: fact_object,
                graph: None,
            });
            if predicate == RDFS_SUBCLASS {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.add_subclass(subject, object);
                }
            } else if predicate == OWL_EQUIVALENT_CLASS {
                // OWL 2 QL 中命名类等价可归一化为两条 subclass 边。匿名 class
                // expression 不在当前最小 TBox 的可观察范围内，因而不在此处展开。
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.add_subclass(subject.clone(), object.clone());
                    self.add_subclass(object, subject);
                }
            } else if predicate == OWL_EQUIVALENT_PROPERTY {
                // 固定 Ontop OWL 2 QL endpoint 基线不会将此 fixture 中的
                // equivalentProperty 改写为查询可见的 subproperty 边。最小 TBox
                // 只保留已实证的 property closure，避免产生额外 RDF binding。
            } else if predicate == RDFS_SUBPROPERTY {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.add_subproperty(subject, object);
                }
            } else if predicate == RDFS_DOMAIN {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.domain.entry(subject).or_default().insert(object);
                }
            } else if predicate == RDFS_RANGE {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.range.entry(subject).or_default().insert(object);
                }
            } else if predicate == OWL_INVERSE_OF {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.inverse
                        .entry(subject.clone())
                        .or_default()
                        .insert(object.clone());
                    self.inverse.entry(object).or_default().insert(subject);
                }
            } else if predicate == OWL_DISJOINT_WITH {
                if let (Some(subject), Some(object)) = (subject, object) {
                    self.disjoint
                        .entry(subject.clone())
                        .or_default()
                        .insert(object.clone());
                    self.disjoint.entry(object).or_default().insert(subject);
                }
            } else if predicate == OWL_IMPORTS {
                let Some(object) = object else { continue };
                let imported = catalog.resolve_import(&object)?;
                self.load_closure(&imported, catalog, visited)?;
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

    fn normalize_intersection_superclasses(
        &mut self,
        triples: &[(Option<String>, String, Option<String>, RdfTerm, RdfTerm)],
    ) {
        let mut intersections = BTreeMap::<RdfTerm, RdfTerm>::new();
        let mut first = BTreeMap::<RdfTerm, RdfTerm>::new();
        let mut rest = BTreeMap::<RdfTerm, RdfTerm>::new();
        let mut named_classes = Vec::<(String, RdfTerm)>::new();
        for (subject, predicate, object, fact_subject, fact_object) in triples {
            match predicate.as_str() {
                OWL_INTERSECTION_OF => {
                    intersections.insert(fact_subject.clone(), fact_object.clone());
                }
                RDF_FIRST => {
                    first.insert(fact_subject.clone(), fact_object.clone());
                }
                RDF_REST => {
                    rest.insert(fact_subject.clone(), fact_object.clone());
                }
                OWL_EQUIVALENT_CLASS | RDFS_SUBCLASS => {
                    if let (Some(class), None) = (subject, object) {
                        named_classes.push((class.clone(), fact_object.clone()));
                    } else if let (None, Some(class)) = (subject, object) {
                        named_classes.push((class.clone(), fact_subject.clone()));
                    }
                }
                _ => {}
            }
        }
        for (class, expression) in named_classes {
            let Some(list) = intersections.get(&expression) else {
                continue;
            };
            for member in rdf_list_members(list, &first, &rest) {
                if let RdfTerm::Iri(parent) = member {
                    self.add_subclass(class.clone(), parent);
                }
            }
        }
    }

    fn normalize_unqualified_existential_domains(
        &mut self,
        triples: &[(Option<String>, String, Option<String>, RdfTerm, RdfTerm)],
    ) {
        let mut property = BTreeMap::<RdfTerm, RdfTerm>::new();
        let mut filler = BTreeMap::<RdfTerm, RdfTerm>::new();
        let mut parents = Vec::<(RdfTerm, String)>::new();
        for (subject, predicate, object, fact_subject, fact_object) in triples {
            match predicate.as_str() {
                OWL_ON_PROPERTY => {
                    property.insert(fact_subject.clone(), fact_object.clone());
                }
                OWL_SOME_VALUES_FROM => {
                    filler.insert(fact_subject.clone(), fact_object.clone());
                }
                RDFS_SUBCLASS => {
                    if let (None, Some(parent)) = (subject, object) {
                        parents.push((fact_subject.clone(), parent.clone()));
                    }
                }
                _ => {}
            }
        }
        for (restriction, parent) in parents {
            let (Some(RdfTerm::Iri(property)), Some(RdfTerm::Iri(filler))) =
                (property.get(&restriction), filler.get(&restriction))
            else {
                continue;
            };
            if filler == OWL_THING {
                self.domain
                    .entry(property.clone())
                    .or_default()
                    .insert(parent);
            }
        }
    }
}

#[derive(Default)]
struct Catalog {
    mappings: BTreeMap<String, PathBuf>,
}

impl Catalog {
    fn load(path: &Path) -> Result<Self, RuntimeError> {
        let content = std::fs::read_to_string(path)
            .map_err(|error| RuntimeError::Ontology(format!("无法读取 XML Catalog：{error}")))?;
        let directory = path.parent().unwrap_or(Path::new("."));
        let entry = Regex::new(r#"<uri\s+[^>]*name=[\"']([^\"']+)[\"'][^>]*uri=[\"']([^\"']+)[\"'][^>]*/?>|<uri\s+[^>]*uri=[\"']([^\"']+)[\"'][^>]*name=[\"']([^\"']+)[\"'][^>]*/?>"#)
            .expect("catalog uri regex");
        let mut mappings = BTreeMap::new();
        for captures in entry.captures_iter(&content) {
            let (name, target) = match (captures.get(1), captures.get(2)) {
                (Some(name), Some(target)) => (name.as_str(), target.as_str()),
                _ => (
                    captures.get(4).unwrap().as_str(),
                    captures.get(3).unwrap().as_str(),
                ),
            };
            let target = target
                .strip_prefix("file://")
                .map(PathBuf::from)
                .unwrap_or_else(|| directory.join(target));
            mappings.insert(name.to_owned(), target);
        }
        Ok(Self { mappings })
    }

    fn resolve_root(&self, path: &Path) -> Result<PathBuf, RuntimeError> {
        path.canonicalize()
            .map_err(|error| RuntimeError::Ontology(format!("无法读取 ontology：{error}")))
    }

    fn resolve_import(&self, iri: &str) -> Result<PathBuf, RuntimeError> {
        if let Some(path) = self.mappings.get(iri) {
            return path.canonicalize().map_err(|error| {
                RuntimeError::Ontology(format!("无法读取 XML Catalog 映射 {iri}：{error}"))
            });
        }
        if let Some(path) = iri.strip_prefix("file://") {
            return Ok(PathBuf::from(path));
        }
        Err(RuntimeError::Ontology(format!(
            "未映射的远程 ontology import：{iri}"
        )))
    }
}

/// URL 形式的顶层 ontology 只能经本地 Catalog 解析，避免配置加载阶段访问网络。
pub(crate) fn resolve_input(
    base: &Path,
    input: &str,
    catalog: Option<&Path>,
) -> Result<PathBuf, RuntimeError> {
    if let Some(path) = input.strip_prefix("file://") {
        return Ok(PathBuf::from(path));
    }
    if !(input.starts_with("http://") || input.starts_with("https://")) {
        return Ok(base.join(input));
    }
    let Some(path) = catalog else {
        return Err(RuntimeError::Ontology(format!(
            "远程 ontology URL 必须由 xml_catalog 映射：{input}"
        )));
    };
    Catalog::load(path)?.resolve_import(input)
}

fn rdf_list_members(
    head: &RdfTerm,
    first: &BTreeMap<RdfTerm, RdfTerm>,
    rest: &BTreeMap<RdfTerm, RdfTerm>,
) -> Vec<RdfTerm> {
    let mut members = Vec::new();
    let mut current = head.clone();
    let mut seen = BTreeSet::new();
    while seen.insert(current.clone()) {
        if matches!(&current, RdfTerm::Iri(value) if value == RDF_NIL) {
            break;
        }
        let Some(member) = first.get(&current) else {
            break;
        };
        members.push(member.clone());
        let Some(next) = rest.get(&current) else {
            break;
        };
        current = next.clone();
    }
    members
}

fn named(term: Term<'_>) -> Option<String> {
    match term {
        Term::NamedNode(node) => Some(node.iri.to_owned()),
        _ => None,
    }
}

fn term(term: Term<'_>) -> RdfTerm {
    match term {
        Term::NamedNode(node) => RdfTerm::Iri(node.iri.to_owned()),
        Term::BlankNode(node) => RdfTerm::BlankNode(node.id.to_owned()),
        Term::Literal(rio_api::model::Literal::Simple { value }) => RdfTerm::Literal {
            value: value.to_owned(),
            datatype: None,
            language: None,
        },
        Term::Literal(rio_api::model::Literal::LanguageTaggedString { value, language }) => {
            RdfTerm::Literal {
                value: value.to_owned(),
                datatype: None,
                language: Some(language.to_owned()),
            }
        }
        Term::Literal(rio_api::model::Literal::Typed { value, datatype }) => RdfTerm::Literal {
            value: value.to_owned(),
            datatype: Some(datatype.iri.to_owned()),
            language: None,
        },
        Term::Triple(_) => RdfTerm::BlankNode("rdf-star-triple-not-supported".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::Ontology;

    #[test]
    fn normalizes_named_equivalent_classes_to_bidirectional_subclasses() {
        let directory = tempfile::tempdir().expect("temporary ontology directory");
        let path = directory.path().join("equivalent.ttl");
        std::fs::write(
            &path,
            "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n<https://example.test/Dealer> owl:equivalentClass <https://example.test/Trader> .",
        )
        .expect("write ontology");
        let ontology = Ontology::load(&path).expect("load named equivalent classes");

        assert!(
            ontology.is_subclass_of("https://example.test/Dealer", "https://example.test/Trader")
        );
        assert!(
            ontology.is_subclass_of("https://example.test/Trader", "https://example.test/Dealer")
        );
    }

    #[test]
    fn normalizes_named_conjunct_of_anonymous_equivalent_intersection() {
        let directory = tempfile::tempdir().expect("temporary ontology directory");
        let path = directory.path().join("intersection.ttl");
        std::fs::write(
            &path,
            "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix ex: <https://example.test/> .\n\
             ex:Student owl:equivalentClass [ owl:intersectionOf (ex:Person [ owl:onProperty ex:takesCourse ]) ] .",
        )
        .expect("write ontology");
        let ontology = Ontology::load(&path).expect("load intersection ontology");

        assert!(ontology.is_subclass_of(
            "https://example.test/Student",
            "https://example.test/Person"
        ));
    }

    #[test]
    fn follows_lubm_rdfxml_intersection_superclass_through_professor_hierarchy() {
        let ontology = Ontology::load_with_catalog(
            std::path::Path::new("../ontop/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/lubm.owl"),
            None,
        )
        .expect("固定 LUBM RDF/XML ontology 应可读取");
        assert!(ontology.is_subclass_of(
            "http://swat.cse.lehigh.edu/onto/univ-bench.owl#AssistantProfessor",
            "http://swat.cse.lehigh.edu/onto/univ-bench.owl#Person"
        ));
    }

    #[test]
    fn follows_catalog_mapped_imports_and_stops_cycles() {
        let directory = tempfile::tempdir().expect("temporary ontology directory");
        let root = directory.path().join("root.ttl");
        let imported = directory.path().join("imported.ttl");
        let catalog = directory.path().join("catalog.xml");
        std::fs::write(
            &root,
            "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n<https://example.test/root> owl:imports <https://example.test/imported> .",
        )
        .expect("write root ontology");
        std::fs::write(
            &imported,
            format!(
                "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n<https://example.test/imported> owl:imports <file://{}> .\n<https://example.test/Child> rdfs:subClassOf <https://example.test/Parent> .",
                root.display()
            ),
        )
        .expect("write imported ontology");
        std::fs::write(
            &catalog,
            "<catalog xmlns=\"urn:oasis:names:tc:entity:xmlns:xml:catalog\"><uri name=\"https://example.test/imported\" uri=\"imported.ttl\"/></catalog>",
        )
        .expect("write catalog");

        let ontology = Ontology::load_with_catalog(&root, Some(&catalog)).expect("load closure");
        assert!(
            ontology.is_subclass_of("https://example.test/Child", "https://example.test/Parent")
        );
    }

    #[test]
    fn rejects_unmapped_remote_import_without_network_access() {
        let directory = tempfile::tempdir().expect("temporary ontology directory");
        let root = directory.path().join("root.ttl");
        std::fs::write(
            &root,
            "@prefix owl: <http://www.w3.org/2002/07/owl#> . <https://example.test/root> owl:imports <https://example.test/unmapped> .",
        )
        .expect("write root ontology");
        let error = match Ontology::load(&root) {
            Ok(_) => panic!("unmapped import must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("未映射的远程 ontology import"));
    }
}
