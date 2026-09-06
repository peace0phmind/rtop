use rtop::{DataSource, DataValue, KnowledgeGraphSpec, RdfTerm, RuntimeError, VkgRuntime};
use std::io::Cursor;

struct FakeSource {
    sql: String,
}

struct NullSource;
impl DataSource for NullSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![vec![None]])
    }
}

struct PairSource;
impl DataSource for PairSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![vec![Some("7".into()), Some("7".into())]])
    }
}

struct D005Source;
impl DataSource for D005Source {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"fname\" AS text) || $2 || CAST(\"lname\" AS text)"));
        assert!(sql.contains("CAST(\"fname\" AS text)"));
        assert_eq!(parameters, ["", "_", ""]);
        Ok(vec![vec![Some("BobSmith".into()), Some("Bob".into())]])
    }
}

struct D008GraphTemplateSource;
impl DataSource for D008GraphTemplateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"ID\" AS text)"));
        assert!(sql.contains("CAST(\"Name\" AS text)"));
        assert!(sql.contains("WHERE "));
        assert!(sql.contains("= $7"));
        assert_eq!(
            parameters,
            [
                "http://example.com/Student/",
                "/",
                "",
                "http://example.com/graph/Student/",
                "/",
                "",
                "http://example.com/graph/Student/10/Venus%20Williams",
            ]
        );
        Ok(vec![vec![
            Some("http://example.com/Student/10/Venus%20Williams".into()),
            Some("Venus Williams".into()),
        ]])
    }
}

struct D008RefObjectSource;
impl DataSource for D008RefObjectSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CROSS JOIN (SELECT * FROM \"Student\") AS parent"));
        assert!(sql.contains("parent.\"Sport\" AS __rtop_parent_subject"));
        assert_eq!(
            parameters,
            [
                "http://example.com/Student/",
                "/",
                "",
                "http://example.com/",
                ""
            ]
        );
        Ok(vec![vec![
            Some("http://example.com/Student/10/Venus%20Williams".into()),
            Some("http://example.com/Tennis".into()),
        ]])
    }
}

struct D008MultiplePredicateSource;
impl DataSource for D008MultiplePredicateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"ID\" AS text)"));
        assert!(sql.contains("CAST(\"Name\" AS text)"));
        assert_eq!(parameters, ["http://example.com/Student/", "/", ""]);
        Ok(vec![vec![
            Some("http://example.com/Student/10/Venus%20Williams".into()),
            Some("Venus Williams".into()),
        ]])
    }
}

struct D009NamedSqlColumnSource;
impl DataSource for D009NamedSqlColumnSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("COUNT(\"Sport\") as SPORTCOUNT"));
        assert_eq!(parameters, ["http://example.com/resource/student_", ""]);
        let projection = sql.split(" FROM ").next().unwrap_or(sql);
        let value = if projection.contains("SPORTCOUNT") {
            "1"
        } else {
            assert!(projection.contains("\"Name\""));
            "Venus Williams"
        };
        Ok(vec![vec![
            Some("http://example.com/resource/student_Venus%20Williams".into()),
            Some(value.into()),
        ]])
    }
}

struct D009PredicateGraphSource;
impl DataSource for D009PredicateGraphSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("JOIN (SELECT * FROM \"Sport\") AS parent"));
        assert!(sql.contains("child.\"Sport\" = parent.\"ID\""));
        assert_eq!(
            parameters,
            [
                "http://example.com/resource/student_",
                "",
                "http://example.com/resource/sport_",
                "",
            ]
        );
        Ok(vec![vec![
            Some("http://example.com/resource/student_10".into()),
            Some("http://example.com/resource/sport_100".into()),
        ]])
    }
}

struct D010IriTemplateSource;
impl DataSource for D010IriTemplateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"Country Code\" AS text)"));
        assert!(sql.contains("CAST(\"Name\" AS text)"));
        for escape in ["%25", "%20", "%2C", "%28", "%29"] {
            assert!(sql.contains(escape), "missing IRI escape {escape}: {sql}");
        }
        assert_eq!(parameters, ["http://example.com/", "/", ""]);
        Ok(vec![vec![
            Some("http://example.com/1/Bolivia%2C%20Plurinational%20State%20of".into()),
            Some("Bolivia, Plurinational State of".into()),
        ]])
    }
}

struct D010EscapedLiteralTemplateSource;
impl DataSource for D010EscapedLiteralTemplateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"ISO 3166\" AS text)"));
        assert_eq!(parameters, ["http://example.com/", "/", "", "{{{ ", " }}}"]);
        Ok(vec![vec![
            Some("http://example.com/1/Bolivia%2C%20Plurinational%20State%20of".into()),
            Some("{{{ BO }}}".into()),
        ]])
    }
}

struct D011LinkMapSource;
impl DataSource for D011LinkMapSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("FROM (SELECT * FROM \"Student_Sport\") AS rtop_mapping"));
        assert!(sql.contains("CAST(\"ID_Student\" AS text)"));
        assert!(sql.contains("CAST(\"ID_Sport\" AS text)"));
        assert_eq!(
            parameters,
            [
                "http://example.com/student/",
                "",
                "http://example.com/sport/",
                "",
            ]
        );
        Ok(vec![vec![
            Some("http://example.com/student/10".into()),
            Some("http://example.com/sport/110".into()),
        ]])
    }
}

struct D011SqlViewSource;
impl DataSource for D011SqlViewSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("FROM \"Student\",\"Sport\",\"Student_Sport\""));
        assert!(sql.contains("\"Student\".\"ID\" = \"Student_Sport\".\"ID_Student\""));
        assert_eq!(
            parameters,
            [
                "http://example.com/",
                "/",
                ";",
                "",
                "http://example.com/",
                "/",
                "",
            ]
        );
        Ok(vec![
            vec![
                Some("http://example.com/10/Venus;Williams".into()),
                Some("http://example.com/110/Tennis".into()),
            ],
            vec![
                Some("http://example.com/11/Fernando;Alonso".into()),
                Some("http://example.com/112/Formula1".into()),
            ],
        ])
    }
}

struct D012CrossMapBlankNodeSource;
impl DataSource for D012CrossMapBlankNodeSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        if sql.contains("FROM (SELECT * FROM \"IOUs\")") {
            assert_eq!(parameters, ["", "_", "", "", " ", ""]);
            Ok(vec![vec![
                Some("BobSmith".into()),
                Some("Bob Smith".into()),
            ]])
        } else {
            assert!(sql.contains("FROM (SELECT * FROM \"Lives\")"));
            assert_eq!(parameters, ["", "_", ""]);
            Ok(vec![vec![Some("BobSmith".into()), Some("Dublin".into())]])
        }
    }
}

struct D013NullTemplateSource;
impl DataSource for D013NullTemplateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"DateOfBirth\" AS text)"));
        assert_eq!(parameters, ["http://example.com/Person/", "/", "/", ""]);
        Ok(vec![
            vec![
                Some("http://example.com/Person/2/Bob/September%2C%202010".into()),
                Some("September, 2010".into()),
            ],
            vec![None, None],
        ])
    }
}

struct D019IriColumnSource;
impl DataSource for D019IriColumnSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("WHERE \"ID\" < 30"));
        assert!(sql.contains("CAST(\"FirstName\" AS text)"));
        assert_eq!(parameters, ["", ""]);
        Ok(vec![vec![Some("Carlos".into()), Some("Carlos".into())]])
    }
}

struct D020IriTemplateSource;
impl DataSource for D020IriTemplateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        for escape in ["%3A", "%2F", "%20"] {
            assert!(sql.contains(escape), "missing IRI escape {escape}: {sql}");
        }
        assert_eq!(parameters, ["", ""]);
        Ok(vec![vec![Some("Emily%20Smith".into())]])
    }
}

struct D020InvalidIriColumnSource;
impl DataSource for D020InvalidIriColumnSource {
    fn execute(
        &mut self,
        _: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert_eq!(parameters, ["", ""]);
        Ok(vec![vec![Some("Emily Smith".into())]])
    }
}

struct D026NestedJoinSource;
impl DataSource for D026NestedJoinSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("JOIN (SELECT * FROM \"SportType\") AS parent"));
        assert!(sql.contains("child.\"SType\" = parent.\"ID1\""));
        assert_eq!(
            parameters,
            [
                "http://example.com/resource/sport_",
                "",
                "http://example.com/resource/sporttype_",
                "",
            ]
        );
        Ok(vec![vec![
            Some("http://example.com/resource/sport_100".into()),
            Some("http://example.com/resource/sporttype_1".into()),
        ]])
    }
}

struct MultipleNativeMappingsSource;
impl DataSource for MultipleNativeMappingsSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert_eq!(parameters, ["https://example.test/person/", ""]);
        let id = if sql.contains("FROM students") {
            "1"
        } else {
            assert!(sql.contains("FROM teachers"));
            "2"
        };
        Ok(vec![vec![Some(format!(
            "https://example.test/person/{id}"
        ))]])
    }
}

struct NativeMultiTargetSource;
impl DataSource for NativeMultiTargetSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![vec![
            Some("http://te.st/ValuesNodeTest#teacher/7".into()),
            Some("http://te.st/ValuesNodeTest#course/7".into()),
        ]])
    }
}

struct NativeNamedGraphSource;
impl DataSource for NativeNamedGraphSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![vec![
            Some("http://example.test/person/7".into()),
            Some("Ada".into()),
        ]])
    }
}

struct NativeLiteralSource;
impl DataSource for NativeLiteralSource {
    fn execute(
        &mut self,
        sql: &str,
        _: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        let value = if sql
            .split(" FROM ")
            .next()
            .unwrap_or(sql)
            .contains("\"name\"")
        {
            "Ada"
        } else {
            assert!(sql
                .split(" FROM ")
                .next()
                .unwrap_or(sql)
                .contains("\"score\""));
            "42"
        };
        Ok(vec![vec![
            Some("http://example.test/person/7".into()),
            Some(value.into()),
        ]])
    }
}

struct NativeBnodeSource;
impl DataSource for NativeBnodeSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![vec![Some("10, 1, 2, professor, 3".into())]])
    }
}

struct ProfessorSource;
impl DataSource for ProfessorSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        if parameters.len() == 3 {
            assert!(sql.contains("CAST($3 AS text)"));
            assert_eq!(
                parameters,
                ["http://example.org/professor/", "", "Professore"]
            );
            Ok(vec![vec![Some("42".into()), Some("Professore".into())]])
        } else {
            Ok(vec![vec![Some("42".into())]])
        }
    }
}

struct CountrySource;
impl DataSource for CountrySource {
    fn execute(
        &mut self,
        sql: &str,
        _: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        if sql.contains("'EN'") {
            Ok(vec![vec![
                Some("BO".into()),
                Some("Bolivia, Plurinational State of".into()),
            ]])
        } else if sql.contains("'ES'") {
            Ok(vec![vec![
                Some("BO".into()),
                Some("Estado Plurinacional de Bolivia".into()),
            ]])
        } else {
            panic!("unexpected D015 query: {sql}")
        }
    }
}

struct EmployeeSource;
impl DataSource for EmployeeSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(job AS text)"));
        assert_eq!(
            parameters,
            [
                "http://example.com/emp/",
                "",
                "http://example.com/emp/job/",
                ""
            ]
        );
        Ok(vec![vec![
            Some("http://example.com/emp/7369".into()),
            Some("http://example.com/emp/job/CLERK".into()),
        ]])
    }
}

struct RefObjectSource;
impl DataSource for RefObjectSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains(
            "JOIN (SELECT ('Department' || deptno) AS deptId, deptno FROM DEPT) AS parent"
        ));
        assert!(sql.contains("child.deptno = parent.deptno"));
        assert_eq!(parameters, ["http://example.com/emp/", "", "", ""]);
        Ok(vec![vec![
            Some("http://example.com/emp/7369".into()),
            Some("Department10".into()),
        ]])
    }
}

