use crate::{datasource::RelationMetadata, sparql::TriplePattern, RdfFact, RdfTerm, RuntimeError};
use rio_api::{
    model::{Literal, Subject, Term},
    parser::TriplesParser,
};
use rio_turtle::TurtleParser;
use sqlparser::{ast::Statement, dialect::PostgreSqlDialect, parser::Parser};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RR: &str = "http://www.w3.org/ns/r2rml#";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RdfNode {
    Iri(String),
    Blank(String),
    Literal {
        value: String,
        datatype: Option<String>,
        language: Option<String>,
    },
}

pub struct Mapping {
    rules: Vec<MappingRule>,
}

#[derive(Clone)]
struct MappingRule {
    subject_template: String,
    subject_term: BindingTerm,
    /// 仅 `rr:column` 为 true；`rr:template "{column}"` 仍须 percent-encode。
    subject_is_column: bool,
    predicate: String,
    object: String,
    object_term: BindingTerm,
    graph: Option<GraphMap>,
    source: String,
    iri_base: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
enum GraphMap {
    Constant(String),
    Template(String),
}

struct TriplesMapInfo {
    source: String,
    subject_template: String,
    subject_term: BindingTerm,
}
pub struct Plan {
    pub sql: String,
    pub parameters: Vec<String>,
    pub variables: Vec<String>,
    pub terms: Vec<BindingTerm>,
    pub iri_bases: Vec<Option<String>>,
    /// 动态 object 的常量查询在 SQL text 预过滤后仍须以实际 RDF term 校验 datatype。
    pub object_validation: Option<String>,
}
#[derive(Clone, PartialEq, Eq)]
pub enum BindingTerm {
    Iri,
    BlankNode,
    Literal {
        datatype: Option<String>,
        language: Option<String>,
        infer_datatype: bool,
    },
}

fn native_serialized_term(value: &str, term: &BindingTerm, iri_base: Option<&str>) -> String {
    match term {
        BindingTerm::Iri => {
            let value = value.trim_matches(['<', '>']);
            let value = if value.contains(':') {
                value.to_owned()
            } else if let Some(iri_base) = iri_base {
                // 与执行期 resolve_mapping_iri 一致：R2RML @base 是模板固定片段的词法前缀。
                format!("{iri_base}{value}")
            } else {
                value.to_owned()
            };
            format!("<{value}>")
        }
        BindingTerm::BlankNode => format!("_:{value}"),
        BindingTerm::Literal { .. } if value.starts_with('"') => value.to_owned(),
        BindingTerm::Literal {
            datatype, language, ..
        } => {
            let suffix = language
                .as_ref()
                .map(|language| format!("@{language}"))
                .or_else(|| datatype.as_ref().map(|datatype| format!("^^<{datatype}>")))
                .unwrap_or_default();
            format!("{value}{suffix}")
        }
    }
}

fn turtle_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn r2rml_term_map(value: &str, term: &BindingTerm, subject: bool) -> String {
    let mut fields = match term {
        BindingTerm::Iri => {
            let iri = value.trim_matches(['<', '>']);
            if iri.contains('{') {
                vec![format!("rr:template {}", turtle_literal(iri))]
            } else {
                vec![format!("rr:constant <{iri}>")]
            }
        }
        BindingTerm::BlankNode => vec![
            format!("rr:template {}", turtle_literal(value)),
            "rr:termType rr:BlankNode".into(),
        ],
        BindingTerm::Literal {
            datatype,
            language,
            infer_datatype,
        } if !infer_datatype && !subject => {
            let mut fields = vec![format!("rr:constant {}", turtle_literal(value))];
            if let Some(language) = language {
                fields.push(format!("rr:language {}", turtle_literal(language)));
            } else if let Some(datatype) = datatype {
                fields.push(format!("rr:datatype <{datatype}>"));
            }
            fields
        }
        BindingTerm::Literal {
            datatype, language, ..
        } => {
            let mut fields = if value.starts_with('{') && value.ends_with('}') {
                vec![format!(
                    "rr:column {}",
                    turtle_literal(&value[1..value.len() - 1])
                )]
            } else {
                vec![
                    format!("rr:template {}", turtle_literal(value)),
                    "rr:termType rr:Literal".into(),
                ]
            };
            if let Some(language) = language {
                fields.push(format!("rr:language {}", turtle_literal(language)));
            } else if let Some(datatype) = datatype {
                fields.push(format!("rr:datatype <{datatype}>"));
            }
            fields
        }
    };
    if subject && matches!(term, BindingTerm::Literal { .. }) {
        fields.push("rr:termType rr:Literal".into());
    }
    format!("[ {} ]", fields.join(" ; "))
}

fn r2rml_graph_map(graph: &GraphMap) -> String {
    match graph {
        GraphMap::Constant(graph) => format!("[ rr:constant <{graph}> ]"),
        GraphMap::Template(graph) => format!("[ rr:template {} ]", turtle_literal(graph)),
    }
}
impl Mapping {
    /// 将已解析的映射写成可由 rtop 再次读取的 native OBDA 子集。
    ///
    /// 输出刻意使用完整 IRI，避免转换结果依赖 R2RML 输入的 prefix 排列。每条 RDF
    /// rule 独立成为一个 mappingId；这保留了 R2RML 展开后的 source、term type 与 graph
    /// 语义，也让生成的文件可直接通过 native parser 重载。
    pub fn to_native_obda(&self) -> String {
        let mut output = String::from("[MappingDeclaration] @collection [[\n");
        for (index, rule) in self.rules.iter().enumerate() {
            let subject = native_serialized_term(
                &rule.subject_template,
                &rule.subject_term,
                rule.iri_base.as_deref(),
            );
            let object =
                native_serialized_term(&rule.object, &rule.object_term, rule.iri_base.as_deref());
            let triple = format!("{subject} {} {object} .", rule.predicate);
            let target = match &rule.graph {
                None => triple,
                Some(GraphMap::Constant(graph)) | Some(GraphMap::Template(graph)) => {
                    format!("GRAPH <{graph}> {{ {triple} }}")
                }
            };
            output.push_str(&format!(
                "mappingId\trtop-r2rml-{}\ntarget\t\t{target}\nsource\t\t{}\n\n",
                index + 1,
                rule.source
            ));
        }
        output.push_str("]]\n");
        output
    }

    /// 将 native mapping 的已解析规则写为标准 R2RML Turtle。
    ///
    /// 该转换是 force 模式：不连接数据库猜测 identifier quoting，而是精确保留 native
    /// source 与 target 中已解析的列拼写。每条规则独立为一个 rr:TriplesMap，避免在
    /// 无 metadata 时错误合并逻辑表或 term map。
    pub fn to_r2rml(&self) -> String {
        let mut output = String::from("@prefix rr: <http://www.w3.org/ns/r2rml#> .\n\n");
        for (index, rule) in self.rules.iter().enumerate() {
            output.push_str(&format!(
                "<rtop-triples-map-{}> a rr:TriplesMap ;\n",
                index + 1
            ));
            output.push_str(&format!(
                "  rr:logicalTable [ rr:sqlQuery {} ] ;\n",
                turtle_literal(&rule.source)
            ));
            let subject_map = r2rml_term_map(&rule.subject_template, &rule.subject_term, true);
            let subject_map = match &rule.graph {
                None => subject_map,
                Some(graph) => format!(
                    "[ {} ; rr:graphMap {} ]",
                    subject_map.trim_matches(['[', ']']).trim(),
                    r2rml_graph_map(graph)
                ),
            };
            output.push_str(&format!("  rr:subjectMap {subject_map} ;\n"));
            output.push_str(&format!(
                "  rr:predicateObjectMap [ rr:predicate {} ; rr:objectMap {} ] .\n\n",
                rule.predicate,
                r2rml_term_map(&rule.object, &rule.object_term, false)
            ));
        }
        output
    }