struct CompositeRefObjectSource;
impl DataSource for CompositeRefObjectSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(
            sql.contains("child.tenant = parent.tenant AND child.code = parent.code"),
            "{sql}"
        );
        assert!(
            sql.contains(
                "parent.tenant AS __rtop_parent_subject_0, parent.id AS __rtop_parent_subject_1"
            ),
            "{sql}"
        );
        assert_eq!(
            parameters,
            [
                "http://example.test/child/",
                "",
                "http://example.test/parent/",
                "/",
                ""
            ]
        );
        Ok(vec![vec![
            Some("http://example.test/child/9".into()),
            Some("http://example.test/parent/tenant-a/7".into()),
        ]])
    }
}

struct FactsFileSource;
impl DataSource for FactsFileSource {
    fn execute(
        &mut self,
        sql: &str,
        _: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        if sql.starts_with("SELECT $1 ||") && !sql.contains(", CAST(") {
            Ok(vec![
                vec![
                    Some("https://example.test/company/1".into()),
                    Some("Big Company".into()),
                ],
                vec![
                    Some("https://example.test/company/2".into()),
                    Some("Some Factory".into()),
                ],
            ])
        } else {
            Ok(vec![
                vec![
                    Some("https://example.test/company/1".into()),
                    Some("Big Company".into()),
                ],
                vec![
                    Some("https://example.test/company/2".into()),
                    Some("Some Factory".into()),
                ],
            ])
        }
    }
}

struct MultiTemplateSource;
impl DataSource for MultiTemplateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"ID\" AS text)"));
        assert!(sql.contains("CAST(\"Name\" AS text)"));
        assert_eq!(parameters, ["http://example.com/", "/", ""]);
        Ok(vec![vec![
            Some("http://example.com/10/Venus".into()),
            Some("10".into()),
        ]])
    }
}

struct DirectObjectSource;
impl DataSource for DirectObjectSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(\"ID\" AS text)"));
        assert_eq!(parameters, ["http://example.com/Patient/", ""]);
        Ok(vec![vec![Some("http://example.com/Patient/1".into())]])
    }
}

struct DataIriSource;
impl DataSource for DataIriSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("upper(encode(\"Photo\", 'hex'))"));
        assert_eq!(
            parameters,
            ["http://example.com/Patient", "", "data:image/png;hex,", ""]
        );
        Ok(vec![vec![
            Some("http://example.com/Patient10".into()),
            Some("data:image/png;hex,89504E47".into()),
        ]])
    }
}

struct TypedD016Source;
impl DataSource for TypedD016Source {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        unreachable!("typed adapter path expected")
    }

    fn execute_typed(
        &mut self,
        sql: &str,
        _: &[String],
    ) -> Result<Vec<Vec<Option<DataValue>>>, RuntimeError> {
        let datatype = if sql.contains("\"BirthDate\"") {
            "http://www.w3.org/2001/XMLSchema#date"
        } else if sql.contains("\"EntranceDate\"") {
            "http://www.w3.org/2001/XMLSchema#dateTime"
        } else {
            assert!(sql.contains("\"PaidInAdvance\""));
            "http://www.w3.org/2001/XMLSchema#boolean"
        };
        let value = if datatype.ends_with("date") {
            "1981-10-10"
        } else if datatype.ends_with("dateTime") {
            "2009-10-10T12:12:22"
        } else {
            "false"
        };
        Ok(vec![vec![
            Some(DataValue {
                value: "http://example.com/Patient10".into(),
                datatype: None,
            }),
            Some(DataValue {
                value: value.into(),
                datatype: Some(datatype.into()),
            }),
        ]])
    }
}

struct NamedGraphSource;
impl DataSource for NamedGraphSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST($1 AS text)"));
        assert_eq!(parameters.len(), 1);
        Ok(vec![vec![Some(parameters[0].clone())]])
    }
}

struct LiteralTemplateSource;
impl DataSource for LiteralTemplateSource {
    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        assert!(sql.contains("CAST(first AS text) || $4 || CAST(last AS text)"));
        assert_eq!(parameters, ["http://example.com/person/", "", "", " ", ""]);
        Ok(vec![vec![
            Some("http://example.com/person/1".into()),
            Some("Ada Lovelace".into()),
        ]])
    }
}
impl DataSource for FakeSource {
    fn execute(
        &mut self,
        sql: &str,
        _: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        self.sql = sql.into();
        Ok(vec![vec![Some("7".into()), Some("7".into())]])
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
        xml_catalog_file: None,
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
fn omits_mapping_triples_with_sql_null_values() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.com/person/{id}> <https://example.com/type> <https://example.com/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    assert_eq!(format!("{:?}", runtime.query("SELECT ?person { ?person <https://example.com/type> <https://example.com/Person> . }").unwrap()), "Bindings([])");
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
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, PairSource).unwrap();
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
        xml_catalog_file: None,
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
fn evaluates_bind_replace_without_a_graph_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query("SELECT ?v WHERE { BIND(REPLACE('ABC AA', 'A', 'Z') AS ?v) }")
        .unwrap();
    assert!(matches!(result, rtop::QueryResult::Bindings(rows)
    if rows == vec![std::collections::BTreeMap::from([(
        "v".into(),
        RdfTerm::Literal {
            value: "ZBC ZZ".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
    )])]));
}

#[test]
fn evaluates_bind_string_functions_after_matching_and_preserves_replace_language() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "<https://example.test/a> <https://example.test/title> \"The Second Book\"@en .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?replacement ?summary ?contains WHERE { ?x <https://example.test/title> ?title . BIND(REPLACE(?title, \"Second\", \"First\") AS ?replacement) BIND(CONCAT(UCASE(?title), \" / \", STRLEN(?title)) AS ?summary) BIND(CONTAINS(?title, \"Second\") AS ?contains) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows.len() == 1
        && rows[0].get("replacement") == Some(&RdfTerm::Literal { value: "The First Book".into(), datatype: None, language: Some("en".into()) })
        && rows[0].get("summary") == Some(&RdfTerm::Literal { value: "THE SECOND BOOK / 15".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None })
        && rows[0].get("contains") == Some(&RdfTerm::Literal { value: "true".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }))
    );
}

#[test]
fn evaluates_bind_numeric_functions_as_decimal_literals() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?abs ?ceil ?floor ?round WHERE { BIND(ABS(-1.5) AS ?abs) BIND(CEIL(0.2) AS ?ceil) BIND(FLOOR(0.8) AS ?floor) BIND(ROUND(1.6) AS ?round) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("abs".into(), RdfTerm::Literal { value: "1.5".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
            ("ceil".into(), RdfTerm::Literal { value: "1".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
            ("floor".into(), RdfTerm::Literal { value: "0".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
            ("round".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
        ])])
    );
}

#[test]
fn evaluates_nested_arithmetic_before_numeric_bind_functions() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?abs ?half WHERE { BIND(ABS((10 - 0.15 * 10) - 10) AS ?abs) BIND(10 / 2 AS ?half) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("abs".into(), RdfTerm::Literal { value: "1.5".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
            ("half".into(), RdfTerm::Literal { value: "5".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
        ])])
    );
}

#[test]
fn evaluates_bind_datetime_extractors_with_their_sparql_datatypes() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?year ?month ?day ?hours ?minutes ?seconds WHERE { BIND(YEAR(\"2014-06-05T18:47:52\") AS ?year) BIND(MONTH(\"2014-06-05T18:47:52\") AS ?month) BIND(DAY(\"2014-06-05T18:47:52\") AS ?day) BIND(HOURS(\"2014-06-05T18:47:52\") AS ?hours) BIND(MINUTES(\"2014-06-05T18:47:52\") AS ?minutes) BIND(SECONDS(\"2014-06-05T18:47:52\") AS ?seconds) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("year".into(), RdfTerm::Literal { value: "2014".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }),
            ("month".into(), RdfTerm::Literal { value: "6".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }),
            ("day".into(), RdfTerm::Literal { value: "5".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }),
            ("hours".into(), RdfTerm::Literal { value: "18".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }),
            ("minutes".into(), RdfTerm::Literal { value: "47".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }),
            ("seconds".into(), RdfTerm::Literal { value: "52".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
        ])])
    );
}

#[test]
fn evaluates_bind_sha256_as_a_lowercase_xsd_string() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query("SELECT ?hash WHERE { BIND(SHA256(STR(\"The Semantic Web\")) AS ?hash) }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([(
            "hash".into(),
        RdfTerm::Literal { value: "5534117f459c7ead3015f0f6adf2c8a7c9f577be686cd8e3a13f37176678c272".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }
        )])])
    );
}

#[test]
fn evaluates_str_on_iri_and_literal_as_xsd_string() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query("SELECT ?iri ?literal WHERE { VALUES ?input { \"hello\"@en } BIND(STR(<https://example.test/resource>) AS ?iri) BIND(STR(?input) AS ?literal) }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("iri".into(), RdfTerm::Literal { value: "https://example.test/resource".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }),
            ("literal".into(), RdfTerm::Literal { value: "hello".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }),
        ])])
    );
}

#[test]
fn filters_sparql_regex_with_case_insensitive_flags_after_bgp() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/title> \"Semantic Web\" . <https://example.test/b> <https://example.test/title> \"Database Systems\" .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?title WHERE { ?x <https://example.test/title> ?title . FILTER(REGEX(?title, \"semantic\", \"i\")) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([(
            "title".into(), RdfTerm::Literal { value: "Semantic Web".into(), datatype: None, language: None }
        )])])
    );
}

#[test]
fn evaluates_ontop_ofn_datetime_difference_functions_on_typed_literals() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("PREFIX ofn: <http://www.ontotext.com/sparql/functions/>\nSELECT ?days ?weeks ?hours ?minutes ?seconds ?millis WHERE { BIND(\"1932-02-22T09:30:00\"^^<http://www.w3.org/2001/XMLSchema#dateTime> AS ?start) BIND(\"1999-12-14T09:00:00\"^^<http://www.w3.org/2001/XMLSchema#dateTime> AS ?end) BIND(ofn:daysBetween(?start, ?end) AS ?days) BIND(ofn:weeksBetween(?start, ?end) AS ?weeks) BIND(ofn:hoursBetween(?start, ?end) AS ?hours) BIND(ofn:minutesBetween(?start, ?end) AS ?minutes) BIND(ofn:secondsBetween(?start, ?end) AS ?seconds) BIND(ofn:millisBetween(?end, ?end) AS ?millis) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("days".into(), RdfTerm::Literal { value: "24766".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#long".into()), language: None }),
            ("weeks".into(), RdfTerm::Literal { value: "3538".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#long".into()), language: None }),
            ("hours".into(), RdfTerm::Literal { value: "594407".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#long".into()), language: None }),
            ("minutes".into(), RdfTerm::Literal { value: "35664450".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#long".into()), language: None }),
            ("seconds".into(), RdfTerm::Literal { value: "2139867000".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#long".into()), language: None }),
            ("millis".into(), RdfTerm::Literal { value: "0".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#long".into()), language: None }),
        ])])
    );
}

#[test]
fn evaluates_bind_now_as_an_xsd_datetime() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query("SELECT ?now WHERE { BIND(NOW() AS ?now) }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if matches!(rows.as_slice(), [row] if matches!(row.get("now"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if datatype == "http://www.w3.org/2001/XMLSchema#dateTime" && chrono::DateTime::parse_from_rfc3339(value).is_ok())))
    );
}

#[test]
fn evaluates_select_projection_uuid_struuid_and_rand_expressions() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query("SELECT (UUID() AS ?uuid) (STRUUID() AS ?struuid) (RAND() AS ?rand) WHERE { }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if matches!(rows.as_slice(), [row]
        if matches!(row.get("uuid"), Some(RdfTerm::Iri(value)) if value.starts_with("urn:uuid:"))
        && matches!(row.get("struuid"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if uuid::Uuid::parse_str(value).is_ok() && datatype == "http://www.w3.org/2001/XMLSchema#string")
        && matches!(row.get("rand"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value.parse::<f64>().is_ok_and(|value| (0.0..1.0).contains(&value)) && datatype == "http://www.w3.org/2001/XMLSchema#decimal")))
    );
}

#[test]
fn evaluates_single_variable_values_and_binds_each_value() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?value ?string WHERE { VALUES ?value { 2 \"aa\" } BIND(STR(?value) AS ?string) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if *rows == vec![
            std::collections::BTreeMap::from([("string".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }), ("value".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None })]),
            std::collections::BTreeMap::from([("string".into(), RdfTerm::Literal { value: "aa".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }), ("value".into(), RdfTerm::Literal { value: "aa".into(), datatype: None, language: None })]),
        ])
    );
}

#[test]
fn preserves_left_rows_for_optional_patterns_and_merges_matches() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/name> \"Ada\" . <https://example.test/a> <https://example.test/age> 36 . <https://example.test/b> <https://example.test/name> \"Bob\" .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?person ?name ?age WHERE { ?person <https://example.test/name> ?name . OPTIONAL { ?person <https://example.test/age> ?age . } }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("age".into(), RdfTerm::Literal { value: "36".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }), ("name".into(), RdfTerm::Literal { value: "Ada".into(), datatype: None, language: None }), ("person".into(), RdfTerm::Iri("https://example.test/a".into()))]),
            std::collections::BTreeMap::from([("name".into(), RdfTerm::Literal { value: "Bob".into(), datatype: None, language: None }), ("person".into(), RdfTerm::Iri("https://example.test/b".into()))]),
        ])
    );
}

#[test]
fn unions_values_branches_before_following_bind() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?value ?string WHERE { { VALUES ?value { 2 } } UNION { VALUES ?value { \"aa\" } } BIND(STR(?value) AS ?string) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("string".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }), ("value".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None })]),
            std::collections::BTreeMap::from([("string".into(), RdfTerm::Literal { value: "aa".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }), ("value".into(), RdfTerm::Literal { value: "aa".into(), datatype: None, language: None })]),
        ])
    );
}

#[test]
fn unions_bgp_branches_and_applies_select_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/name> \"Ada\" . <https://example.test/a> <https://example.test/ssn> \"one\" . <https://example.test/b> <https://example.test/name> \"Bob\" .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("PREFIX ex: <https://example.test/>\nSELECT DISTINCT ?person WHERE {\n  { ?person ex:name ?value }\n  UNION\n  { ?person ex:ssn ?value }\n}").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("person".into(), RdfTerm::Iri("https://example.test/a".into()))]),
            std::collections::BTreeMap::from([("person".into(), RdfTerm::Iri("https://example.test/b".into()))]),
        ])
    );
}

#[test]
fn projects_and_deduplicates_a_whole_where_subquery() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?value WHERE { { SELECT DISTINCT ?value WHERE { VALUES ?value { 2 2 \"aa\" } } } }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("value".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None })]),
            std::collections::BTreeMap::from([("value".into(), RdfTerm::Literal { value: "aa".into(), datatype: None, language: None })]),
        ])
    );
}

#[test]
fn applies_limit_after_subquery_projection_and_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?value WHERE { { SELECT DISTINCT ?value WHERE { VALUES ?value { 2 2 \"aa\" } } LIMIT 1 } }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([("value".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None })])])
    );
}

#[test]
fn keeps_rows_when_bind_substr_errors_after_nested_subquery_union() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query(
            r#"
        SELECT ?b ?v WHERE {
          {
            SELECT DISTINCT ?b {
              { SELECT * { VALUES ?b { 2 } } }
              UNION
              { SELECT * { VALUES ?b { "aa" "aa" } } }
            }
          }
          BIND(SUBSTR("yyy", ?b) AS ?v)
        }
    "#,
        )
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("b".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }), ("v".into(), RdfTerm::Literal { value: "yy".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None })]),
            std::collections::BTreeMap::from([("b".into(), RdfTerm::Literal { value: "aa".into(), datatype: None, language: None })]),
        ])
    );
}

#[test]
fn evaluates_logical_bind_expressions_with_sparql_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query(r#"
        SELECT ?title ?and ?or WHERE {
          VALUES ?title { "Semantic Web" "Semantic Book" "Other" }
          BIND((CONTAINS(?title, "Semantic") && CONTAINS(?title, "Web")) AS ?and)
          BIND(CONTAINS(?title, "Semantic") && CONTAINS(?title, "Book") || CONTAINS(?title, "Web") AS ?or)
        }
    "#).unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("and".into(), RdfTerm::Literal { value: "true".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }), ("or".into(), RdfTerm::Literal { value: "true".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }), ("title".into(), RdfTerm::Literal { value: "Semantic Web".into(), datatype: None, language: None })]),
            std::collections::BTreeMap::from([("and".into(), RdfTerm::Literal { value: "false".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }), ("or".into(), RdfTerm::Literal { value: "true".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }), ("title".into(), RdfTerm::Literal { value: "Semantic Book".into(), datatype: None, language: None })]),
            std::collections::BTreeMap::from([("and".into(), RdfTerm::Literal { value: "false".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }), ("or".into(), RdfTerm::Literal { value: "false".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }), ("title".into(), RdfTerm::Literal { value: "Other".into(), datatype: None, language: None })]),
        ])
    );
}

#[test]
fn evaluates_rdf_term_predicates_and_bound() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query(
            r#"
        SELECT ?value ?bound ?iri ?literal ?numeric WHERE {
          VALUES ?value { <https://example.test/a> "text" 2 }
          BIND(BOUND(?missing) AS ?bound)
          BIND(isIRI(?value) AS ?iri)
          BIND(isLiteral(?value) AS ?literal)
          BIND(isNumeric(?value) AS ?numeric)
        }
    "#,
        )
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("expected bindings")
    };
    assert_eq!(rows.len(), 3);
    assert!(matches!(rows[0].get("iri"), Some(RdfTerm::Literal { value, .. }) if value == "true"));
    assert!(
        matches!(rows[0].get("literal"), Some(RdfTerm::Literal { value, .. }) if value == "false")
    );
    assert!(
        matches!(rows[1].get("literal"), Some(RdfTerm::Literal { value, .. }) if value == "true")
    );
    assert!(
        matches!(rows[2].get("numeric"), Some(RdfTerm::Literal { value, .. }) if value == "true")
    );
    assert!(rows.iter().all(
        |row| matches!(row.get("bound"), Some(RdfTerm::Literal { value, .. }) if value == "false")
    ));
}

#[test]
fn evaluates_lang_on_language_and_simple_literals() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?language WHERE { VALUES ?title { \"title\"@en \"plain\" } BIND(LANG(?title) AS ?language) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("language".into(), RdfTerm::Literal { value: "en".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None })]),
            std::collections::BTreeMap::from([("language".into(), RdfTerm::Literal { value: "".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None })]),
        ])
    );
}

#[test]
fn evaluates_datatype_as_an_iri_and_errors_for_language_literals() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?value ?type WHERE { VALUES ?value { 2 \"plain\" \"english\"@en } BIND(DATATYPE(?value) AS ?type) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("type".into(), RdfTerm::Iri("http://www.w3.org/2001/XMLSchema#integer".into())), ("value".into(), RdfTerm::Literal { value: "2".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None })]),
            std::collections::BTreeMap::from([("type".into(), RdfTerm::Iri("http://www.w3.org/2001/XMLSchema#string".into())), ("value".into(), RdfTerm::Literal { value: "plain".into(), datatype: None, language: None })]),
            std::collections::BTreeMap::from([("value".into(), RdfTerm::Literal { value: "english".into(), datatype: None, language: Some("en".into()) })]),
        ])
    );
}

#[test]
fn evaluates_rdf_term_equality_and_inequality_in_logical_bind() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?ok WHERE { VALUES ?left { \"same\" } VALUES ?right { \"same\" } VALUES ?other { \"other\" } BIND(?left = ?right && ?left != ?other AS ?ok) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([("ok".into(), RdfTerm::Literal { value: "true".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None })])])
    )
}

#[test]
fn evaluates_same_term_with_rdf_term_identity() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?same ?different WHERE { VALUES ?left { \"value\"@en } VALUES ?right { \"value\"@en } VALUES ?other { \"other\"@en } BIND(sameTerm(?left, ?right) AS ?same) BIND(!sameTerm(?left, ?other) AS ?different) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if matches!(rows[0].get("same"), Some(RdfTerm::Literal { value, .. }) if value == "true"))
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if matches!(rows[0].get("different"), Some(RdfTerm::Literal { value, .. }) if value == "true"))
    );
}

#[test]
fn converts_absolute_strings_and_iris_with_iri_and_uri() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?iri ?uri WHERE { BIND(IRI(\"urn:john\") AS ?iri) BIND(URI(\"mailto:a@example.test\") AS ?uri) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if matches!(rows[0].get("iri"), Some(RdfTerm::Iri(value)) if value == "urn:john") && matches!(rows[0].get("uri"), Some(RdfTerm::Iri(value)) if value == "mailto:a@example.test"))
    );
}

#[test]
fn resolves_relative_iri_and_uri_against_sparql_base() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("BASE <http://example.org/project1#data/>\nSELECT ?iri ?uri WHERE { BIND(IRI(\"john\") AS ?iri) BIND(URI(\"jane\") AS ?uri) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if matches!(rows[0].get("iri"), Some(RdfTerm::Iri(value)) if value == "http://example.org/project1#data/john") && matches!(rows[0].get("uri"), Some(RdfTerm::Iri(value)) if value == "http://example.org/project1#data/jane")),
        "{result:?}"
    );
}

#[test]
fn evaluates_if_coalesce_and_numeric_relational_comparisons() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?first ?second ?fallback ?short WHERE { BIND(IF(1 < 2, \"first\", \"second\") AS ?first) BIND(IF(1 > 2, \"first\", \"second\") AS ?second) BIND(COALESCE(IF(\"rrr\" * \"2\"^^xsd:integer, \"1\", \"2\"), \"other\") AS ?fallback) BIND(COALESCE(IF(1 > 2, \"rrr\" * \"2\"^^xsd:integer, \"second\"), \"other\") AS ?short) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if matches!(rows[0].get("first"), Some(RdfTerm::Literal { value, .. }) if value == "first") && matches!(rows[0].get("second"), Some(RdfTerm::Literal { value, .. }) if value == "second") && matches!(rows[0].get("fallback"), Some(RdfTerm::Literal { value, .. }) if value == "other") && matches!(rows[0].get("short"), Some(RdfTerm::Literal { value, .. }) if value == "second"))
    );
}

#[test]
fn evaluates_distinct_grouped_aggregates() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?p (SUM(DISTINCT ?n) AS ?sum) (AVG(DISTINCT ?n) AS ?avg) (COUNT(DISTINCT ?n) AS ?count) (GROUP_CONCAT(DISTINCT ?text; SEPARATOR=\"|\") AS ?concat) WHERE { VALUES (?p ?n ?text) { (<https://example.test/one> 10 \"10\") (<https://example.test/one> 11 \"11\") (<https://example.test/one> 10 \"10\") } } GROUP BY ?p").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("p"), Some(RdfTerm::Iri(value)) if value == "https://example.test/one")
        && matches!(rows[0].get("sum"), Some(RdfTerm::Literal { value, datatype: Some(datatype), .. }) if value == "21" && datatype.ends_with("#integer"))
        && matches!(rows[0].get("avg"), Some(RdfTerm::Literal { value, datatype: Some(datatype), .. }) if value == "10.5" && datatype.ends_with("#decimal"))
        && matches!(rows[0].get("count"), Some(RdfTerm::Literal { value, datatype: Some(datatype), .. }) if value == "2" && datatype.ends_with("#integer"))
        && matches!(rows[0].get("concat"), Some(RdfTerm::Literal { value, datatype: Some(datatype), .. }) if value == "10|11" && datatype.ends_with("#string"))),
        "{result:?}"
    );
}