    /// 由 PostgreSQL catalog 元数据规划 W3C RDB2RDF Direct Mapping。
    ///
    /// 这不是 R2RML 的序列化或导入：规则直接由 relation、键和外键构造，并复用
    /// 同一条 Mapping/SQL 执行通路。调用者必须传入同一 schema 中所有需要暴露的
    /// relation metadata，以便外键能引用 parent relation 的主键形状。
    pub fn from_direct_mapping(
        base_iri: &str,
        relations: &[(String, RelationMetadata)],
    ) -> Result<Self, RuntimeError> {
        let _base = oxiri::Iri::parse(base_iri.to_owned()).map_err(|error| {
            RuntimeError::Mapping(format!("无效 Direct Mapping base IRI：{error}"))
        })?;
        if !base_iri.contains(':') {
            return Err(RuntimeError::Mapping(
                "Direct Mapping base IRI 必须是绝对 IRI".into(),
            ));
        }
        let metadata = relations
            .iter()
            .map(|(table, relation)| (table, relation))
            .collect::<BTreeMap<_, _>>();
        let mut rules = Vec::new();
        for (table, relation) in relations {
            if relation.columns.is_empty() {
                return Err(RuntimeError::Mapping(format!(
                    "Direct Mapping relation `{table}` 不存在或没有 column"
                )));
            }
            let source = direct_source(table, relation);
            let (subject_template, subject_term) = direct_subject(base_iri, table, relation);
            rules.push(MappingRule {
                subject_template: subject_template.clone(),
                subject_term: subject_term.clone(),
                subject_is_column: false,
                predicate: format!("<{RDF_TYPE}>"),
                object: format!("<{base_iri}{}>", direct_iri_identifier(table)),
                object_term: BindingTerm::Iri,
                graph: None,
                source: source.clone(),
                iri_base: None,
            });
            for column in &relation.columns {
                rules.push(MappingRule {
                    subject_template: subject_template.clone(),
                    subject_term: subject_term.clone(),
                    subject_is_column: false,
                    predicate: format!(
                        "<{base_iri}{}#{}>",
                        direct_iri_identifier(table),
                        direct_iri_identifier(&column.name)
                    ),
                    object: format!("{{{}}}", column.name),
                    object_term: BindingTerm::Literal {
                        datatype: None,
                        language: None,
                        infer_datatype: true,
                    },
                    graph: None,
                    source: source.clone(),
                    iri_base: None,
                });
            }
            for foreign_key in &relation.foreign_keys {
                let parent = metadata.get(&foreign_key.referenced_table).ok_or_else(|| {
                    RuntimeError::Mapping(format!(
                        "Direct Mapping 外键 `{}` 的 parent relation `{}` 未提供",
                        foreign_key.name, foreign_key.referenced_table
                    ))
                })?;
                let (object, object_term, rule_source) = if parent.primary_key.is_empty() {
                    (
                        format!(
                            "{}-{{__rtop_direct_parent_row_id}}",
                            direct_iri_identifier(&foreign_key.referenced_table)
                        ),
                        BindingTerm::BlankNode,
                        direct_foreign_key_source(table, relation, foreign_key, &[]),
                    )
                } else if parent.primary_key == foreign_key.referenced_columns {
                    (
                        format!(
                            "<{}>",
                            direct_iri_template(
                                base_iri,
                                &foreign_key.referenced_table,
                                &foreign_key.referenced_columns,
                                &foreign_key.columns,
                            )
                        ),
                        BindingTerm::Iri,
                        source.clone(),
                    )
                } else {
                    let parent_key_aliases = parent
                        .primary_key
                        .iter()
                        .enumerate()
                        .map(|(index, _)| format!("__rtop_direct_parent_pk_{index}"))
                        .collect::<Vec<_>>();
                    (
                        format!(
                            "<{}>",
                            direct_iri_template(
                                base_iri,
                                &foreign_key.referenced_table,
                                &parent.primary_key,
                                &parent_key_aliases,
                            )
                        ),
                        BindingTerm::Iri,
                        direct_foreign_key_source(
                            table,
                            relation,
                            foreign_key,
                            &parent.primary_key,
                        ),
                    )
                };
                rules.push(MappingRule {
                    subject_template: subject_template.clone(),
                    subject_term: subject_term.clone(),
                    subject_is_column: false,
                    predicate: format!(
                        "<{base_iri}{}#ref-{}>",
                        direct_iri_identifier(table),
                        foreign_key
                            .columns
                            .iter()
                            .map(|column| direct_iri_identifier(column))
                            .collect::<Vec<_>>()
                            .join(";")
                    ),
                    object,
                    object_term,
                    graph: None,
                    source: rule_source,
                    iri_base: None,
                });
            }
        }
        Ok(Self { rules })
    }

    pub fn describe(&self) -> Result<Vec<Plan>, RuntimeError> {
        self.rules
            .iter()
            .map(|rule| {
                rule.reformulate(
                    &TriplePattern {
                        subject: "?resource".into(),
                        predicate: rule.predicate.clone(),
                        object: rule.object.clone(),
                        graph: None,
                    },
                    &["resource".into()],
                )
            })
            .collect()
    }