#[test]
fn evaluates_min_max_and_empty_numeric_aggregates() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query("SELECT (MIN(?n) AS ?min) (MAX(?n) AS ?max) WHERE { VALUES ?n { 11 10 13 } }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("min"), Some(RdfTerm::Literal { value, .. }) if value == "10")
        && matches!(rows[0].get("max"), Some(RdfTerm::Literal { value, .. }) if value == "13")),
        "{result:?}"
    );
    let empty = runtime.query("SELECT (SUM(?n) AS ?sum) (AVG(?n) AS ?avg) (COUNT(?n) AS ?count) WHERE { VALUES ?n { } }").unwrap();
    assert!(
        matches!(empty, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("sum"), Some(RdfTerm::Literal { value, .. }) if value == "0")
        && matches!(rows[0].get("avg"), Some(RdfTerm::Literal { value, .. }) if value == "0")
        && matches!(rows[0].get("count"), Some(RdfTerm::Literal { value, .. }) if value == "0")),
        "{empty:?}"
    );
}

#[test]
fn evaluates_minus_against_an_aggregate_subquery() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime
        .query(
            "SELECT ?p WHERE {
                VALUES ?p { <https://example.test/one> <https://example.test/two> <https://example.test/three> }
                MINUS {
                    SELECT ?p (SUM(?n) AS ?total) WHERE {
                        VALUES (?p ?n) { (<https://example.test/one> 10) (<https://example.test/one> 11) (<https://example.test/two> 3) }
                    } GROUP BY ?p
                }
            } ORDER BY ?p",
        )
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if *rows == vec![
            std::collections::BTreeMap::from([("p".into(), RdfTerm::Iri("https://example.test/three".into()))]),
        ]),
        "{result:?}"
    );
}

#[test]
fn generates_fresh_and_labeled_blank_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let result = runtime.query("SELECT ?fresh ?named ?same WHERE { VALUES ?v { 1 2 } BIND(BNODE() AS ?fresh) BIND(BNODE(\"b1\") AS ?named) BIND(sameTerm(?named, BNODE(\"b1\")) AS ?same) }").unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("expected bindings");
    };
    assert_eq!(rows.len(), 2);
    assert!(matches!(rows[0].get("fresh"), Some(RdfTerm::BlankNode(_))));
    assert!(matches!(rows[1].get("fresh"), Some(RdfTerm::BlankNode(_))));
    assert_ne!(rows[0].get("fresh"), rows[1].get("fresh"));
    assert!(rows
        .iter()
        .all(|row| matches!(row.get("named"), Some(RdfTerm::BlankNode(value)) if value == "b1")));
    assert!(rows.iter().all(
        |row| matches!(row.get("same"), Some(RdfTerm::Literal { value, .. }) if value == "true")
    ));
}

#[test]
fn expands_ontop_style_sparql_prefixes_and_rdf_type_shorthand() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "@prefix ex: <https://example.test/> . ex:factCompany a ex:Company .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("PREFIX ex: <https://example.test/>\nSELECT * { ?company a ex:Company }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn applies_select_distinct_to_duplicate_fact_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/p> <https://example.test/b> . <https://example.test/a> <https://example.test/p> <https://example.test/b> .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("SELECT DISTINCT ?subject { ?subject <https://example.test/p> <https://example.test/b> }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn evaluates_bounded_property_paths_and_correlated_exists_against_facts() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "<https://example.test/a> <https://example.test/p> <https://example.test/b>, <https://example.test/d> .
         <https://example.test/b> <https://example.test/q> <https://example.test/c> .
         <https://example.test/d> <https://example.test/q> <https://example.test/c> .
         <https://example.test/alice> <https://example.test/type> <https://example.test/Person> ; <https://example.test/name> \"Alice\" .
         <https://example.test/bob> <https://example.test/type> <https://example.test/Person> ; <https://example.test/blocked> \"yes\" .
         <https://example.test/charlie> <https://example.test/type> <https://example.test/Person> .
         <https://example.test/a> <https://example.test/value> 1 ; <https://example.test/other> 1, 2 .
         <https://example.test/b> <https://example.test/value> 3.0 ; <https://example.test/other> 4.0, 5.0 .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();

    let paths = runtime
        .query("PREFIX ex: <https://example.test/>\nSELECT ?target { ex:a ex:p/ex:q ?target }")
        .unwrap();
    assert!(
        matches!(paths, rtop::QueryResult::Bindings(rows) if rows.len() == 2 && rows.iter().all(|row| row.get("target") == Some(&RdfTerm::Iri("https://example.test/c".into()))))
    );

    let exists = runtime
        .query("PREFIX ex: <https://example.test/>\nSELECT ?person { ?person ex:type ex:Person . FILTER EXISTS { ?person ex:name ?name } FILTER NOT EXISTS { ?person ex:blocked ?blocked } }")
        .unwrap();
    assert!(
        matches!(exists, rtop::QueryResult::Bindings(rows) if rows.len() == 1
        && rows[0].get("person") == Some(&RdfTerm::Iri("https://example.test/alice".into())))
    );

    let correlated = runtime
        .query("PREFIX ex: <https://example.test/>\nSELECT ?subject ?value { ?subject ex:value ?value FILTER NOT EXISTS { ?subject ex:other ?other . FILTER(?value = ?other) } }")
        .unwrap();
    assert!(
        matches!(correlated, rtop::QueryResult::Bindings(rows) if rows.len() == 1
        && rows[0].get("subject") == Some(&RdfTerm::Iri("https://example.test/b".into()))
        && rows[0].get("value") == Some(&RdfTerm::Literal {
            value: "3.0".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
            language: None,
        }))
    );
}

#[test]
fn orders_fact_bindings_by_an_ontop_style_order_by_variable() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/name> \"Zulu\" . <https://example.test/b> <https://example.test/name> \"Alpha\" .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query("SELECT ?name { ?person <https://example.test/name> ?name } ORDER BY ?name")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"name\": Literal { value: \"Alpha\", datatype: None, language: None }}, {\"name\": Literal { value: \"Zulu\", datatype: None, language: None }}])");
}

#[test]
fn orders_fact_bindings_by_multiple_asc_and_desc_terms() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/country> \"Italy\" . <https://example.test/a> <https://example.test/number> \"2\" . <https://example.test/a> <https://example.test/street> \"Zulu\" . <https://example.test/b> <https://example.test/country> \"Italy\" . <https://example.test/b> <https://example.test/number> \"1\" . <https://example.test/b> <https://example.test/street> \"Alpha\" . <https://example.test/c> <https://example.test/country> \"Austria\" . <https://example.test/c> <https://example.test/number> \"9\" . <https://example.test/c> <https://example.test/street> \"Beta\" .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime.query("SELECT ?person ?country ?number ?street { ?person <https://example.test/country> ?country . ?person <https://example.test/number> ?number . ?person <https://example.test/street> ?street } ORDER BY DESC(?country) ?number DESC(?street)").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows.iter().map(|row| row["person"].clone()).collect::<Vec<_>>() == vec![RdfTerm::Iri("https://example.test/b".into()), RdfTerm::Iri("https://example.test/a".into()), RdfTerm::Iri("https://example.test/c".into())])
    );
}

#[test]
fn evaluates_manifest_style_equality_filters_and_semicolon_patterns() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/type> <https://example.test/Row> . <https://example.test/a> <https://example.test/boolean> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> . <https://example.test/a> <https://example.test/numeric> \"1.00\"^^<http://www.w3.org/2001/XMLSchema#decimal> . <https://example.test/a> <https://example.test/timestamp> \"2013-03-19T02:12:10Z\"^^<http://www.w3.org/2001/XMLSchema#dateTimeStamp> . <https://example.test/a> <https://example.test/date> \"2025-12-31\"^^<http://www.w3.org/2001/XMLSchema#date> .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    for query in [
        "SELECT ?x ?value { ?x <https://example.test/type> <https://example.test/Row> ; <https://example.test/boolean> ?value FILTER (?value = \"1\"^^<http://www.w3.org/2001/XMLSchema#boolean>) }",
        "SELECT ?x ?value { ?x <https://example.test/type> <https://example.test/Row> ; <https://example.test/numeric> ?value FILTER (?value = 1.0) }",
        "SELECT ?x ?value { ?x <https://example.test/type> <https://example.test/Row> ; <https://example.test/timestamp> ?value FILTER (?value = \"2013-03-19T03:12:10+01:00\"^^<http://www.w3.org/2001/XMLSchema#dateTimeStamp>) }",
        "SELECT ?x ?value { ?x <https://example.test/type> <https://example.test/Row> ; <https://example.test/date> ?value FILTER (?value <= \"2025-12-31\"^^<http://www.w3.org/2001/XMLSchema#date>) }",
        "SELECT ?x { ?x <https://example.test/type> <https://example.test/Row> . OPTIONAL { ?x <https://example.test/missing> ?end } FILTER (!BOUND(?end) || \"2025-12-31\"^^<http://www.w3.org/2001/XMLSchema#date> < ?end) }",
    ] {
        assert!(matches!(runtime.query(query), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1));
    }
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
        xml_catalog_file: None,
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
        xml_catalog_file: None,
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
        xml_catalog_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(spec, FakeSource { sql: String::new() }), Err(RuntimeError::Mapping(message)) if message.contains("SourceDeclaration"))
    );
}

#[test]
fn rejects_missing_native_obda_target_term_from_the_ontop_mistake_case() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("missing-target-term.obda");
    std::fs::write(&mapping, "[PrefixDeclaration]\n: http://example.org/voc#\n\n[MappingDeclaration] @collection [[\nmappingId mapping-unbound-target-variable\ntarget <http:/localhost/person/{ID}> :firstName .\nsource SELECT ID,FNAME FROM PERSON\n]]").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(spec, FakeSource { sql: String::new() }), Err(RuntimeError::Mapping(message)) if message.contains("恰好包含一个三元组"))
    );
}

#[test]
fn distinguishes_invalid_and_unsupported_native_mapping_source_sql() {
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("invalid-sql.obda");
    let unsupported = dir.path().join("unsupported-sql.obda");
    for (path, source, expected) in [
        (&invalid, "SELECT ID FROM PERSON !", "SQL 语法无效"),
        (&unsupported, "DELETE FROM PERSON", "SQL 不受支持"),
    ] {
        std::fs::write(path, format!("[MappingDeclaration]\ntarget <https://example.test/person/{{id}}> <https://example.test/type> <https://example.test/Person> .\nsource {source}\n")).unwrap();
        let spec = KnowledgeGraphSpec {
            mapping_file: path.clone(),
            facts_file: None,
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        };
        assert!(
            matches!(VkgRuntime::new(spec, FakeSource { sql: String::new() }), Err(RuntimeError::Mapping(message)) if message.contains(expected))
        );
    }
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
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?person { ?person <https://example.com/type> <https://example.com/Person> . }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn loads_relative_r2rml_iris_from_a_reader() {
    let spec = KnowledgeGraphSpec {
        mapping_file: std::path::PathBuf::from("reader.ttl"),
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mapping = "@prefix rr: <http://www.w3.org/ns/r2rml#> . [] a rr:TriplesMap; rr:logicalTable [ rr:tableName \"people\" ]; rr:subjectMap [ rr:template \"http://example.test/person/{id}\" ]; rr:predicateObjectMap [ rr:predicate <kind>; rr:object <Person> ] .";
    let mut runtime = VkgRuntime::new_with_r2rml_reader(
        spec,
        Cursor::new(mapping),
        "http://example.test/base/",
        FakeSource { sql: String::new() },
    )
    .unwrap();
    assert!(
        matches!(runtime.query("SELECT ?person { ?person <http://example.test/base/kind> <http://example.test/base/Person> }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn loads_ontop_r2rml_d000_table_and_column_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d000.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> .\n@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n<TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/{\\\"Name\\\"}\" ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    if let Err(error) = VkgRuntime::new(spec, FakeSource { sql: String::new() }) {
        panic!("{error}");
    }
}

#[test]
fn queries_each_predicate_object_map_from_ontop_r2rml_d002() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d002.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @prefix ex: <http://example.com/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/{\\\"ID\\\"}/{\\\"Name\\\"}\"; rr:class foaf:Person ]; rr:predicateObjectMap [ rr:predicate ex:id; rr:objectMap [ rr:column \"\\\"ID\\\"\" ] ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, PairSource).unwrap();
    for (predicate, object, projection) in [
        ("<http://example.com/id>", "?value", "?person ?value"),
        (
            "<http://xmlns.com/foaf/0.1/name>",
            "?value",
            "?person ?value",
        ),
        (
            "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>",
            "<http://xmlns.com/foaf/0.1/Person>",
            "?person",
        ),
    ] {
        assert!(
            matches!(runtime.query(&format!("SELECT {projection} {{ ?person {predicate} {object} }}")), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
        );
    }
}

#[test]
fn preserves_r2rml_blank_node_subject_term_type_from_ontop_d002() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d002b.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . [] a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:column \"\\\"ID\\\"\"; rr:termType rr:BlankNode ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, PairSource).unwrap();
    let result = runtime
        .query("SELECT ?person ?name { ?person <http://xmlns.com/foaf/0.1/name> ?name }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"name\": Literal { value: \"7\", datatype: None, language: None }, \"person\": BlankNode(\"7\")}])");
}

#[test]
fn preserves_r2rml_multi_column_blank_node_subject_from_ontop_d005() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d005.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @base <http://example.com/base/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"IOUs\\\"\" ]; rr:subjectMap [ rr:template \"{\\\"fname\\\"}_{\\\"lname\\\"}\"; rr:class <IOUs>; rr:termType rr:BlankNode ]; rr:predicateObjectMap [ rr:predicate <IOUs#fname>; rr:objectMap [ rr:column \"\\\"fname\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D005Source).unwrap();
    let result = runtime
        .query("SELECT ?iou ?fname { ?iou <http://example.com/base/IOUs#fname> ?fname }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"fname\": Literal { value: \"Bob\", datatype: None, language: None }, \"iou\": BlankNode(\"BobSmith\")}])"
    );
}

#[test]
fn queries_r2rml_template_graph_map_from_ontop_d008() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d008a.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @base <http://example.com/base/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/Student/{\\\"ID\\\"}/{\\\"Name\\\"}\"; rr:graphMap [ rr:template \"http://example.com/graph/Student/{\\\"ID\\\"}/{\\\"Name\\\"}\" ] ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D008GraphTemplateSource).unwrap();
    let result = runtime
        .query("SELECT ?student ?name { GRAPH <http://example.com/graph/Student/10/Venus%20Williams> { ?student <http://xmlns.com/foaf/0.1/name> ?name } }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"name\": Literal { value: \"Venus Williams\", datatype: None, language: None }, \"student\": Iri(\"http://example.com/Student/10/Venus%20Williams\")}])"
    );
}

#[test]
fn resolves_r2rml_ref_object_map_without_join_from_ontop_d008() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d008b.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <SportMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/{\\\"Sport\\\"}\" ]; rr:predicateObjectMap [ rr:predicate <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>; rr:object ex:activity ] . <StudentMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/Student/{\\\"ID\\\"}/{\\\"Name\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:Sport; rr:objectMap [ a rr:RefObjectMap; rr:parentTriplesMap <SportMap> ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D008RefObjectSource).unwrap();
    let result = runtime
        .query("SELECT ?student ?sport { ?student <http://example.com/Sport> ?sport }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"sport\": Iri(\"http://example.com/Tennis\"), \"student\": Iri(\"http://example.com/Student/10/Venus%20Williams\")}])"
    );
}

#[test]
fn expands_multiple_r2rml_predicates_in_one_object_map_from_ontop_d008() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d008c.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @prefix ex: <http://example.com/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/Student/{\\\"ID\\\"}/{\\\"Name\\\"}\" ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:predicateMap [ rr:constant ex:name ]; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D008MultiplePredicateSource).unwrap();
    for predicate in [
        "<http://xmlns.com/foaf/0.1/name>",
        "<http://example.com/name>",
    ] {
        let result = runtime
            .query(&format!("SELECT ?name {{ ?student {predicate} ?name }}"))
            .unwrap();
        assert_eq!(
            format!("{result:?}"),
            "Bindings([{\"name\": Literal { value: \"Venus Williams\", datatype: None, language: None }}])"
        );
    }
}

#[test]
fn resolves_named_sql_projection_column_from_ontop_d009() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d009d.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @prefix ex: <http://example.com/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery \"SELECT \\\"Name\\\", COUNT(\\\"Sport\\\") as SPORTCOUNT FROM \\\"Student\\\" GROUP BY \\\"Name\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/resource/student_{\\\"Name\\\"}\" ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ]; rr:predicateObjectMap [ rr:predicate ex:numSport; rr:objectMap [ rr:column \"SPORTCOUNT\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D009NamedSqlColumnSource).unwrap();
    let result = runtime
        .query("SELECT ?name ?count { ?student <http://xmlns.com/foaf/0.1/name> ?name . ?student <http://example.com/numSport> ?count }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"count\": Literal { value: \"1\", datatype: None, language: None }, \"name\": Literal { value: \"Venus Williams\", datatype: None, language: None }}])"
    );
}

#[test]
fn combines_subject_and_predicate_object_graphs_from_ontop_d009() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d009b.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <SportMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Sport\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/resource/sport_{\\\"ID\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:label; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] . <StudentMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/resource/student_{\\\"ID\\\"}\"; rr:graph <http://example.com/graph/students> ]; rr:predicateObjectMap [ rr:predicate ex:practises; rr:objectMap [ a rr:RefObjectMap; rr:parentTriplesMap <SportMap>; rr:joinCondition [ rr:child \"\\\"Sport\\\"\"; rr:parent \"\\\"ID\\\"\" ] ]; rr:graph <http://example.com/graph/practise> ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D009PredicateGraphSource).unwrap();
    for graph in [
        "http://example.com/graph/students",
        "http://example.com/graph/practise",
    ] {
        let result = runtime
            .query(&format!("SELECT ?sport {{ GRAPH <{graph}> {{ ?student <http://example.com/practises> ?sport }} }}"))
            .unwrap();
        assert_eq!(
            format!("{result:?}"),
            "Bindings([{\"sport\": Iri(\"http://example.com/resource/sport_100\")}])"
        );
    }
}

#[test]
fn percent_encodes_iri_template_columns_from_ontop_d010() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d010b.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Country Info\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/{\\\"Country Code\\\"}/{\\\"Name\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:name; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D010IriTemplateSource).unwrap();
    let result = runtime
        .query("SELECT ?country ?name { ?country <http://example.com/name> ?name }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"country\": Iri(\"http://example.com/1/Bolivia%2C%20Plurinational%20State%20of\"), \"name\": Literal { value: \"Bolivia, Plurinational State of\", datatype: None, language: None }}])"
    );
}

#[test]
fn preserves_escaped_braces_in_literal_template_from_ontop_d010() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d010c.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> .
<TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName "\"Country Info\"" ]; rr:subjectMap [ rr:template "http://example.com/{\"Country Code\"}/{\"Name\"}" ]; rr:predicateObjectMap [ rr:predicate ex:code; rr:objectMap [ rr:template "\\{\\{\\{ {\"ISO 3166\"} \\}\\}\\}"; rr:termType rr:Literal ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D010EscapedLiteralTemplateSource).unwrap();
    let result = runtime
        .query("SELECT ?country ?code { ?country <http://example.com/code> ?code }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"code\": Literal { value: \"{{{ BO }}}\", datatype: None, language: None }, \"country\": Iri(\"http://example.com/1/Bolivia%2C%20Plurinational%20State%20of\")}])"
    );
}

#[test]
fn maps_many_to_many_link_table_from_ontop_d011() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d011b.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <LinkMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student_Sport\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/student/{\\\"ID_Student\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:plays; rr:objectMap [ rr:template \"http://example.com/sport/{\\\"ID_Sport\\\"}\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D011LinkMapSource).unwrap();
    let result = runtime
        .query("SELECT ?student ?sport { ?student <http://example.com/plays> ?sport }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"sport\": Iri(\"http://example.com/sport/110\"), \"student\": Iri(\"http://example.com/student/10\")}])"
    );
}

#[test]
fn maps_many_to_many_sql_view_rows_from_ontop_d011() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d011a.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery \"SELECT \\\"Student\\\".\\\"ID\\\" as ID, \\\"Student\\\".\\\"FirstName\\\" as FirstName, \\\"Student\\\".\\\"LastName\\\" as LastName, \\\"Sport\\\".\\\"Description\\\" as Description, \\\"Sport\\\".\\\"ID\\\" as Sport_ID FROM \\\"Student\\\",\\\"Sport\\\",\\\"Student_Sport\\\" WHERE \\\"Student\\\".\\\"ID\\\" = \\\"Student_Sport\\\".\\\"ID_Student\\\" AND \\\"Sport\\\".\\\"ID\\\" = \\\"Student_Sport\\\".\\\"ID_Sport\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/{ID}/{FirstName};{LastName}\" ]; rr:predicateObjectMap [ rr:predicate ex:plays; rr:objectMap [ rr:template \"http://example.com/{Sport_ID}/{Description}\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D011SqlViewSource).unwrap();
    let result = runtime
        .query("SELECT ?student ?sport { ?student <http://example.com/plays> ?sport }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"sport\": Iri(\"http://example.com/110/Tennis\"), \"student\": Iri(\"http://example.com/10/Venus;Williams\")}, {\"sport\": Iri(\"http://example.com/112/Formula1\"), \"student\": Iri(\"http://example.com/11/Fernando;Alonso\")}])"
    );
}

#[test]
fn rejects_multiple_subject_maps_from_ontop_d012() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d012d.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"IOUs\\\"\" ]; rr:subjectMap [ rr:template \"{\\\"fname\\\"}_{\\\"lname\\\"}_{\\\"amount\\\"}\" ]; rr:subjectMap [ rr:template \"{\\\"amount\\\"}_{\\\"fname\\\"}_{\\\"lname\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:amount; rr:objectMap [ rr:column \"\\\"amount\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(spec, PairSource), Err(RuntimeError::Mapping(message)) if message.contains("只能有一个 rr:subjectMap"))
    );
}

#[test]
fn joins_equivalent_blank_nodes_across_triples_maps_from_ontop_d012() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d012b.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @prefix ex: <http://example.com/> . <IOUsMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"IOUs\\\"\" ]; rr:subjectMap [ rr:template \"{\\\"fname\\\"}_{\\\"lname\\\"}\"; rr:termType rr:BlankNode ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:template \"{\\\"fname\\\"} {\\\"lname\\\"}\"; rr:termType rr:Literal ] ] . <LivesMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Lives\\\"\" ]; rr:subjectMap [ rr:template \"{\\\"fname\\\"}_{\\\"lname\\\"}\"; rr:termType rr:BlankNode ]; rr:predicateObjectMap [ rr:predicate ex:city; rr:objectMap [ rr:column \"\\\"city\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D012CrossMapBlankNodeSource).unwrap();
    let result = runtime
        .query("SELECT ?name ?city { ?person <http://xmlns.com/foaf/0.1/name> ?name . ?person <http://example.com/city> ?city }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"city\": Literal { value: \"Dublin\", datatype: None, language: None }, \"name\": Literal { value: \"Bob Smith\", datatype: None, language: None }}])"
    );
}