    pub fn mapped_facts(&self, subject: String) -> Vec<RdfFact> {
        self.rules
            .iter()
            .filter(|rule| !rule.object.starts_with('{'))
            .map(|rule| RdfFact {
                subject: RdfTerm::Iri(subject.clone()),
                predicate: rule.predicate.trim_matches(['<', '>']).into(),
                object: RdfTerm::Iri(rule.object.trim_matches(['<', '>']).into()),
                graph: None,
            })
            .collect()
    }
    pub fn parse_file(path: &Path) -> Result<Self, RuntimeError> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| RuntimeError::Mapping(format!("无法读取 mapping：{error}")))?;
        if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("ttl" | "turtle")
        ) {
            let base_iri = oxiri::Iri::parse(format!(
                "file://{}",
                path.canonicalize()
                    .unwrap_or_else(|_| path.to_path_buf())
                    .display()
            ))
            .map_err(|error| RuntimeError::Mapping(format!("无效 R2RML base IRI：{error}")))?;
            Self::parse_r2rml(&text, base_iri)
        } else {
            Self::parse(&text)
        }
    }

    pub fn parse_r2rml_reader<R: Read>(
        mut reader: R,
        base_iri: &str,
    ) -> Result<Self, RuntimeError> {
        let mut text = String::new();
        reader
            .read_to_string(&mut text)
            .map_err(|error| RuntimeError::Mapping(format!("无法读取 R2RML reader：{error}")))?;
        let base_iri = oxiri::Iri::parse(base_iri.to_owned())
            .map_err(|error| RuntimeError::Mapping(format!("无效 R2RML base IRI：{error}")))?;
        Self::parse_r2rml(&text, base_iri)
    }

    pub fn parse(text: &str) -> Result<Self, RuntimeError> {
        if text.contains("[SourceDeclaration]") {
            return Err(RuntimeError::Mapping(
                "不支持已废弃的 [SourceDeclaration]；请在 rtop 配置中声明 datasource".into(),
            ));
        }
        let lines: Vec<_> = text
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect();
        if !lines
            .iter()
            .any(|line| line.trim().starts_with("[MappingDeclaration]"))
        {
            return Err(RuntimeError::Mapping("缺少 [MappingDeclaration]".into()));
        }
        let prefixes = prefixes(&lines)?;
        let starts = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.trim().starts_with("mappingId").then_some(index))
            .collect::<Vec<_>>();
        let sections = if starts.is_empty() {
            vec![(0, lines.len())]
        } else {
            starts
                .iter()
                .enumerate()
                .map(|(index, start)| (*start, *starts.get(index + 1).unwrap_or(&lines.len())))
                .collect()
        };
        let mut rules = Vec::new();
        for (start, end) in sections {
            let section = &lines[start..end];
            let target = section
                .iter()
                .find_map(|line| line.trim().strip_prefix("target").map(str::trim))
                .ok_or_else(|| RuntimeError::Mapping("mapping 必须包含 target".into()))?;
            let source_at = section
                .iter()
                .position(|line| line.trim().starts_with("source"))
                .ok_or_else(|| RuntimeError::Mapping("mapping 必须包含 source".into()))?;
            let mut source = section[source_at]
                .trim()
                .strip_prefix("source")
                .unwrap()
                .trim()
                .to_owned();
            for line in section.iter().skip(source_at + 1) {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('[') || trimmed.starts_with("]]") {
                    break;
                }
                source.push(' ');
                source.push_str(trimmed);
            }
            // Ontop 原生 OBDA 的 source SQL 常以 statement terminator 结束；rtop 会把
            // 它嵌进 FROM (...)，因此此处只剥离末尾分号而不改变 SQL 主体。
            source = source.trim_end().trim_end_matches(';').trim_end().into();
            validate_source_sql(&source)?;
            for (subject, predicate, object, graph) in parse_native_target(target, &prefixes)? {
                rules.push(MappingRule {
                    subject_template: subject.0,
                    subject_term: subject.1,
                    subject_is_column: false,
                    predicate,
                    object: object.0,
                    object_term: object.1,
                    graph,
                    source: source.clone(),
                    iri_base: None,
                });
            }
        }
        Ok(Self { rules })
    }

    fn parse_r2rml(text: &str, base_iri: oxiri::Iri<String>) -> Result<Self, RuntimeError> {
        let iri_base = r2rml_base(text);
        let mut triples = Vec::new();
        TurtleParser::new(Cursor::new(text), Some(base_iri))
            .parse_all(&mut |triple| {
                triples.push((
                    node_from_subject(triple.subject),
                    triple.predicate.iri.to_owned(),
                    node_from_term(triple.object),
                ));
                Ok(()) as Result<(), rio_turtle::TurtleError>
            })
            .map_err(|error| RuntimeError::Mapping(format!("R2RML RDF 语法错误：{error}")))?;
        let triples_maps = triples
            .iter()
            .filter_map(|(subject, predicate, object)| {
                ((predicate == RDF_TYPE && *object == RdfNode::Iri(format!("{RR}TriplesMap")))
                    || predicate == &format!("{RR}logicalTable"))
                    .then_some(subject.clone())
            })
            .collect::<std::collections::BTreeSet<_>>();
        if triples_maps.is_empty() {
            return Err(RuntimeError::Mapping(
                "R2RML 结构错误：缺少 rr:TriplesMap".into(),
            ));
        }
        let mut rules = Vec::new();
        for triples_map in triples_maps {
            let logical_table = object_for(&triples, &triples_map, &format!("{RR}logicalTable"))
                .ok_or_else(|| {
                    RuntimeError::Mapping("R2RML 结构错误：缺少 rr:logicalTable".into())
                })?;
            validate_sql_version(&triples, logical_table)?;
            let source = literal_for(&triples, logical_table, &format!("{RR}sqlQuery"))
                .or_else(|| {
                    literal_for(&triples, logical_table, &format!("{RR}tableName"))
                        .map(|table| format!("SELECT * FROM {table}"))
                })
                .ok_or_else(|| {
                    RuntimeError::Mapping("R2RML 结构错误：缺少 rr:sqlQuery 或 rr:tableName".into())
                })?;
            // 与原生 OBDA source 一样，R2RML rr:sqlQuery 可能以 SQL statement
            // terminator 结束。mapping rule 会将其作为 FROM (...) 子查询使用，
            // PostgreSQL 不接受嵌套分号，故只归一化末尾 terminator。
            let source = source
                .trim_end()
                .trim_end_matches(';')
                .trim_end()
                .to_owned();
            validate_source_sql(&source)?;
            let subject_maps = objects_for(&triples, &triples_map, &format!("{RR}subjectMap"));
            let subject_map = match subject_maps.as_slice() {
                [subject_map] => *subject_map,
                [] => {
                    return Err(RuntimeError::Mapping(
                        "R2RML 结构错误：缺少 rr:subjectMap".into(),
                    ))
                }
                _ => {
                    return Err(RuntimeError::Mapping(
                        "R2RML 结构错误：TriplesMap 只能有一个 rr:subjectMap".into(),
                    ))
                }
            };
            let (template, subject_is_column) = if let Some(template) =
                literal_for(&triples, subject_map, &format!("{RR}template"))
            {
                (template, false)
            } else if let Some(column) = literal_for(&triples, subject_map, &format!("{RR}column"))
            {
                (format!("{{{column}}}"), true)
            } else if let Some(constant) = iri_for(&triples, subject_map, &format!("{RR}constant"))
            {
                (constant, false)
            } else {
                return Err(RuntimeError::Mapping(
                    "R2RML 结构错误：缺少 rr:constant、rr:template 或 rr:column".into(),
                ));
            };
            let direct_graph = iri_for(&triples, subject_map, &format!("{RR}graph"));
            let graph_map = object_for(&triples, subject_map, &format!("{RR}graphMap"));
            let graph = direct_graph.map(GraphMap::Constant).or_else(|| {
                graph_map.and_then(|graph_map| {
                    iri_for(&triples, graph_map, &format!("{RR}constant"))
                        .map(GraphMap::Constant)
                        .or_else(|| {
                            literal_for(&triples, graph_map, &format!("{RR}template"))
                                .map(GraphMap::Template)
                        })
                })
            });
            if let Some(graph_map) = graph_map {
                if graph.is_none()
                    || !matches!(
                        iri_for(&triples, graph_map, &format!("{RR}termType")).as_deref(),
                        None | Some("http://www.w3.org/ns/r2rml#IRI")
                    )
                {
                    return Err(RuntimeError::Mapping(
                        "R2RML 结构错误：graphMap 必须是 IRI rr:constant 或 rr:template".into(),
                    ));
                }
            }
            let graph = match graph {
                Some(GraphMap::Constant(value)) if value == format!("{RR}defaultGraph") => None,
                graph => graph,
            };
            let subject_term =
                match iri_for(&triples, subject_map, &format!("{RR}termType")).as_deref() {
                    None | Some("http://www.w3.org/ns/r2rml#IRI") => BindingTerm::Iri,
                    Some("http://www.w3.org/ns/r2rml#BlankNode") => BindingTerm::BlankNode,
                    Some(term_type) => {
                        return Err(RuntimeError::Mapping(format!(
                            "R2RML 结构错误：subjectMap 不支持的 termType `{term_type}`"
                        )))
                    }
                };
            validate_source_sql(&source)?;
            let predicate_objects =
                objects_for(&triples, &triples_map, &format!("{RR}predicateObjectMap"));
            if predicate_objects.is_empty() {
                return Err(RuntimeError::Mapping(
                    "R2RML 结构错误：缺少 rr:predicateObjectMap".into(),
                ));
            }
            rules.extend(
                predicate_objects
                    .into_iter()
                    .map(|predicate_object| {
                        let mut predicates = objects_for(
                            &triples,
                            predicate_object,
                            &format!("{RR}predicate"),
                        )
                        .into_iter()
                        .map(|predicate| match predicate {
                            RdfNode::Iri(predicate) => Ok(predicate.clone()),
                            _ => Err(RuntimeError::Mapping(
                                "R2RML 结构错误：rr:predicate 必须是 IRI".into(),
                            )),
                        })
                        .collect::<Result<Vec<_>, RuntimeError>>()?;
                        predicates.extend(
                            objects_for(&triples, predicate_object, &format!("{RR}predicateMap"))
                                .into_iter()
                                .map(|predicate_map| {
                                    iri_for(&triples, predicate_map, &format!("{RR}constant"))
                                        .ok_or_else(|| RuntimeError::Mapping(
                                            "R2RML 结构错误：predicateMap 必须有 IRI rr:constant".into(),
                                        ))
                                })
                                .collect::<Result<Vec<_>, RuntimeError>>()?,
                        );
                        if predicates.is_empty() {
                            return Err(RuntimeError::Mapping(
                                "R2RML 结构错误：缺少 rr:predicate".into(),
                            ));
                        }
                        let mut rule_graphs = graph.clone().into_iter().collect::<Vec<_>>();
                        for predicate_graph in
                            objects_for(&triples, predicate_object, &format!("{RR}graph"))
                        {
                            let RdfNode::Iri(predicate_graph) = predicate_graph else {
                                return Err(RuntimeError::Mapping(
                                    "R2RML 结构错误：PredicateObjectMap rr:graph 必须是 IRI"
                                        .into(),
                                ));
                            };
                            let predicate_graph = GraphMap::Constant(predicate_graph.clone());
                            if !rule_graphs.contains(&predicate_graph) {
                                rule_graphs.push(predicate_graph);
                            }
                        }
                        let rule_graphs = if rule_graphs.is_empty() {
                            vec![None]
                        } else {
                            rule_graphs.into_iter().map(Some).collect::<Vec<_>>()
                        };
                        if let Some(object) =
                            iri_for(&triples, predicate_object, &format!("{RR}object"))
                        {
                            let mut variants = Vec::new();
                            for predicate in predicates {
                                for graph in &rule_graphs {
                                    variants.push(MappingRule {
                                        subject_template: format!("<{template}>"),
                                        subject_term: subject_term.clone(),
                                        subject_is_column,
                                        predicate: format!("<{predicate}>"),
                                        object: format!("<{object}>"),
                                        object_term: BindingTerm::Iri,
                                        graph: graph.clone(),
                                        source: source.clone(),
                                        iri_base: iri_base.clone(),
                                    });
                                }
                            }
                            return Ok(variants);
                        }
                        if let Some(RdfNode::Literal {
                            value,
                            datatype,
                            language,
                        }) = literal_node_for(&triples, predicate_object, &format!("{RR}object"))
                        {
                            let suffix = language
                                .as_ref()
                                .map(|language| format!("@{language}"))
                                .or_else(|| {
                                    datatype.as_ref().map(|datatype| format!("^^<{datatype}>"))
                                })
                                .unwrap_or_default();
                            let mut variants = Vec::new();
                            for predicate in predicates {
                                for graph in &rule_graphs {
                                    variants.push(MappingRule {
                                        subject_template: format!("<{template}>"),
                                        subject_term: subject_term.clone(),
                                        subject_is_column,
                                        predicate: format!("<{predicate}>"),
                                        object: format!("\"{value}\"{suffix}"),
                                        object_term: BindingTerm::Literal {
                                            datatype: datatype.clone(),
                                            language: language.clone(),
                                            infer_datatype: false,
                                        },
                                        graph: graph.clone(),
                                        source: source.clone(),
                                        iri_base: iri_base.clone(),
                                    });
                                }
                            }
                            return Ok(variants);
                        }
                        let object_map =
                            object_for(&triples, predicate_object, &format!("{RR}objectMap"))
                                .ok_or_else(|| {
                                    RuntimeError::Mapping(
                                        "R2RML 结构错误：PredicateObjectMap 缺少 rr:objectMap"
                                            .into(),
                                    )
                                })?;
                        let (object, object_term, rule_source) = if let Some(iri) =
                            iri_for(&triples, object_map, &format!("{RR}constant"))
                        {
                            (format!("<{iri}>"), BindingTerm::Iri, source.clone())
                        } else if let Some(RdfNode::Literal {
                            value,
                            datatype,
                            language,
                        }) =
                            literal_node_for(&triples, object_map, &format!("{RR}constant"))
                        {
                            let suffix = language
                                .as_ref()
                                .map(|language| format!("@{language}"))
                                .or_else(|| {
                                    datatype.as_ref().map(|datatype| format!("^^<{datatype}>"))
                                })
                                .unwrap_or_default();
                            (format!("\"{value}\"{suffix}"), BindingTerm::Literal { datatype, language, infer_datatype: true }, source.clone())
                        } else if let Some(template) =
                            literal_for(&triples, object_map, &format!("{RR}template"))
                        {
                            let object_term = match iri_for(&triples, object_map, &format!("{RR}termType")).as_deref() {
                                None | Some("http://www.w3.org/ns/r2rml#IRI") => BindingTerm::Iri,
                                Some("http://www.w3.org/ns/r2rml#Literal") => BindingTerm::Literal { datatype: None, language: None, infer_datatype: true },
                                Some("http://www.w3.org/ns/r2rml#BlankNode") => BindingTerm::BlankNode,
                                Some(_) => return Err(RuntimeError::Mapping("R2RML 结构错误：不支持的 rr:objectMap termType".into())),
                            };
                            let object = if matches!(object_term, BindingTerm::Iri) {
                                format!("<{template}>")
                            } else {
                                template
                            };
                            (object, object_term, source.clone())
                        } else if let Some(parent_triples_map) =
                            iri_for(&triples, object_map, &format!("{RR}parentTriplesMap"))
                        {
                            let parent = triples_map_info(
                                &triples,
                                &RdfNode::Iri(parent_triples_map),
                            )?;
                            let (object_template, parent_projection) =
                                ref_object_parent_projection(&parent.subject_template)?;
                            let join_conditions = objects_for(
                                &triples,
                                object_map,
                                &format!("{RR}joinCondition"),
                            );
                            let rule_source = if !join_conditions.is_empty() {
                                let conditions = join_conditions
                                    .into_iter()
                                    .map(|join_condition| {
                                        let child_column = literal_for(
                                            &triples,
                                            join_condition,
                                            &format!("{RR}child"),
                                        )
                                        .ok_or_else(|| RuntimeError::Mapping(
                                            "R2RML 结构错误：joinCondition 缺少 rr:child".into(),
                                        ))?;
                                        let parent_column = literal_for(
                                            &triples,
                                            join_condition,
                                            &format!("{RR}parent"),
                                        )
                                        .ok_or_else(|| RuntimeError::Mapping(
                                            "R2RML 结构错误：joinCondition 缺少 rr:parent".into(),
                                        ))?;
                                        Ok(format!(
                                            "child.{child_column} = parent.{parent_column}"
                                        ))
                                    })
                                    .collect::<Result<Vec<_>, RuntimeError>>()?
                                    .join(" AND ");
                                format!(
                                    "SELECT child.*, {parent_projection} FROM ({source}) AS child JOIN ({}) AS parent ON {conditions}",
                                    parent.source,
                                )
                            } else {
                                format!(
                                    "SELECT child.*, {parent_projection} FROM ({source}) AS child CROSS JOIN ({}) AS parent",
                                    parent.source,
                                )
                            };
                            (
                                format!("<{object_template}>"),
                                parent.subject_term,
                                rule_source,
                            )
                        } else if let Some(column) =
                            literal_for(&triples, object_map, &format!("{RR}column"))
                        {
                            let term_type = iri_for(&triples, object_map, &format!("{RR}termType"));
                            let datatype = iri_for(&triples, object_map, &format!("{RR}datatype"));
                            let language =
                                literal_for(&triples, object_map, &format!("{RR}language"));
                            if datatype.is_some() && language.is_some() {
                                return Err(RuntimeError::Mapping(
                                "R2RML 结构错误：ObjectMap 不能同时声明 rr:datatype 与 rr:language"
                                    .into(),
                            ));
                            }
                            if language
                                .as_deref()
                                .is_some_and(|value| !valid_language_tag(value))
                            {
                                return Err(RuntimeError::Mapping(
                                    "R2RML 结构错误：rr:language 不是有效的 BCP47 语言标签".into(),
                                ));
                            }
                            let object_term = match term_type.as_deref() {
                                None | Some("http://www.w3.org/ns/r2rml#Literal") => BindingTerm::Literal {
                                    datatype,
                                    language,
                                    infer_datatype: true,
                                },
                                Some("http://www.w3.org/ns/r2rml#IRI") => BindingTerm::Iri,
                                Some("http://www.w3.org/ns/r2rml#BlankNode") => BindingTerm::BlankNode,
                                Some(_) => return Err(RuntimeError::Mapping("R2RML 结构错误：不支持的 rr:objectMap termType".into())),
                            };
                            (format!("{{{column}}}"), object_term, source.clone())
                        } else {
                            return Err(RuntimeError::Mapping(
                            "R2RML 结构错误：ObjectMap 缺少 rr:constant、rr:template 或 rr:column".into(),
                            ));
                        };
                        let mut variants = Vec::new();
                        for predicate in predicates {
                            for graph in &rule_graphs {
                                variants.push(MappingRule {
                                    subject_template: format!("<{template}>"),
                                    subject_term: subject_term.clone(),
                                    subject_is_column,
                                    predicate: format!("<{predicate}>"),
                                    object: object.clone(),
                                    object_term: object_term.clone(),
                                    graph: graph.clone(),
                                    source: rule_source.clone(),
                                    iri_base: iri_base.clone(),
                                });
                            }
                        }
                        Ok(variants)
                    })
                    .collect::<Result<Vec<Vec<_>>, RuntimeError>>()?
                    .into_iter()
                    .flatten(),
            );
            rules.extend(
                objects_for(&triples, subject_map, &format!("{RR}class"))
                    .into_iter()
                    .map(|class| match class {
                        RdfNode::Iri(class) => Ok(MappingRule {
                            subject_template: format!("<{template}>"),
                            subject_term: subject_term.clone(),
                            subject_is_column,
                            predicate: "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>".into(),
                            object: format!("<{class}>"),
                            object_term: BindingTerm::Iri,
                            graph: graph.clone(),
                            source: source.clone(),
                            iri_base: iri_base.clone(),
                        }),
                        _ => Err(RuntimeError::Mapping(
                            "R2RML 结构错误：rr:class 必须是 IRI".into(),
                        )),
                    })
                    .collect::<Result<Vec<_>, RuntimeError>>()?,
            );
        }
        Ok(Self { rules })
    }
    pub fn reformulate(
        &self,
        query: &TriplePattern,
        variables: &[String],
    ) -> Result<Vec<Plan>, RuntimeError> {
        let plans = self
            .rules
            .iter()
            .filter(|rule| {
                (match (query.graph.as_deref(), rule.graph.as_ref()) {
                    (None, None) => true,
                    (Some(query), Some(GraphMap::Constant(rule))) => query == rule,
                    (Some(_), Some(GraphMap::Template(_))) => true,
                    _ => false,
                }) && rule.predicate == query.predicate
                    && (query.object.starts_with('?')
                        || rule.object == query.object
                        || rule.object.contains('{'))
            })
            .map(|rule| rule.reformulate(query, variables))
            .collect::<Result<Vec<_>, _>>()?;
        if plans.is_empty() {
            Err(RuntimeError::NotFullyTranslatable(
                "三元组模式不匹配 mapping rules".into(),
            ))
        } else {
            Ok(plans)
        }
    }

    pub fn reformulate_predicate_variable(
        &self,
        query: &TriplePattern,
        variables: &[String],
    ) -> Result<Vec<(Plan, String)>, RuntimeError> {
        let mut plans = Vec::new();
        for rule in &self.rules {
            let mut candidate = query.clone();
            candidate.predicate = rule.predicate.clone();
            if let Ok(candidate_plans) = self.reformulate(&candidate, variables) {
                plans.extend(
                    candidate_plans
                        .into_iter()
                        .map(|plan| (plan, rule.predicate.clone())),
                );
            }
        }
        if plans.is_empty() {
            Err(RuntimeError::NotFullyTranslatable(
                "三元组模式不匹配 mapping rules".into(),
            ))
        } else {
            Ok(plans)
        }
    }
}