#[test]
fn omits_triples_with_null_template_columns_from_ontop_d013() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d013a.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Person\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/Person/{\\\"ID\\\"}/{\\\"Name\\\"}/{\\\"DateOfBirth\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:BirthDay; rr:objectMap [ rr:column \"\\\"DateOfBirth\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D013NullTemplateSource).unwrap();
    let result = runtime
        .query("SELECT ?person ?birthday { ?person <http://example.com/BirthDay> ?birthday }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"birthday\": Literal { value: \"September, 2010\", datatype: None, language: None }, \"person\": Iri(\"http://example.com/Person/2/Bob/September%2C%202010\")}])"
    );
}

#[test]
fn resolves_relative_iri_column_against_r2rml_base_from_ontop_d019() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d019a.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @base <http://example.com/base/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery \"SELECT \\\"ID\\\", \\\"FirstName\\\", \\\"LastName\\\" FROM \\\"Employee\\\" WHERE \\\"ID\\\" < 30\" ]; rr:subjectMap [ rr:column \"\\\"FirstName\\\"\" ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:column \"\\\"FirstName\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D019IriColumnSource).unwrap();
    let result = runtime
        .query("SELECT ?employee ?name { ?employee <http://xmlns.com/foaf/0.1/name> ?name }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"employee\": Iri(\"http://example.com/base/Carlos\"), \"name\": Literal { value: \"Carlos\", datatype: None, language: None }}])"
    );
}

#[test]
fn percent_encodes_iri_template_component_from_ontop_d020() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d020a.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @base <http://example.com/base/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"{\\\"Name\\\"}\"; rr:termType rr:IRI ]; rr:predicateObjectMap [ rr:predicate rdf:type; rr:object foaf:Person ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D020IriTemplateSource).unwrap();
    let result = runtime
        .query("SELECT ?student { ?student <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"student\": Iri(\"http://example.com/base/Emily%20Smith\")}])"
    );
}

#[test]
fn rejects_invalid_iri_column_data_from_ontop_d020() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d020b.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @base <http://example.com/base/> . <TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:column \"\\\"Name\\\"\"; rr:termType rr:IRI ]; rr:predicateObjectMap [ rr:predicate rdf:type; rr:object foaf:Person ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D020InvalidIriColumnSource).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?student { ?student <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> }"), Err(RuntimeError::Mapping(message)) if message.contains("data error"))
    );
}

#[test]
fn resolves_second_level_ref_object_map_from_ontop_d026() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d026.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> . <SportTypeMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"SportType\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/resource/sporttype_{\\\"ID1\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:label; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] . <SportMap> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Sport\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/resource/sport_{\\\"ID2\\\"}\" ]; rr:predicateObjectMap [ rr:predicate ex:practises_type; rr:objectMap [ a rr:RefObjectMap; rr:parentTriplesMap <SportTypeMap>; rr:joinCondition [ rr:child \"\\\"SType\\\"\"; rr:parent \"\\\"ID1\\\"\" ] ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, D026NestedJoinSource).unwrap();
    let result = runtime
        .query("SELECT ?type { ?sport <http://example.com/practises_type> ?type }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"type\": Iri(\"http://example.com/resource/sporttype_1\")}])"
    );
}

#[test]
fn resolves_composite_r2rml_ref_object_map_join() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("composite-join.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.test/> . <Parent> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"parent\" ]; rr:subjectMap [ rr:template \"http://example.test/parent/{tenant}/{id}\" ]; rr:predicateObjectMap [ rr:predicate ex:label; rr:objectMap [ rr:column \"id\" ] ] . <Child> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"child\" ]; rr:subjectMap [ rr:template \"http://example.test/child/{id}\" ]; rr:predicateObjectMap [ rr:predicate ex:parent; rr:objectMap [ a rr:RefObjectMap; rr:parentTriplesMap <Parent>; rr:joinCondition [ rr:child \"tenant\"; rr:parent \"tenant\" ]; rr:joinCondition [ rr:child \"code\"; rr:parent \"code\" ] ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, CompositeRefObjectSource).unwrap();
    let result = runtime
        .query("SELECT ?child ?parent { ?child <http://example.test/parent> ?parent }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"child\": Iri(\"http://example.test/child/9\"), \"parent\": Iri(\"http://example.test/parent/tenant-a/7\")}])");
}

#[test]
fn loads_each_native_obda_mapping_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("multiple.obda");
    std::fs::write(&mapping, "[PrefixDeclaration]\nex: https://example.test/\n\n[MappingDeclaration] @collection [[\nmappingId students\ntarget ex:person/{id} ex:type ex:Student .\nsource SELECT id FROM students\nmappingId teachers\ntarget ex:person/{id} ex:type ex:Teacher .\nsource SELECT id FROM teachers\n]]").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, MultipleNativeMappingsSource).unwrap();
    for (class, person) in [
        ("Student", "https://example.test/person/1"),
        ("Teacher", "https://example.test/person/2"),
    ] {
        let result = runtime
            .query(&format!("SELECT ?person {{ ?person <https://example.test/type> <https://example.test/{class}> }}"))
            .unwrap();
        assert_eq!(
            format!("{result:?}"),
            format!("Bindings([{{\"person\": Iri(\"{person}\")}}])")
        );
    }
}

#[test]
fn expands_native_obda_semicolon_and_multiple_target_triples() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("values-node.obda");
    std::fs::write(&mapping, "[PrefixDeclaration]\n: http://te.st/ValuesNodeTest#\n\n[MappingDeclaration] @collection [[\nmappingId teacher-teaches-class\ntarget :teacher/{teacher} a :Teacher ; :teaches :course/{course} . :course/{course} a :Course .\nsource SELECT teacher, course FROM example1\n]]").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NativeMultiTargetSource).unwrap();
    for (predicate, object) in [
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "http://te.st/ValuesNodeTest#Teacher",
        ),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "http://te.st/ValuesNodeTest#Course",
        ),
    ] {
        let result = runtime
            .query(&format!(
                "SELECT ?resource {{ ?resource <{predicate}> <{object}> }}"
            ))
            .unwrap_or_else(|error| panic!("{predicate} {object}: {error:?}"));
        assert!(matches!(result, rtop::QueryResult::Bindings(rows) if rows.len() == 1));
    }
    let result = runtime
        .query("SELECT ?teacher ?course { ?teacher <http://te.st/ValuesNodeTest#teaches> ?course }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1 && format!("{:?}", rows[0].get("course")) == "Some(Iri(\"http://te.st/ValuesNodeTest#course/7\"))"),
        "{result:?}"
    );
}

#[test]
fn queries_native_obda_constant_named_graph_target() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("named-graph.obda");
    std::fs::write(&mapping, "[PrefixDeclaration]\n: http://example.test/\n\n[MappingDeclaration] @collection [[\nmappingId people\ntarget GRAPH <http://example.test/graph> { :person/{id} a :Person ; :name {name} . }\nsource SELECT id, name FROM people\n]]").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NativeNamedGraphSource).unwrap();
    let result = runtime.query("SELECT ?person { GRAPH <http://example.test/graph> { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/Person> } }").unwrap();
    assert!(matches!(result, rtop::QueryResult::Bindings(rows) if rows.len() == 1));
    assert!(matches!(
        runtime.query("SELECT ?person { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/Person> }"),
        Err(RuntimeError::NotFullyTranslatable(_))
    ));
}

#[test]
fn queries_native_obda_template_named_graph_target() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("template-named-graph.obda");
    std::fs::write(&mapping, "[PrefixDeclaration]\n: http://example.test/\n\n[MappingDeclaration] @collection [[\nmappingId people\ntarget GRAPH :graph/{id} { :person/{id} a :Person . }\nsource SELECT id FROM people\n]]").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NativeNamedGraphSource).unwrap();
    let result = runtime.query("SELECT ?person { GRAPH <http://example.test/graph/7> { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/Person> } }").unwrap();
    assert!(matches!(result, rtop::QueryResult::Bindings(rows) if rows.len() == 1));
}

#[test]
fn preserves_native_obda_literal_language_and_datatype() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("literals.obda");
    std::fs::write(&mapping, "[PrefixDeclaration]\nxsd: http://www.w3.org/2001/XMLSchema#\n\n[MappingDeclaration] @collection [[\nmappingId people\ntarget <http://example.test/person/{id}> <http://example.test/name> {\"name\"}@en . <http://example.test/person/{id}> <http://example.test/score> {\"score\"}^^xsd:positiveInteger .\nsource SELECT id, name, score FROM people\n]]").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NativeLiteralSource).unwrap();
    let name = runtime
        .query("SELECT ?person ?name { ?person <http://example.test/name> ?name }")
        .unwrap();
    assert_eq!(format!("{name:?}"), "Bindings([{\"name\": Literal { value: \"Ada\", datatype: None, language: Some(\"en\") }, \"person\": Iri(\"http://example.test/person/7\")}])");
    let score = runtime
        .query("SELECT ?person ?score { ?person <http://example.test/score> ?score }")
        .unwrap();
    assert_eq!(format!("{score:?}"), "Bindings([{\"person\": Iri(\"http://example.test/person/7\"), \"score\": Literal { value: \"42\", datatype: Some(\"http://www.w3.org/2001/XMLSchema#positiveInteger\"), language: None }}])");
}

#[test]
fn preserves_native_obda_multi_column_bnode_subject() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("bnode.obda");
    std::fs::write(&mapping, "[MappingDeclaration] @collection [[\nmappingId coauthors\ntarget BNODE({depid}, {uniid}, {publicationid}, {authortype}, {authorid}) a <http://example.test/Coauthor> .\nsource SELECT depid, uniid, publicationid, authortype, authorid FROM coauthors\n]]").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NativeBnodeSource).unwrap();
    let result = runtime.query("SELECT ?coauthor { ?coauthor <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/Coauthor> }").unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"coauthor\": BlankNode(\"10, 1, 2, professor, 3\")}])"
    );
}

#[test]
fn queries_r2rml_literal_constant_from_ontop_d014() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d014.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <https://example.test/> . [] a rr:TriplesMap; rr:logicalTable [ rr:tableName \"company\" ]; rr:subjectMap [ rr:template \"https://example.test/company/{id}\" ]; rr:predicateObjectMap [ rr:predicate ex:name; rr:objectMap [ rr:constant \"EXAMPLE Corporation\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?company { ?company <https://example.test/name> \"EXAMPLE Corporation\" }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn queries_ontop_r2rml_d000_table_and_column_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("d000.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> .\n@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n<TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName \"\\\"Student\\\"\" ]; rr:subjectMap [ rr:template \"http://example.com/{\\\"Name\\\"}\" ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:column \"\\\"Name\\\"\" ] ] .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, PairSource).unwrap();
    let result = runtime
        .query("SELECT ?person ?name { ?person <http://xmlns.com/foaf/0.1/name> ?name . }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"name\": Literal { value: \"7\", datatype: None, language: None }, \"person\": Iri(\"7\")}])");
}

#[test]
fn resolves_relative_r2rml_predicates_against_the_mapping_file_iri() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("relative.ttl");
    std::fs::write(&mapping, "@prefix rr: <http://www.w3.org/ns/r2rml#> .\n[] a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery \"SELECT id FROM people\" ]; rr:subjectMap [ rr:template \"https://example.test/person/{id}\" ]; rr:predicateObjectMap [ rr:predicate <kind>; rr:objectMap [ rr:constant <https://example.test/Person> ] ] .").unwrap();
    let predicate = format!("<file://{}/kind>", dir.path().display());
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query(&format!("SELECT ?person {{ ?person {predicate} <https://example.test/Person> }}")), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn distinguishes_r2rml_rdf_syntax_and_structure_errors_from_the_ontop_mistake_cases() {
    let dir = tempfile::tempdir().unwrap();
    let malformed = dir.path().join("malformed.ttl");
    let missing_object_map = dir.path().join("missing-object-map.ttl");
    std::fs::write(
        &malformed,
        "@prefix rr: <http://www.w3.org/ns/r2rml#> . [] rr:logicalTable [",
    )
    .unwrap();
    std::fs::write(&missing_object_map, "@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <https://example.test/> . [] a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery \"SELECT id FROM people\" ]; rr:subjectMap [ rr:template \"https://example.test/person/{id}\" ]; rr:predicateObjectMap [ rr:predicate ex:kind ] .").unwrap();
    for (mapping, expected) in [
        (malformed, "RDF 语法错误"),
        (missing_object_map, "PredicateObjectMap 缺少 rr:objectMap"),
    ] {
        let spec = KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: None,
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        };
        assert!(
            matches!(VkgRuntime::new(spec, FakeSource { sql: String::new() }), Err(RuntimeError::Mapping(message)) if message.contains(expected))
        );
    }
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
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime.query("SELECT ?person { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Person> . }");
    assert!(
        matches!(result, Ok(rtop::QueryResult::Bindings(ref rows)) if rows.len() == 1),
        "{result:?}"
    );
}

#[test]
fn applies_imported_subproperty_axioms_when_querying_facts() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    let ontology = dir.path().join("ontology.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "<https://example.test/a> <https://example.test/child> <https://example.test/b> .",
    )
    .unwrap();
    std::fs::write(&ontology, "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . <https://example.test/child> rdfs:subPropertyOf <https://example.test/parent> .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: Some(ontology),
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?subject { ?subject <https://example.test/parent> <https://example.test/b> . }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn applies_subproperty_axioms_when_querying_mapping_results() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            _: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            Ok(vec![vec![Some("alice".into()), Some("course".into())]])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let ontology = dir.path().join("ontology.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/child> <https://example.test/{value}> .\nsource SELECT id, value FROM relation\n",
    )
    .unwrap();
    std::fs::write(
        &ontology,
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . <https://example.test/child> rdfs:subPropertyOf <https://example.test/parent> .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: Some(ontology),
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, Source).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?person { ?person <https://example.test/parent> <https://example.test/course> . }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn matches_a_native_obda_simple_literal_against_an_xsd_string_query_literal() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            _: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            Ok(vec![vec![Some("alice".into()), Some("Ada".into())]])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/name> {name} .\nsource SELECT id, name FROM people\n",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, Source).unwrap();
    let result = runtime.query("PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\nSELECT ?person { ?person <https://example.test/name> \"Ada\"^^xsd:string . }");
    assert!(
        matches!(result, Ok(rtop::QueryResult::Bindings(ref rows)) if rows.len() == 1),
        "{result:?}"
    );
}

#[test]
fn applies_ontology_domain_and_range_axioms_to_facts() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    let ontology = dir.path().join("ontology.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/alice> <https://example.test/teaches> <https://example.test/course> .").unwrap();
    std::fs::write(&ontology, "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . <https://example.test/teaches> rdfs:domain <https://example.test/Teacher>; rdfs:range <https://example.test/Course> .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: Some(ontology),
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?person { ?person <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Teacher> }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
    assert!(
        matches!(runtime.query("SELECT ?course { ?course <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Course> }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
}

#[test]
fn applies_ontology_inverse_property_axioms_to_facts() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    let ontology = dir.path().join("ontology.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/teacher> <https://example.test/teaches> <https://example.test/course> .").unwrap();
    std::fs::write(&ontology, "@prefix owl: <http://www.w3.org/2002/07/owl#> . <https://example.test/isTaughtBy> owl:inverseOf <https://example.test/teaches> .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: Some(ontology),
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?course { ?course <https://example.test/isTaughtBy> <https://example.test/teacher> }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
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
        xml_catalog_file: None,
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
fn queries_nquads_facts_in_a_named_graph_without_mixing_the_default_graph() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/s> <https://example.test/p> \"2022\" <https://example.test/extra> .\n<https://example.test/s> <https://example.test/p> \"default\" .\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime.query("SELECT ?value { GRAPH <https://example.test/extra> { <https://example.test/s> <https://example.test/p> ?value } }").unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"value\": Literal { value: \"2022\", datatype: None, language: None }}])"
    );

    let variable_graph = runtime
        .query("SELECT ?graph ?value { GRAPH ?graph { <https://example.test/s> <https://example.test/p> ?value } }")
        .unwrap();
    assert!(
        matches!(variable_graph, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("graph".into(), RdfTerm::Iri("https://example.test/extra".into())),
            ("value".into(), RdfTerm::Literal { value: "2022".into(), datatype: None, language: None }),
        ])])
    );

    let from_named = runtime
        .query("SELECT ?value FROM NAMED <https://example.test/extra> { GRAPH <https://example.test/extra> { <https://example.test/s> <https://example.test/p> ?value } }")
        .unwrap();
    assert!(matches!(from_named, rtop::QueryResult::Bindings(rows) if rows.len() == 1));
}

#[test]
fn binds_a_variable_predicate_for_nquads_facts_in_a_named_graph() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/s> <https://example.test/p> \"2022\" <https://example.test/extra> .\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query("SELECT ?predicate ?value { GRAPH <https://example.test/extra> { ?subject ?predicate ?value } }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"predicate\": Iri(\"https://example.test/p\"), \"value\": Literal { value: \"2022\", datatype: None, language: None }}])"
    );
}

#[test]
fn answers_a_fully_bound_ask_against_a_template_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query("ASK { <https://example.test/person/1> <https://example.test/type> <https://example.test/Person> }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Boolean(true)");
}

#[test]
fn queries_rdfxml_facts_with_the_explicit_base_iri_from_the_ontop_facts_case() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.rdf");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, r#"<?xml version="1.0"?><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:ex="https://example.test/"><rdf:Description rdf:about="factCompany"><ex:name>The Fact Company</ex:name></rdf:Description></rdf:RDF>"#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: Some("https://data.example.test/facts/".into()),
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query("SELECT ?company { ?company <https://example.test/name> \"The Fact Company\" }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"company\": Iri(\"https://data.example.test/facts/factCompany\")}])"
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
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime.query("SELECT ?person ?label { ?person <https://example.test/knows> ?friend . ?friend <https://example.test/label> ?label . }").unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"label\": Literal { value: \"B\", datatype: None, language: None }, \"person\": Iri(\"https://example.test/a\")}])");
}

#[test]
fn describes_facts_produced_by_a_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("DESCRIBE <7>"), Ok(rtop::QueryResult::Graph(facts)) if facts.len() == 1)
    );
}

#[test]
fn constructs_every_template_triple_from_a_basic_graph_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "@prefix ex: <https://example.test/> . ex:a ex:knows ex:b . ex:b ex:label \"B\" .",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    assert!(
        matches!(runtime.query("CONSTRUCT { ?person <https://example.test/knows> ?friend . ?friend <https://example.test/label> ?label . } WHERE { ?person <https://example.test/knows> ?friend . ?friend <https://example.test/label> ?label . }"), Ok(rtop::QueryResult::Graph(graph)) if graph.len() == 2)
    );
}

#[test]
fn constructs_a_virtual_mapping_triple_with_a_variable_predicate() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query("CONSTRUCT { ?subject ?predicate ?object } WHERE { ?subject ?predicate ?object }");
    assert!(
        matches!(
            &result,
            Ok(rtop::QueryResult::Graph(graph))
            if *graph == vec![rtop::RdfFact {
                subject: RdfTerm::Iri("7".into()),
                    predicate: "https://example.test/type".into(),
                object: RdfTerm::Iri("7".into()),
                    graph: None,
                }]
        ),
        "{result:?}"
    );
}

#[test]
fn preserves_r2rml_constant_language_tags_from_ontop_rdf4j_case() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("prof-cst-langtag.mapping.ttl");
    std::fs::write(&mapping, r#"@prefix : <http://example.org/voc#> .
@prefix rr: <http://www.w3.org/ns/r2rml#> .
<urn:MAPID-professor> a rr:TriplesMap;
  rr:logicalTable [ rr:sqlQuery "SELECT prof_id, last_name FROM professors" ];
  rr:predicateObjectMap [ rr:objectMap [ rr:constant "Professore"@it ]; rr:predicate :label ];
  rr:subjectMap [ rr:class :Professor; rr:template "http://example.org/professor/{prof_id}"; rr:termType rr:IRI ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, ProfessorSource).unwrap();
    let result = runtime
        .query("PREFIX : <http://example.org/voc#>\nSELECT ?v { ?p a :Professor . ?p :label ?v }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"v\": Literal { value: \"Professore\", datatype: None, language: Some(\"it\") }}])");
}

#[test]
fn filters_r2rml_language_tags_like_ontop_rdf4j_case() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("prof-cst-langtag.mapping.ttl");
    std::fs::write(&mapping, r#"@prefix : <http://example.org/voc#> . @prefix rr: <http://www.w3.org/ns/r2rml#> .
[] a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT prof_id, last_name FROM professors" ]; rr:predicateObjectMap [ rr:objectMap [ rr:constant "Professore"@it ]; rr:predicate :label ]; rr:subjectMap [ rr:class :Professor; rr:template "http://example.org/professor/{prof_id}" ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, ProfessorSource).unwrap();
    let result = runtime.query("PREFIX : <http://example.org/voc#>\nSELECT * WHERE { ?p a :Professor . ?p :label ?v FILTER (lang(?v)='it') }").unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"p\": Iri(\"42\"), \"v\": Literal { value: \"Professore\", datatype: None, language: Some(\"it\") }}])");
}

#[test]
fn preserves_r2rml_column_datatypes_from_ontop_d014() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmlc.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> .
@prefix dept: <http://example.com/dept#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
[] a rr:TriplesMap;
  rr:logicalTable [ rr:sqlQuery "SELECT deptno FROM DEPT" ];
  rr:subjectMap [ rr:template "http://example.com/dept/{deptno}" ];
  rr:predicateObjectMap [ rr:predicate dept:deptno; rr:objectMap [ rr:column "deptno"; rr:datatype xsd:positiveInteger ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, PairSource).unwrap();
    let result = runtime
        .query(
            "SELECT ?department ?number { ?department <http://example.com/dept#deptno> ?number }",
        )
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"department\": Iri(\"7\"), \"number\": Literal { value: \"7\", datatype: Some(\"http://www.w3.org/2001/XMLSchema#positiveInteger\"), language: None }}])");
}

#[test]
fn rejects_an_r2rml_object_map_with_both_datatype_and_language() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("invalid-object-map.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> .
@prefix ex: <https://example.test/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
[] a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT id FROM people" ];
   rr:subjectMap [ rr:template "https://example.test/{id}" ];
   rr:predicateObjectMap [ rr:predicate ex:value; rr:objectMap [ rr:column "id"; rr:datatype xsd:string; rr:language "en" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(spec, PairSource), Err(RuntimeError::Mapping(message)) if message.contains("rr:datatype 与 rr:language"))
    );
}