impl MappingRule {
    fn reformulate(
        &self,
        query: &TriplePattern,
        variables: &[String],
    ) -> Result<Plan, RuntimeError> {
        let subject_template = quoted_alias_template(&self.subject_template, &self.source);
        if self.predicate != query.predicate
            || (!query.object.starts_with('?')
                && self.object != query.object
                && !self.object.contains('{'))
            || (!query.subject.starts_with('?')
                && subject_template != query.subject
                && !subject_template.contains('{'))
        {
            return Err(RuntimeError::NotFullyTranslatable(
                format!(
                    "三元组模式不匹配 mapping：query predicate={} object={}；rule predicate={} object={}",
                    query.predicate, query.object, self.predicate, self.object
                ),
            ));
        }
        if variables.len() == 1
            && !query.subject.starts_with('?')
            && query.object.starts_with('?')
            && variables[0] == query.object.trim_start_matches('?')
        {
            let mut parameters = Vec::new();
            let projection = self.object_projection(&mut parameters)?;
            let graph_filter = match (&query.graph, &self.graph) {
                (Some(query_graph), Some(GraphMap::Template(template))) => {
                    let projection = iri_template_projection(template, &mut parameters)?;
                    parameters.push(query_graph.clone());
                    Some(format!("{projection} = ${}", parameters.len()))
                }
                _ => None,
            };
            let subject_filter = if subject_template.contains('{') {
                let iri = subject_template.trim_matches('<').trim_matches('>');
                let projection =
                    if matches!(&self.subject_term, BindingTerm::Iri) && !self.subject_is_column {
                        iri_template_projection(iri, &mut parameters)?
                    } else {
                        template_projection(iri, &mut parameters)?
                    };
                parameters.push(query.subject.trim_matches(['<', '>']).into());
                Some(format!("{projection} = ${}", parameters.len()))
            } else {
                None
            };
            let filters = [graph_filter, subject_filter]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            return Ok(Plan {
                sql: format!(
                    "SELECT {projection} FROM ({}) AS rtop_mapping{}",
                    self.source,
                    (!filters.is_empty())
                        .then(|| format!(" WHERE {}", filters.join(" AND ")))
                        .unwrap_or_default()
                ),
                parameters,
                variables: variables.to_vec(),
                terms: vec![self.object_term.clone()],
                iri_bases: vec![matches!(&self.object_term, BindingTerm::Iri)
                    .then(|| self.iri_base.clone())
                    .flatten()],
                object_validation: None,
            });
        }
        if variables.is_empty() {
            let mut parameters = Vec::new();
            let subject_filter = if subject_template.contains('{') {
                let iri = subject_template.trim_matches('<').trim_matches('>');
                let projection =
                    if matches!(&self.subject_term, BindingTerm::Iri) && !self.subject_is_column {
                        iri_template_projection(iri, &mut parameters)?
                    } else {
                        template_projection(iri, &mut parameters)?
                    };
                parameters.push(query.subject.trim_matches(['<', '>']).into());
                Some(format!("{projection} = ${}", parameters.len()))
            } else {
                None
            };
            return Ok(Plan {
                sql: format!(
                    "SELECT 1 FROM ({}) AS rtop_mapping{}",
                    self.source,
                    subject_filter
                        .map(|filter| format!(" WHERE {filter}"))
                        .unwrap_or_default()
                ),
                parameters,
                variables: Vec::new(),
                terms: Vec::new(),
                iri_bases: Vec::new(),
                object_validation: None,
            });
        }
        if variables.len() > 2 || variables[0] != query.subject.trim_start_matches('?') {
            return Err(RuntimeError::NotFullyTranslatable(
                "最小切片只投影 subject 变量".into(),
            ));
        }
        let iri = subject_template.trim_matches('<').trim_matches('>');
        let mut parameters = Vec::new();
        let subject_projection = if iri.contains('{') {
            if matches!(&self.subject_term, BindingTerm::Iri) && !self.subject_is_column {
                iri_template_projection(iri, &mut parameters)?
            } else {
                template_projection(iri, &mut parameters)?
            }
        } else {
            parameters.push(iri.into());
            format!("CAST(${} AS text)", parameters.len())
        };
        let needs_object_validation = !query.object.starts_with('?')
            && self.object.contains('{')
            && matches!(self.object_term, BindingTerm::Literal { .. });
        let mut plan_variables = variables.to_vec();
        if needs_object_validation {
            plan_variables.push("__rtop_constant_object".into());
        }
        let mut projections = vec![subject_projection];
        let mut terms = vec![self.subject_term.clone()];
        let mut iri_bases = vec![matches!(&self.subject_term, BindingTerm::Iri)
            .then(|| self.iri_base.clone())
            .flatten()];
        if plan_variables.len() == 2 {
            projections.push(self.object_projection(&mut parameters)?);
            terms.push(self.object_term.clone());
            iri_bases.push(
                matches!(&self.object_term, BindingTerm::Iri)
                    .then(|| self.iri_base.clone())
                    .flatten(),
            );
        }
        let graph_filter = match (&query.graph, &self.graph) {
            (Some(query_graph), Some(GraphMap::Template(template))) => {
                let projection = iri_template_projection(template, &mut parameters)?;
                parameters.push(query_graph.clone());
                Some(format!("{projection} = ${}", parameters.len()))
            }
            _ => None,
        };
        let subject_filter = if !query.subject.starts_with('?') && subject_template.contains('{') {
            let projection =
                if matches!(&self.subject_term, BindingTerm::Iri) && !self.subject_is_column {
                    iri_template_projection(iri, &mut parameters)?
                } else {
                    template_projection(iri, &mut parameters)?
                };
            parameters.push(query.subject.trim_matches(['<', '>']).into());
            Some(format!("{projection} = ${}", parameters.len()))
        } else {
            None
        };
        // 动态 literal 的 SQL text cast 不是 RDF lexical 的无损表示：例如 PostgreSQL
        // CHAR(n)::text 会去掉尾空格。该类常量由 object_validation 在构造 RDF term 后
        // 精确比较；只向数据库下推 IRI 或非 literal 的安全过滤。
        let object_filter = if !needs_object_validation
            && !query.object.starts_with('?')
            && self.object.contains('{')
        {
            let projection = self.object_projection(&mut parameters)?;
            parameters.push(query_object_value(&query.object)?);
            // 常量 RDF literal 在 adapter 端是 Rust String。将 PostgreSQL 列显式转为 text
            // 可避免 DATE/TIME/NUMERIC 等列把 `$n` 推断为数据库专属二进制参数类型，同时
            // 仍保持值通过参数绑定而非 SQL 拼接。
            Some(format!(
                "CAST({projection} AS text) = ${}",
                parameters.len()
            ))
        } else {
            None
        };
        let filters = [graph_filter, subject_filter, object_filter]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        Ok(Plan {
            sql: format!(
                "SELECT {} FROM ({}) AS rtop_mapping{}",
                projections.join(", "),
                self.source,
                (!filters.is_empty())
                    .then(|| format!(" WHERE {}", filters.join(" AND ")))
                    .unwrap_or_default()
            ),
            parameters,
            variables: plan_variables,
            terms,
            iri_bases,
            object_validation: needs_object_validation.then(|| query.object.clone()),
        })
    }

    fn object_projection(&self, parameters: &mut Vec<String>) -> Result<String, RuntimeError> {
        let object = quoted_alias_template(&self.object, &self.source);
        if object.starts_with('{') && object.matches('{').count() == 1 {
            let object_column = object.trim_matches(['{', '}']);
            Ok(object_column.into())
        } else if object.contains('{') {
            let object_template = object.trim_matches(['<', '>']);
            if matches!(&self.object_term, BindingTerm::Iri) {
                iri_template_projection(object_template, parameters)
            } else {
                template_projection(object_template, parameters)
            }
        } else {
            let value = object
                .strip_prefix('<')
                .and_then(|value| value.strip_suffix('>'))
                .map(str::to_owned)
                .or_else(|| {
                    self.object.strip_prefix('"').and_then(|value| {
                        value
                            .rsplit_once('"')
                            .map(|(literal, _)| literal.to_owned())
                    })
                })
                .or_else(|| {
                    matches!(self.object_term, BindingTerm::Literal { .. })
                        .then(|| self.object.clone())
                })
                .ok_or_else(|| RuntimeError::Mapping("无效的常量 object".into()))?;
            parameters.push(value);
            Ok(format!("CAST(${} AS text)", parameters.len()))
        }
    }
}

fn direct_source(table: &str, relation: &RelationMetadata) -> String {
    let columns = relation
        .columns
        .iter()
        .map(|column| {
            let quoted = postgres_identifier(&column.name);
            format!("{quoted} AS {quoted}")
        })
        .collect::<Vec<_>>();
    let mut projections = vec!["ctid::text AS \"__rtop_direct_row_id\"".into()];
    projections.extend(columns);
    format!(
        "SELECT {} FROM {}",
        projections.join(", "),
        postgres_identifier(table)
    )
}