#[test]
fn preserves_r2rml_column_language_tags_from_ontop_d015() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmla.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
<map-en> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT Code, Name FROM Country WHERE Lan = 'EN'" ]; rr:subjectMap [ rr:template "http://example.com/{Code}" ]; rr:predicateObjectMap [ rr:predicate rdfs:label; rr:objectMap [ rr:column "Name"; rr:language "en" ] ] .
<map-es> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT Code, Name FROM Country WHERE Lan = 'ES'" ]; rr:subjectMap [ rr:template "http://example.com/{Code}" ]; rr:predicateObjectMap [ rr:predicate rdfs:label; rr:objectMap [ rr:column "Name"; rr:language "es" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, CountrySource).unwrap();
    let result = runtime
        .query("SELECT ?label { ?country <http://www.w3.org/2000/01/rdf-schema#label> ?label }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"label\": Literal { value: \"Bolivia, Plurinational State of\", datatype: None, language: Some(\"en\") }}, {\"label\": Literal { value: \"Estado Plurinacional de Bolivia\", datatype: None, language: Some(\"es\") }}])");
}

#[test]
fn rejects_invalid_r2rml_language_tags_from_ontop_d015() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmlb.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <https://example.test/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT id FROM people" ]; rr:subjectMap [ rr:template "https://example.test/{id}" ]; rr:predicateObjectMap [ rr:predicate ex:label; rr:objectMap [ rr:column "id"; rr:language "english" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(spec, PairSource), Err(RuntimeError::Mapping(message)) if message.contains("有效的 BCP47"))
    );
}

#[test]
fn resolves_named_r2rml_object_templates_from_ontop_d014() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmlc.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix emp: <http://example.com/emp#> .
<jobtypeObjectMap> a rr:ObjectMap; rr:template "http://example.com/emp/job/{job}" .
<TriplesMap2> a rr:TriplesMap; rr:logicalTable [ rr:tableName "EMP" ]; rr:subjectMap [ rr:template "http://example.com/emp/{empno}" ]; rr:predicateObjectMap [ rr:predicate emp:jobtype; rr:objectMap <jobtypeObjectMap> ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, EmployeeSource).unwrap();
    let result = runtime
        .query("SELECT ?employee ?jobtype { ?employee <http://example.com/emp#jobtype> ?jobtype }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"employee\": Iri(\"http://example.com/emp/7369\"), \"jobtype\": Iri(\"http://example.com/emp/job/CLERK\")}])");
}

#[test]
fn resolves_r2rml_ref_object_map_joins_from_ontop_d014() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmlc.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix emp: <http://example.com/emp#> . @prefix dept: <http://example.com/dept#> .
<TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT ('Department' || deptno) AS deptId, deptno FROM DEPT" ]; rr:subjectMap [ rr:column "deptId"; rr:termType rr:BlankNode ]; rr:predicateObjectMap [ rr:predicate dept:number; rr:objectMap [ rr:column "deptno" ] ] .
<TriplesMap2> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT empno, deptno FROM EMP" ]; rr:subjectMap [ rr:template "http://example.com/emp/{empno}" ]; rr:predicateObjectMap [ rr:predicate emp:c_ref_deptno; rr:objectMap [ a rr:RefObjectMap; rr:parentTriplesMap <TriplesMap1>; rr:joinCondition [ rr:child "deptno"; rr:parent "deptno" ] ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, RefObjectSource).unwrap();
    let result = runtime.query("SELECT ?employee ?department { ?employee <http://example.com/emp#c_ref_deptno> ?department }").unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"department\": BlankNode(\"Department10\"), \"employee\": Iri(\"http://example.com/emp/7369\")}])");
}

#[test]
fn combines_turtle_facts_and_mapping_results_from_ontop_facts_file_test() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.ttl");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://www.semanticweb.org/ontop-facts#> .
[] a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT id, name FROM company" ]; rr:subjectMap [ rr:template "https://example.test/company/{id}"; rr:class ex:Company ]; rr:predicateObjectMap [ rr:predicate ex:name; rr:objectMap [ rr:column "name" ] ] ."#).unwrap();
    std::fs::write(&facts, r#"@prefix : <http://www.semanticweb.org/ontop-facts#> . :factCompany a :Company; :name "The Fact Company"^^<http://www.w3.org/2001/XMLSchema#string> ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FactsFileSource).unwrap();
    let result = runtime.query("PREFIX : <http://www.semanticweb.org/ontop-facts#>\nSELECT DISTINCT ?v WHERE { ?c a :Company . ?c :name ?v. } ORDER BY ?v").unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"v\": Literal { value: \"Big Company\", datatype: None, language: None }}, {\"v\": Literal { value: \"Some Factory\", datatype: None, language: None }}, {\"v\": Literal { value: \"The Fact Company\", datatype: Some(\"http://www.w3.org/2001/XMLSchema#string\"), language: None }}])");
}

#[test]
fn rejects_disjoint_type_facts_from_ontop_university_tbox() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    let facts = dir.path().join("facts.ttl");
    let ontology = dir.path().join("university-complete.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "@prefix : <http://example.org/voc#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . <https://example.test/alice> a :Course, foaf:Person .").unwrap();
    std::fs::write(&ontology, "@prefix : <http://example.org/voc#> . @prefix owl: <http://www.w3.org/2002/07/owl#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . :Course owl:disjointWith foaf:Person .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: Some(ontology),
        xml_catalog_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(spec, FakeSource { sql: String::new() }), Err(RuntimeError::Ontology(message)) if message.contains("ontology inconsistent"))
    );
}

#[test]
fn expands_multiple_r2rml_template_columns_from_ontop_d002() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmla.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:tableName "Student" ]; rr:subjectMap [ rr:template "http://example.com/{\"ID\"}/{\"Name\"}" ]; rr:predicateObjectMap [ rr:predicate ex:id; rr:objectMap [ rr:column "\"ID\"" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, MultiTemplateSource).unwrap();
    let result = runtime
        .query("SELECT ?student ?id { ?student <http://example.com/id> ?id }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"id\": Literal { value: \"10\", datatype: None, language: None }, \"student\": Iri(\"http://example.com/10/Venus\")}])");
}

#[test]
fn supports_r2rml_direct_object_from_ontop_d016() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmla.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:tableName "\"Patient\"" ]; rr:subjectMap [ rr:template "http://example.com/Patient/{\"ID\"}" ]; rr:predicateObjectMap [ rr:predicate rdf:type; rr:object foaf:Person ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, DirectObjectSource).unwrap();
    let result = runtime.query("SELECT ?patient { ?patient <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> }").unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"patient\": Iri(\"http://example.com/Patient/1\")}])"
    );
}

#[test]
fn preserves_data_iri_object_templates_from_ontop_d016() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmle.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:tableName "\"Patient\"" ]; rr:subjectMap [ rr:template "http://example.com/Patient{\"ID\"}" ]; rr:predicateObjectMap [ rr:predicate ex:photo; rr:objectMap [ rr:template "data:image/png;hex,{\"Photo\"}" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, DataIriSource).unwrap();
    let result = runtime
        .query("SELECT ?patient ?photo { ?patient <http://example.com/photo> ?photo }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"patient\": Iri(\"http://example.com/Patient10\"), \"photo\": Iri(\"data:image/png;hex,89504E47\")}])");
}

#[test]
fn preserves_postgres_default_datatypes_for_ontop_d016_columns() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmlc.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:tableName "\"Patient\"" ]; rr:subjectMap [ rr:template "http://example.com/Patient{\"ID\"}" ];
rr:predicateObjectMap [ rr:predicate ex:birthdate; rr:objectMap [ rr:column "\"BirthDate\"" ] ];
rr:predicateObjectMap [ rr:predicate ex:entrancedate; rr:objectMap [ rr:column "\"EntranceDate\"" ] ];
rr:predicateObjectMap [ rr:predicate ex:paid; rr:objectMap [ rr:column "\"PaidInAdvance\"" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, TypedD016Source).unwrap();
    let result = runtime.query("SELECT ?birth ?entrance ?paid { ?patient <http://example.com/birthdate> ?birth . ?patient <http://example.com/entrancedate> ?entrance . ?patient <http://example.com/paid> ?paid }").unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"birth\": Literal { value: \"1981-10-10\", datatype: Some(\"http://www.w3.org/2001/XMLSchema#date\"), language: None }, \"entrance\": Literal { value: \"2009-10-10T12:12:22\", datatype: Some(\"http://www.w3.org/2001/XMLSchema#dateTime\"), language: None }, \"paid\": Literal { value: \"false\", datatype: Some(\"http://www.w3.org/2001/XMLSchema#boolean\"), language: None }}])");
}

#[test]
fn supports_r2rml_constant_subject_predicate_map_and_graph_map_from_ontop_d006() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmla.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> .
<TriplesMap1> a rr:TriplesMap; rr:logicalTable [ rr:tableName "\"Student\"" ]; rr:subjectMap [ rr:constant ex:BadStudent; rr:graphMap [ rr:constant <http://example.com/graph/student> ] ]; rr:predicateObjectMap [ rr:predicateMap [ rr:constant ex:description ]; rr:objectMap [ rr:constant "Bad Student" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NamedGraphSource).unwrap();
    let result = runtime.query("SELECT ?student { GRAPH <http://example.com/graph/student> { ?student <http://example.com/description> \"Bad Student\" } }").unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"student\": Iri(\"http://example.com/BadStudent\")}])"
    );
    assert!(matches!(
        runtime
            .query("SELECT ?student { ?student <http://example.com/description> \"Bad Student\" }"),
        Err(RuntimeError::NotFullyTranslatable(_))
    ));
}

#[test]
fn supports_direct_r2rml_graph_and_rejects_literal_graph_map_from_ontop_d007() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmlb.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:tableName "\"Student\"" ]; rr:subjectMap [ rr:constant ex:Student; rr:graph ex:PersonGraph ]; rr:predicateObjectMap [ rr:predicate ex:label; rr:objectMap [ rr:constant "Student" ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NamedGraphSource).unwrap();
    assert!(
        matches!(runtime.query("SELECT ?student { GRAPH <http://example.com/PersonGraph> { ?student <http://example.com/label> \"Student\" } }"), Ok(rtop::QueryResult::Bindings(rows)) if rows.len() == 1)
    );
    let invalid = dir.path().join("r2rmlh.ttl");
    std::fs::write(&invalid, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix ex: <http://example.com/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:tableName "\"Student\"" ]; rr:subjectMap [ rr:constant ex:Student; rr:graphMap [ rr:column "Name"; rr:termType rr:Literal ] ]; rr:predicateObjectMap [ rr:predicate ex:label; rr:objectMap [ rr:constant "Student" ] ] ."#).unwrap();
    let invalid_spec = KnowledgeGraphSpec {
        mapping_file: invalid,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    assert!(
        matches!(VkgRuntime::new(invalid_spec, NamedGraphSource), Err(RuntimeError::Mapping(message)) if message.contains("graphMap"))
    );
}

#[test]
fn preserves_literal_object_templates_from_ontop_d003() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("r2rmlc.ttl");
    std::fs::write(&mapping, r#"@prefix rr: <http://www.w3.org/ns/r2rml#> . @prefix foaf: <http://xmlns.com/foaf/0.1/> .
[] a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery "SELECT id, first, last FROM people" ]; rr:subjectMap [ rr:template "http://example.com/person/{id}" ]; rr:predicateObjectMap [ rr:predicate foaf:name; rr:objectMap [ rr:template "{first} {last}"; rr:termType rr:Literal ] ] ."#).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, LiteralTemplateSource).unwrap();
    let result = runtime
        .query("SELECT ?person ?name { ?person <http://xmlns.com/foaf/0.1/name> ?name }")
        .unwrap();
    assert_eq!(format!("{result:?}"), "Bindings([{\"name\": Literal { value: \"Ada Lovelace\", datatype: None, language: None }, \"person\": Iri(\"http://example.com/person/1\")}])");
}