/// parent 没有 primary key 时，RDB2RDF Direct Mapping 将其 row 表示为 blank node。
/// 外键 rule 必须 join parent 才能投影同一个 parent `ctid`，而不是用 child foreign-key
/// 值伪造 blank-node identity。
fn direct_foreign_key_source(
    child_table: &str,
    child: &RelationMetadata,
    foreign_key: &crate::RelationForeignKey,
    parent_primary_key: &[String],
) -> String {
    let mut projections = vec!["child.ctid::text AS \"__rtop_direct_row_id\"".into()];
    projections.extend(child.columns.iter().map(|column| {
        let quoted = postgres_identifier(&column.name);
        format!("child.{quoted} AS {quoted}")
    }));
    projections.push("parent.ctid::text AS \"__rtop_direct_parent_row_id\"".into());
    projections.extend(
        parent_primary_key
            .iter()
            .enumerate()
            .map(|(index, column)| {
                format!(
                    "parent.{} AS {}",
                    postgres_identifier(column),
                    postgres_identifier(&format!("__rtop_direct_parent_pk_{index}")),
                )
            }),
    );
    let conditions = foreign_key
        .columns
        .iter()
        .zip(&foreign_key.referenced_columns)
        .map(|(child_column, parent_column)| {
            format!(
                "child.{} = parent.{}",
                postgres_identifier(child_column),
                postgres_identifier(parent_column)
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ");
    format!(
        "SELECT {} FROM {} AS child JOIN {} AS parent ON {conditions}",
        projections.join(", "),
        postgres_identifier(child_table),
        postgres_identifier(&foreign_key.referenced_table)
    )
}

fn direct_subject(
    base_iri: &str,
    table: &str,
    relation: &RelationMetadata,
) -> (String, BindingTerm) {
    if relation.primary_key.is_empty() {
        (
            format!("{table}-{{__rtop_direct_row_id}}"),
            BindingTerm::BlankNode,
        )
    } else {
        (
            direct_iri_template(
                base_iri,
                table,
                &relation.primary_key,
                &relation.primary_key,
            ),
            BindingTerm::Iri,
        )
    }
}

fn direct_iri_template(
    base_iri: &str,
    table: &str,
    predicate_columns: &[String],
    source_columns: &[String],
) -> String {
    let pairs = predicate_columns
        .iter()
        .zip(source_columns)
        .map(|(predicate, source)| format!("{}={{{source}}}", direct_iri_identifier(predicate)))
        .collect::<Vec<_>>();
    format!(
        "{base_iri}{}/{}",
        direct_iri_identifier(table),
        pairs.join(";")
    )
}

/// Direct Mapping 的 relation/column 名是 IRI 固定片段而非 SQL token，因此采用
/// 与 template component 一致的保守 ASCII percent-encoding。
fn direct_iri_identifier(identifier: &str) -> String {
    let mut result = identifier.replace('%', "%25");
    for (character, escaped) in [
        (" ", "%20"),
        (":", "%3A"),
        ("/", "%2F"),
        ("\"", "%22"),
        ("<", "%3C"),
        (">", "%3E"),
        ("#", "%23"),
        ("?", "%3F"),
        ("{", "%7B"),
        ("}", "%7D"),
        ("|", "%7C"),
        ("\\", "%5C"),
        ("^", "%5E"),
        ("`", "%60"),
        (",", "%2C"),
        ("(", "%28"),
        (")", "%29"),
    ] {
        result = result.replace(character, escaped);
    }
    result
}

fn postgres_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn query_object_value(value: &str) -> Result<String, RuntimeError> {
    if value.starts_with('<') && value.ends_with('>') {
        return Ok(value.trim_matches(['<', '>']).into());
    }
    // SPARQL 的裸数值是带 XSD numeric datatype 的 literal，但 PostgreSQL mapping
    // 的动态 object source 以 text 参数过滤；保留其词法以匹配数据库投影。datatype
    // identity 仍由运行时 RDF term 层处理，不能把它当作普通未加引号 token 拒绝。
    if value.parse::<f64>().is_ok() {
        return Ok(value.into());
    }
    query_literal_value(value)
}

/// PostgreSQL 允许子查询为列指定区分大小写的 quoted alias。OBDA template 使用
/// `{ALIAS}` 表示该输出列时，外层 SQL 必须继续引用 `"ALIAS"`，否则 PostgreSQL 会把
/// 未引用的 `ALIAS` 折叠成小写。仅转换 source 中 `AS "..."` 声明过的槽位。
fn quoted_alias_template(template: &str, source: &str) -> String {
    let upper = source.to_ascii_uppercase();
    let mut offset = 0;
    let mut result = template.to_owned();
    while let Some(found) = upper[offset..].find("AS \"") {
        let start = offset + found + 4;
        let Some(end) = source[start..].find('"').map(|index| start + index) else {
            break;
        };
        let alias = &source[start..end];
        result = result.replace(&format!("{{{alias}}}"), &format!("{{\"{alias}\"}}"));
        offset = end + 1;
    }
    result
}

fn query_literal_value(token: &str) -> Result<String, RuntimeError> {
    let Some(rest) = token.strip_prefix('"') else {
        return Err(RuntimeError::NotFullyTranslatable(
            "mapping 动态 object 的常量查询仅支持 RDF literal".into(),
        ));
    };
    let mut escaped = false;
    for (index, character) in rest.char_indices() {
        if character == '"' && !escaped {
            let mut value = rest[..index].replace("\\\"", "\"");
            // PostgreSQL `time with time zone` 的 text 输出将整点偏移写为 `-08`，
            // 而 SPARQL xsd:time 常见词法是 `-08:00`。两者表示相同 offset；为 SQL
            // text 常量比较规范化这一种 PostgreSQL 词法差异。
            if !value.contains('T')
                && value.matches(':').count() >= 2
                && value.len() >= 6
                && value.as_bytes()[value.len() - 3] == b':'
                && matches!(value.as_bytes()[value.len() - 6], b'+' | b'-')
            {
                value.truncate(value.len() - 3);
            }
            return Ok(value);
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    Err(RuntimeError::MalformedSparql("literal 缺少结束引号".into()))
}

#[cfg(test)]
mod tests {
    use super::{query_literal_value, query_object_value, Mapping};
    use crate::{
        sparql::{parse, GraphPattern, Query, TriplePattern},
        RelationColumn, RelationMetadata,
    };

    #[test]
    fn normalizes_postgres_timetz_offset_for_text_parameter() {
        assert_eq!(
            query_literal_value("\"18:12:10-08:00\"^^<http://www.w3.org/2001/XMLSchema#time>")
                .unwrap(),
            "18:12:10-08"
        );
    }

    #[test]
    fn accepts_a_bare_numeric_sparql_object_for_dynamic_mapping_filter() {
        assert_eq!(query_object_value("-12.6").unwrap(), "-12.6");
        assert_eq!(query_object_value("+3").unwrap(), "+3");
    }

    #[test]
    fn plans_direct_mapping_predicates_with_sparql_iri_tokens() {
        let mapping = Mapping::from_direct_mapping(
            "http://example.com/base/",
            &[(
                "成分".into(),
                RelationMetadata {
                    columns: vec![RelationColumn {
                        name: "皿".into(),
                        nullable: true,
                    }],
                    primary_key: vec![],
                    foreign_keys: vec![],
                },
            )],
        )
        .unwrap();
        assert_eq!(
            mapping.rules[1].predicate,
            "<http://example.com/base/成分#皿>"
        );
        assert!(mapping
            .reformulate(
                &TriplePattern {
                    subject: "?component".into(),
                    predicate: "<http://example.com/base/成分#皿>".into(),
                    object: "?dish".into(),
                    graph: None,
                },
                &["component".into(), "dish".into()],
            )
            .is_ok());
        let Query::Select { pattern: GraphPattern::Bgp(patterns), .. } = parse(
            "SELECT ?dish ?component WHERE { ?component <http://example.com/base/成分#皿> ?dish . }",
        )
        .unwrap() else { panic!("expected BGP select") };
        assert_eq!(patterns[0].predicate, "<http://example.com/base/成分#皿>");
        assert!(mapping
            .reformulate(&patterns[0], &["component".into(), "dish".into()])
            .is_ok());
    }
}

fn template_projection(
    template: &str,
    parameters: &mut Vec<String>,
) -> Result<String, RuntimeError> {
    let mut expression = Vec::new();
    let mut placeholders = 0;
    // R2RML D016e uses `data:*;hex,{column}` for a binary payload. PostgreSQL
    // bytea 的 text cast 带有 `\\x` 前缀，不能形成基线 data IRI；在该明确的
    // hex data-IRI 模式下由服务端直接投影无前缀的大写十六进制词法。
    let data_hex_template = template.starts_with("data:") && template.contains(";hex,");
    for part in template_parts(template)? {
        match part {
            TemplatePart::Literal(value) => {
                parameters.push(value);
                expression.push(format!("${}", parameters.len()));
            }
            TemplatePart::Column(column) => {
                if data_hex_template {
                    expression.push(format!("upper(encode({column}, 'hex'))"));
                } else {
                    expression.push(format!("CAST({column} AS text)"));
                }
                placeholders += 1;
            }
        }
    }
    if placeholders == 0 {
        return Err(RuntimeError::Mapping(
            "R2RML 模板必须包含至少一个 {column} 槽位".into(),
        ));
    }
    Ok(expression.join(" || "))
}

/// 将 parent subject template 的列投影到 child 查询中。
/// 单列保持既有 `__rtop_parent_subject` 别名；多列使用独立别名，避免将多个
/// parent 列压缩为一个不透明字符串后再拼装 RDF IRI。
fn ref_object_parent_projection(template: &str) -> Result<(String, String), RuntimeError> {
    let parts = template_parts(template)?;
    let columns = parts
        .iter()
        .filter(|part| matches!(part, TemplatePart::Column(_)))
        .count();
    if columns == 0 {
        return Err(RuntimeError::Mapping(
            "RefObjectMap parent subject template 必须包含至少一个 {column} 槽位".into(),
        ));
    }
    if columns == 1 {
        let parent_subject_column = template_column(template)?;
        return Ok((
            template.replace(
                &format!("{{{parent_subject_column}}}"),
                "{__rtop_parent_subject}",
            ),
            format!("parent.{parent_subject_column} AS __rtop_parent_subject"),
        ));
    }
    let mut object_template = String::new();
    let mut projections = Vec::new();
    let mut index = 0;
    for part in parts {
        match part {
            TemplatePart::Literal(value) => object_template.push_str(&value),
            TemplatePart::Column(column) => {
                let alias = format!("__rtop_parent_subject_{index}");
                object_template.push_str(&format!("{{{alias}}}"));
                projections.push(format!("parent.{column} AS {alias}"));
                index += 1;
            }
        }
    }
    Ok((object_template, projections.join(", ")))
}

fn r2rml_base(text: &str) -> Option<String> {
    ["@base", "BASE"].into_iter().find_map(|keyword| {
        text.split_once(keyword)
            .and_then(|(_, value)| value.trim().strip_prefix('<'))
            .and_then(|value| value.split_once('>').map(|(base, _)| base.to_owned()))
    })
}

/// R2RML 的 IRI template 只对 column 槽位做 percent-encoding；固定片段保留原样。
fn iri_template_projection(
    template: &str,
    parameters: &mut Vec<String>,
) -> Result<String, RuntimeError> {
    let mut expression = Vec::new();
    let mut placeholders = 0;
    let data_hex_template = template.starts_with("data:") && template.contains(";hex,");
    for part in template_parts(template)? {
        match part {
            TemplatePart::Literal(value) => {
                parameters.push(value);
                expression.push(format!("${}", parameters.len()));
            }
            TemplatePart::Column(column) => {
                if data_hex_template {
                    expression.push(format!("upper(encode({column}, 'hex'))"));
                } else {
                    expression.push(iri_component_projection(&column));
                }
                placeholders += 1;
            }
        }
    }
    if placeholders == 0 {
        return Err(RuntimeError::Mapping(
            "R2RML 模板必须包含至少一个 {column} 槽位".into(),
        ));
    }
    Ok(expression.join(" || "))
}

enum TemplatePart {
    Literal(String),
    Column(String),
}

fn template_parts(template: &str) -> Result<Vec<TemplatePart>, RuntimeError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = template.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\\' if matches!(chars.peek(), Some('{' | '}' | '\\')) => {
                literal.push(chars.next().expect("peeked template escape"));
            }
            '{' => {
                parts.push(TemplatePart::Literal(std::mem::take(&mut literal)));
                let mut column = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(character) => column.push(character),
                        None => return Err(RuntimeError::Mapping("R2RML 模板缺少 `}`".into())),
                    }
                }
                if column.is_empty() {
                    return Err(RuntimeError::Mapping("R2RML 模板包含空 column 槽位".into()));
                }
                parts.push(TemplatePart::Column(column));
            }
            '}' => return Err(RuntimeError::Mapping("R2RML 模板包含未转义的 `}`".into())),
            character => literal.push(character),
        }
    }
    parts.push(TemplatePart::Literal(literal));
    Ok(parts)
}

fn iri_component_projection(column: &str) -> String {
    let mut expression = format!("CAST({column} AS text)");
    for (character, escaped) in [
        ("%", "%25"),
        (" ", "%20"),
        (":", "%3A"),
        ("/", "%2F"),
        ("\"", "%22"),
        ("<", "%3C"),
        (">", "%3E"),
        ("#", "%23"),
        ("?", "%3F"),
        ("{", "%7B"),
        ("}", "%7D"),
        ("|", "%7C"),
        ("\\", "%5C"),
        ("^", "%5E"),
        ("`", "%60"),
        (",", "%2C"),
        ("(", "%28"),
        (")", "%29"),
    ] {
        expression = format!("replace({expression}, '{character}', '{escaped}')");
    }
    expression
}

/// R2RML 的标准 SQL 版本标识符是 rr:SQL2008；其他 IRI 不能静默按 PostgreSQL
/// 方言执行，否则会把基线规定的非一致 mapping 误接受。
fn validate_sql_version(
    triples: &[(RdfNode, String, RdfNode)],
    logical_table: &RdfNode,
) -> Result<(), RuntimeError> {
    if let Some(version) = iri_for(triples, logical_table, &format!("{RR}sqlVersion")) {
        if version != format!("{RR}SQL2008") {
            return Err(RuntimeError::Mapping(format!(
                "R2RML 结构错误：不支持的 rr:sqlVersion <{version}>"
            )));
        }
    }
    Ok(())
}

fn triples_map_info(
    triples: &[(RdfNode, String, RdfNode)],
    triples_map: &RdfNode,
) -> Result<TriplesMapInfo, RuntimeError> {
    let logical_table =
        object_for(triples, triples_map, &format!("{RR}logicalTable")).ok_or_else(|| {
            RuntimeError::Mapping("R2RML 结构错误：parentTriplesMap 缺少 rr:logicalTable".into())
        })?;
    validate_sql_version(triples, logical_table)?;
    let source = literal_for(triples, logical_table, &format!("{RR}sqlQuery"))
        .or_else(|| {
            literal_for(triples, logical_table, &format!("{RR}tableName"))
                .map(|table| format!("SELECT * FROM {table}"))
        })
        .ok_or_else(|| {
            RuntimeError::Mapping("R2RML 结构错误：parentTriplesMap 缺少 SQL source".into())
        })?;
    // parentTriplesMap 的 logicalTable 与普通 TriplesMap 一样会嵌入 FROM (...)；
    // 两条解析路径必须一致地移除仅作为 statement terminator 的末尾分号。
    let source = source
        .trim_end()
        .trim_end_matches(';')
        .trim_end()
        .to_owned();
    validate_source_sql(&source)?;
    let subject_map =
        object_for(triples, triples_map, &format!("{RR}subjectMap")).ok_or_else(|| {
            RuntimeError::Mapping("R2RML 结构错误：parentTriplesMap 缺少 rr:subjectMap".into())
        })?;
    let subject_template = literal_for(triples, subject_map, &format!("{RR}template"))
        .or_else(|| {
            literal_for(triples, subject_map, &format!("{RR}column"))
                .map(|column| format!("{{{column}}}"))
        })
        .ok_or_else(|| {
            RuntimeError::Mapping(
                "R2RML 结构错误：parent subjectMap 缺少 rr:template 或 rr:column".into(),
            )
        })?;
    let subject_term = match iri_for(triples, subject_map, &format!("{RR}termType")).as_deref() {
        None | Some("http://www.w3.org/ns/r2rml#IRI") => BindingTerm::Iri,
        Some("http://www.w3.org/ns/r2rml#BlankNode") => BindingTerm::BlankNode,
        Some(_) => {
            return Err(RuntimeError::Mapping(
                "R2RML 结构错误：不支持的 parent subjectMap termType".into(),
            ))
        }
    };
    Ok(TriplesMapInfo {
        source,
        subject_template,
        subject_term,
    })
}

fn template_column(template: &str) -> Result<&str, RuntimeError> {
    template
        .split_once('{')
        .and_then(|(_, rest)| rest.split_once('}').map(|(column, _)| column))
        .ok_or_else(|| {
            RuntimeError::Mapping("R2RML 结构错误：parent subject 模板缺少 {column}".into())
        })
}

fn node_from_subject(subject: Subject<'_>) -> RdfNode {
    match subject {
        Subject::NamedNode(node) => RdfNode::Iri(node.iri.into()),
        Subject::BlankNode(node) => RdfNode::Blank(node.id.into()),
        Subject::Triple(_) => RdfNode::Blank("rdf-star-triple-not-supported".into()),
    }
}

fn node_from_term(term: Term<'_>) -> RdfNode {
    match term {
        Term::NamedNode(node) => RdfNode::Iri(node.iri.into()),
        Term::BlankNode(node) => RdfNode::Blank(node.id.into()),
        Term::Literal(Literal::Simple { value }) => RdfNode::Literal {
            value: value.into(),
            datatype: None,
            language: None,
        },
        Term::Literal(Literal::LanguageTaggedString { value, language }) => RdfNode::Literal {
            value: value.into(),
            datatype: None,
            language: Some(language.into()),
        },
        Term::Literal(Literal::Typed { value, datatype }) => RdfNode::Literal {
            value: value.into(),
            datatype: Some(datatype.iri.into()),
            language: None,
        },
        Term::Triple(_) => RdfNode::Blank("rdf-star-triple-not-supported".into()),
    }
}

fn object_for<'a>(
    triples: &'a [(RdfNode, String, RdfNode)],
    subject: &RdfNode,
    predicate: &str,
) -> Option<&'a RdfNode> {
    triples.iter().find_map(|(candidate, relation, object)| {
        (candidate == subject && relation == predicate).then_some(object)
    })
}

fn objects_for<'a>(
    triples: &'a [(RdfNode, String, RdfNode)],
    subject: &RdfNode,
    predicate: &str,
) -> Vec<&'a RdfNode> {
    triples
        .iter()
        .filter_map(|(candidate, relation, object)| {
            (candidate == subject && relation == predicate).then_some(object)
        })
        .collect()
}

fn literal_for(
    triples: &[(RdfNode, String, RdfNode)],
    subject: &RdfNode,
    predicate: &str,
) -> Option<String> {
    match object_for(triples, subject, predicate)? {
        RdfNode::Literal { value, .. } => Some(value.clone()),
        _ => None,
    }
}

fn literal_node_for(
    triples: &[(RdfNode, String, RdfNode)],
    subject: &RdfNode,
    predicate: &str,
) -> Option<RdfNode> {
    match object_for(triples, subject, predicate)? {
        literal @ RdfNode::Literal { .. } => Some(literal.clone()),
        _ => None,
    }
}

fn iri_for(
    triples: &[(RdfNode, String, RdfNode)],
    subject: &RdfNode,
    predicate: &str,
) -> Option<String> {
    match object_for(triples, subject, predicate)? {
        RdfNode::Iri(value) => Some(value.clone()),
        _ => None,
    }
}

fn validate_source_sql(source: &str) -> Result<(), RuntimeError> {
    let statements = match Parser::parse_sql(&PostgreSqlDialect {}, source) {
        Ok(statements) => statements,
        // sqlparser 0.52 尚不能解析 PostgreSQL table function 的 `WITH ORDINALITY`。
        // 它是 nested lens 展平保留数组位置的必要语法。此处不将任意解析失败的 SQL
        // 放行：仅接受不含分号、以 SELECT 开头且同时带 LATERAL/ORDINALITY 的单语句，
        // 其余输入仍给出原有稳定诊断，并交由 PostgreSQL 服务端解析实际方言。
        Err(_) if postgres_lateral_ordinality_select(source) => return Ok(()),
        Err(error) => {
            return Err(RuntimeError::Mapping(format!(
                "mapping source SQL 语法无效：{error}"
            )))
        }
    };
    if !matches!(statements.as_slice(), [Statement::Query(_)]) {
        return Err(RuntimeError::Mapping(
            "mapping source SQL 不受支持：必须恰好为一个 SELECT 查询".into(),
        ));
    }
    Ok(())
}

fn postgres_lateral_ordinality_select(source: &str) -> bool {
    let normalized = source.trim();
    normalized.len() >= "SELECT".len()
        && normalized[.."SELECT".len()].eq_ignore_ascii_case("SELECT")
        && !normalized.contains(';')
        && normalized
            .to_ascii_uppercase()
            .contains("CROSS JOIN LATERAL")
        && normalized.to_ascii_uppercase().contains("WITH ORDINALITY")
}

fn valid_language_tag(value: &str) -> bool {
    let mut parts = value.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    (2..=3).contains(&primary.len())
        && primary.bytes().all(|byte| byte.is_ascii_alphabetic())
        && parts.all(|part| {
            !part.is_empty()
                && part.len() <= 8
                && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

fn prefixes(lines: &[&str]) -> Result<BTreeMap<String, String>, RuntimeError> {
    let mut result = BTreeMap::new();
    // Ontop 原生 OBDA 测试资产可省略 XML Schema 的常用前缀声明。
    result.insert("xsd:".into(), "http://www.w3.org/2001/XMLSchema#".into());
    let mut in_prefixes = false;
    for line in lines {
        let line = line.trim();
        if line == "[PrefixDeclaration]" {
            in_prefixes = true;
            continue;
        }
        if line.starts_with('[') {
            in_prefixes = false;
        }
        if in_prefixes && !line.is_empty() {
            let (prefix, iri) = line
                .split_once(char::is_whitespace)
                .ok_or_else(|| RuntimeError::Mapping("无效的 prefix declaration".into()))?;
            result.insert(prefix.into(), iri.trim().trim_matches(['<', '>']).into());
        }
    }
    Ok(result)
}

fn expand(term: &str, prefixes: &BTreeMap<String, String>) -> Result<String, RuntimeError> {
    if term.starts_with('<') || term.starts_with('"') || term.starts_with('_') {
        return Ok(term.into());
    }
    let (prefix, local) = term
        .split_once(':')
        .ok_or_else(|| RuntimeError::Mapping(format!("无效的 mapping RDF term：{term}")))?;
    let key = format!("{prefix}:");
    let iri = prefixes
        .get(&key)
        .ok_or_else(|| RuntimeError::Mapping(format!("未声明的 prefix：{key}")))?;
    Ok(format!("<{iri}{local}>"))
}

/// 解析 Ontop 原生 target 的实用子集，而不是把 `.obda` target 当作空白分隔的三元组。
/// `;` 复用 subject，`.` 开始下一条 statement，`a` 是 rdf:type 的缩写。
fn parse_native_target(
    target: &str,
    prefixes: &BTreeMap<String, String>,
) -> Result<
    Vec<(
        (String, BindingTerm),
        String,
        (String, BindingTerm),
        Option<GraphMap>,
    )>,
    RuntimeError,
> {
    let tokens = native_target_tokens(target)?;
    let mut index = 0;
    let mut triples = Vec::new();
    while index < tokens.len() {
        if tokens[index] != "GRAPH" {
            return Ok(parse_native_triples(&tokens[index..], prefixes)?
                .into_iter()
                .map(|(subject, predicate, object)| (subject, predicate, object, None))
                .collect());
        }
        let graph_token = tokens
            .get(index + 1)
            .ok_or_else(|| RuntimeError::Mapping("原生 GRAPH target 缺少图名".into()))?;
        let graph = expand(graph_token, prefixes)?;
        let graph = graph.trim_matches(['<', '>']).to_owned();
        if tokens.get(index + 2).map(String::as_str) != Some("{") {
            return Err(RuntimeError::Mapping("原生 GRAPH target 缺少 `{`".into()));
        }
        let mut end = index + 3;
        while tokens.get(end).map(String::as_str) != Some("}") {
            if end == tokens.len() {
                return Err(RuntimeError::Mapping("原生 GRAPH target 缺少 `}`".into()));
            }
            end += 1;
        }
        let graph = if graph.contains('{') {
            GraphMap::Template(graph)
        } else {
            GraphMap::Constant(graph)
        };
        triples.extend(
            parse_native_triples(&tokens[index + 3..end], prefixes)?
                .into_iter()
                .map(|(subject, predicate, object)| {
                    (subject, predicate, object, Some(graph.clone()))
                }),
        );
        index = end + 1;
    }
    Ok(triples)
}

fn parse_native_triples(
    tokens: &[String],
    prefixes: &BTreeMap<String, String>,
) -> Result<Vec<((String, BindingTerm), String, (String, BindingTerm))>, RuntimeError> {
    let mut index = 0;
    let mut triples = Vec::new();
    let mut subject = None;
    let mut need_subject = true;
    while index < tokens.len() {
        if need_subject {
            if matches!(tokens[index].as_str(), ";" | ".") {
                return Err(RuntimeError::Mapping(
                    "原生 mapping target 缺少 subject".into(),
                ));
            }
            subject = Some(native_subject(&tokens[index], prefixes)?);
            index += 1;
            need_subject = false;
        }
        let predicate_token = tokens
            .get(index)
            .ok_or_else(|| RuntimeError::Mapping("原生 mapping target 缺少 predicate".into()))?;
        if matches!(predicate_token.as_str(), ";" | ".") {
            return Err(RuntimeError::Mapping(
                "原生 mapping target 缺少 predicate".into(),
            ));
        }
        let predicate = if predicate_token == "a" {
            format!("<{RDF_TYPE}>")
        } else {
            expand(predicate_token, prefixes)?
        };
        index += 1;
        let object_token = tokens.get(index).ok_or_else(|| {
            RuntimeError::Mapping(
                "原生 mapping target 必须恰好包含一个三元组或有效的多三元组序列".into(),
            )
        })?;
        if matches!(object_token.as_str(), ";" | ".") {
            return Err(RuntimeError::Mapping(
                "原生 mapping target 必须恰好包含一个三元组或有效的多三元组序列".into(),
            ));
        }
        triples.push((
            subject.clone().expect("subject is assigned above"),
            predicate.clone(),
            native_object(object_token, prefixes)?,
        ));
        index += 1;
        while tokens.get(index).map(String::as_str) == Some(",") {
            index += 1;
            let object_token = tokens.get(index).ok_or_else(|| {
                RuntimeError::Mapping("原生 mapping target 的 , 后缺少 object".into())
            })?;
            if matches!(object_token.as_str(), ";" | "." | ",") {
                return Err(RuntimeError::Mapping(
                    "原生 mapping target 的 , 后缺少 object".into(),
                ));
            }
            triples.push((
                subject.clone().expect("subject is assigned above"),
                predicate.clone(),
                native_object(object_token, prefixes)?,
            ));
            index += 1;
        }
        match tokens.get(index).map(String::as_str) {
            None => break,
            Some(";") => {
                index += 1;
                if index == tokens.len() {
                    return Err(RuntimeError::Mapping(
                        "原生 mapping target 的 ; 后缺少 predicate".into(),
                    ));
                }
            }
            Some(".") => {
                index += 1;
                need_subject = true;
            }
            Some(_) => {
                return Err(RuntimeError::Mapping(
                    "原生 mapping target 的三元组之间缺少 ; 或 .".into(),
                ))
            }
        }
    }
    if triples.is_empty() || !need_subject && tokens.last().is_some_and(|token| token == ";") {
        return Err(RuntimeError::Mapping(
            "原生 mapping target 必须恰好包含一个三元组或有效的多三元组序列".into(),
        ));
    }
    Ok(triples)
}

fn native_subject(
    token: &str,
    prefixes: &BTreeMap<String, String>,
) -> Result<(String, BindingTerm), RuntimeError> {
    if let Some(template) = native_bnode_template(token) {
        return Ok((template.into(), BindingTerm::BlankNode));
    }
    if token.starts_with("_:") {
        return Ok((
            token.trim_start_matches("_:").into(),
            BindingTerm::BlankNode,
        ));
    }
    Ok((expand(token, prefixes)?, BindingTerm::Iri))
}

fn native_object(
    token: &str,
    prefixes: &BTreeMap<String, String>,
) -> Result<(String, BindingTerm), RuntimeError> {
    if let Some(template) = native_bnode_template(token) {
        return Ok((template.into(), BindingTerm::BlankNode));
    }
    if token.starts_with('<')
        || (!token.starts_with('{') && token.contains(':') && !token.starts_with('"'))
    {
        return Ok((expand(token, prefixes)?, BindingTerm::Iri));
    }
    if token.starts_with("_:") {
        return Ok((
            token.trim_start_matches("_:").into(),
            BindingTerm::BlankNode,
        ));
    }
    let (value, suffix) = if token.starts_with('"') {
        let end = closing_quote(token)
            .ok_or_else(|| RuntimeError::Mapping("原生 mapping target 的 literal 未闭合".into()))?;
        (token[1..end].into(), token[end + 1..].to_owned())
    } else if let Some((value, suffix)) = token.split_once("^^") {
        (value.into(), format!("^^{suffix}"))
    } else if let Some((value, suffix)) = token.rsplit_once('@') {
        (value.into(), format!("@{suffix}"))
    } else {
        (token.into(), String::new())
    };
    let (datatype, language) = if let Some(datatype) = suffix.strip_prefix("^^") {
        (
            Some(expand(datatype, prefixes)?.trim_matches(['<', '>']).into()),
            None,
        )
    } else if let Some(language) = suffix.strip_prefix('@') {
        if !valid_language_tag(language) {
            return Err(RuntimeError::Mapping(format!(
                "无效 literal language：{language}"
            )));
        }
        (None, Some(language.into()))
    } else if suffix.is_empty() {
        (None, None)
    } else {
        return Err(RuntimeError::Mapping(
            "无效原生 mapping literal 后缀".into(),
        ));
    };
    Ok((
        value,
        BindingTerm::Literal {
            datatype,
            language,
            // 带引号的原生 target 是 simple literal；bare column 保持 PostgreSQL
            // datatype 推断，以兼容 books 等 Ontop mapping 的数值对象。
            infer_datatype: !token.starts_with('"'),
        },
    ))
}

fn native_bnode_template(token: &str) -> Option<&str> {
    token
        .strip_prefix("BNODE(")
        .and_then(|value| value.strip_suffix(')'))
}

fn closing_quote(token: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in token.char_indices().skip(1) {
        if character == '"' && !escaped {
            return Some(index);
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    None
}

fn native_target_tokens(target: &str) -> Result<Vec<String>, RuntimeError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_iri = false;
    let mut in_literal = false;
    let mut escaped = false;
    let mut bnode_depth = 0;
    let mut characters = target.chars().peekable();
    while let Some(character) = characters.next() {
        if in_literal {
            current.push(character);
            if character == '"' && !escaped {
                in_literal = false;
            }
            escaped = character == '\\' && !escaped;
            if character != '\\' {
                escaped = false;
            }
            continue;
        }
        match character {
            '<' => {
                in_iri = true;
                current.push(character);
            }
            '>' => {
                in_iri = false;
                current.push(character);
            }
            '"' if !in_iri => {
                in_literal = true;
                current.push(character);
            }
            '(' if !in_iri && current.eq_ignore_ascii_case("BNODE") => {
                bnode_depth = 1;
                current.push(character);
            }
            ')' if !in_iri && bnode_depth > 0 => {
                bnode_depth -= 1;
                current.push(character);
            }
            ';' | '.' | ',' if !in_iri && bnode_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(character.into());
            }
            '{' if !in_iri
                && current.is_empty()
                && characters.peek().is_some_and(|next| next.is_whitespace()) =>
            {
                tokens.push(character.into())
            }
            '}' if !in_iri && current.is_empty() => tokens.push(character.into()),
            character if character.is_whitespace() && !in_iri && bnode_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if in_iri || in_literal {
        return Err(RuntimeError::Mapping(
            "原生 mapping target 含未闭合 IRI 或 literal".into(),
        ));
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}
