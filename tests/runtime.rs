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

struct LanguagePairSource;
impl DataSource for LanguagePairSource {
    fn execute(
        &mut self,
        sql: &str,
        _: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        if sql.contains("left_titles") {
            Ok(vec![vec![Some("Crime and Punishment".into())]])
        } else if sql.contains("right_titles") {
            Ok(vec![vec![Some("SPARQL Tutorial".into())]])
        } else {
            Ok(vec![])
        }
    }
}

/// 固定 Ontop `R2RMLConversionTest` 的 native mapping 都由单行 source 驱动。
/// 这里的 adapter 只模拟 PostgreSQL 已投影的行值；断言仍通过 VkgRuntime 的公开
/// 查询 seam 观察 RDF term，而不是窥探 Mapping 的私有规则。
struct ConversionAssetSource {
    row: Vec<Option<String>>,
}

impl DataSource for ConversionAssetSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![self.row.clone()])
    }
}

struct PairSource;
impl DataSource for PairSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![vec![Some("7".into()), Some("7".into())]])
    }
}

struct NamedGraphMappingSource;
impl DataSource for NamedGraphMappingSource {
    fn execute(&mut self, _: &str, _: &[String]) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        Ok(vec![vec![
            Some("https://example.test/s".into()),
            Some("named value".into()),
            Some("https://example.test/graph".into()),
        ]])
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
        mapping_file: mapping.clone(),
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
fn runs_ontop_r2rml_conversion_native_assets_through_the_public_runtime_seam() {
    // 来源：固定 Ontop mapping/sql/all 的 R2RMLConversionTest。它验证 native
    // mapping 转 R2RML 时 column、constant、plain literal、language 和 IRI 的
    // term-map 语义；这里以 PostgreSQL 投影后的行值从 VkgRuntime 重新观察它们。
    let baseline = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ontop/mapping/sql/all/src/test/resources");
    let r2rml_baseline = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ontop/test/rdb2rdf-compliance/src/test/resources");
    let converted = tempfile::tempdir().unwrap();
    let spec_for = |asset: &str| KnowledgeGraphSpec {
        mapping_file: {
            let native = rtop::Mapping::parse_file(&baseline.join(asset)).unwrap();
            let output = converted.path().join(format!("{asset}.ttl"));
            std::fs::write(&output, native.to_r2rml()).unwrap();
            output
        },
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };

    let mut typed_column = VkgRuntime::new(
        spec_for("npd-column-mapping.obda"),
        ConversionAssetSource {
            row: vec![
                Some("http://sws.ifi.uio.no/data/npd-v2/wellbore/7/point".into()),
                Some("2024-01-02T03:04:05".into()),
            ],
        },
    )
    .unwrap();
    let typed = typed_column
        .query("SELECT ?s ?o WHERE { ?s <http://sws.ifi.uio.no/vocab/npd-v2#dateSyncNPD> ?o }")
        .unwrap();
    assert!(
        matches!(typed, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("s"), Some(RdfTerm::Iri(value)) if value == "http://sws.ifi.uio.no/data/npd-v2/wellbore/7/point")
        && matches!(rows[0].get("o"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value == "2024-01-02T03:04:05" && datatype == "http://www.w3.org/2001/XMLSchema#dateTime")),
        "{typed:?}"
    );

    let mut constant = VkgRuntime::new(
        spec_for("npd-constant-mapping.obda"),
        ConversionAssetSource {
            row: vec![
                Some("http://sws.ifi.uio.no/data/npd-v2/survey/North".into()),
                Some("true".into()),
            ],
        },
    )
    .unwrap();
    let constant = constant
        .query("SELECT ?s ?o WHERE { ?s <http://sws.ifi.uio.no/vocab/npd-v2#isShallowDrillingPerformed> ?o }")
        .unwrap();
    assert!(
        matches!(constant, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("s"), Some(RdfTerm::Iri(value)) if value == "http://sws.ifi.uio.no/data/npd-v2/survey/North")
        && matches!(rows[0].get("o"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value == "true" && datatype == "http://www.w3.org/2001/XMLSchema#boolean")),
        "{constant:?}"
    );

    let mut language = VkgRuntime::new(
        spec_for("langstring-mapping.obda"),
        ConversionAssetSource {
            row: vec![
                Some("http://example.org/bookbook-1".into()),
                Some("A title".into()),
            ],
        },
    )
    .unwrap();
    let language = language
        .query("SELECT ?title WHERE { ?book <http://purl.org/dc/elements/1.1/title> ?title }")
        .unwrap();
    assert!(
        matches!(language, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("title"), Some(RdfTerm::Literal { value, datatype: None, language: Some(language) }) if value == "A title" && language == "en")),
        "{language:?}"
    );

    let mut constant_iri = VkgRuntime::new(
        spec_for("npd-constant-iri-mapping.obda"),
        ConversionAssetSource {
            row: vec![
                Some("http://sws.ifi.uio.no/data/npd-v2/wellbore/point".into()),
                Some("Fake point".into()),
            ],
        },
    )
    .unwrap();
    let constant_iri = constant_iri
        .query("SELECT ?s ?o WHERE { ?s <http://www.w3.org/2000/01/rdf-schema#label> ?o }")
        .unwrap();
    assert!(
        matches!(constant_iri, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("s"), Some(RdfTerm::Iri(value)) if value == "http://sws.ifi.uio.no/data/npd-v2/wellbore/point")
        && matches!(rows[0].get("o"), Some(RdfTerm::Literal { value, .. }) if value == "Fake point")),
        "{constant_iri:?}"
    );

    // 反向转换：R2RML 的 blank node / column term 与 subject graph 在写成 native
    // OBDA 后重新加载，仍须从同一 runtime seam 观察到相同 term 与 graph。
    let r2rml_to_native_spec = |asset: &str| KnowledgeGraphSpec {
        mapping_file: {
            let r2rml = rtop::Mapping::parse_file(&r2rml_baseline.join(asset)).unwrap();
            let output = converted
                .path()
                .join(format!("{}.obda", asset.replace('/', "_")));
            std::fs::write(&output, r2rml.to_native_obda()).unwrap();
            output
        },
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut blank_node = VkgRuntime::new(
        r2rml_to_native_spec("D002/r2rmlb.ttl"),
        ConversionAssetSource {
            row: vec![Some("students10".into()), Some("Venus".into())],
        },
    )
    .unwrap();
    let blank_node = blank_node
        .query("SELECT ?s ?name WHERE { ?s <http://xmlns.com/foaf/0.1/name> ?name }")
        .unwrap();
    assert!(
        matches!(blank_node, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("s"), Some(RdfTerm::BlankNode(value)) if value == "students10")
        && matches!(rows[0].get("name"), Some(RdfTerm::Literal { value, .. }) if value == "Venus")),
        "{blank_node:?}"
    );

    let mut named_graph = VkgRuntime::new(
        r2rml_to_native_spec("D007/r2rmlb.ttl"),
        ConversionAssetSource {
            row: vec![
                Some("http://example.com/Student/10/Venus%20Williams".into()),
                Some("Venus Williams".into()),
            ],
        },
    )
    .unwrap();
    let named_graph = named_graph
        .query("SELECT ?name WHERE { GRAPH <http://example.com/PersonGraph> { ?s <http://xmlns.com/foaf/0.1/name> ?name } }")
        .unwrap();
    assert!(
        matches!(named_graph, rtop::QueryResult::Bindings(ref rows) if rows.len() == 1
        && matches!(rows[0].get("name"), Some(RdfTerm::Literal { value, .. }) if value == "Venus Williams")),
        "{named_graph:?}"
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
        Err(RuntimeError::NotFullyTranslatable(_))
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
        mapping_file: mapping.clone(),
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
    let result = runtime.query("SELECT ?replacement ?summary ?contains WHERE { ?x <https://example.test/title> ?title . BIND(REPLACE(?title, \"Second\", \"First\") AS ?replacement) BIND(CONCAT(UCASE(?title), \" / \", STR(STRLEN(?title))) AS ?summary) BIND(CONTAINS(?title, \"Second\") AS ?contains) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows.len() == 1
        && rows[0].get("replacement") == Some(&RdfTerm::Literal { value: "The First Book".into(), datatype: None, language: Some("en".into()) })
        // DAWG concat02：语言标签 literal 与普通字符串混合时，CONCAT 产生
        // simple literal，而非虚构 xsd:string 或保留语言标签。
        && rows[0].get("summary") == Some(&RdfTerm::Literal { value: "THE SECOND BOOK / 15".into(), datatype: None, language: None })
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
            // 整数除法产生 xsd:decimal，并保留 decimal lexical 的小数位；
            // 这与固定 DAWG coalesce01.srx 的 0.0/2.0 一致。
            ("half".into(), RdfTerm::Literal { value: "5.0".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()), language: None }),
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
fn evaluates_bind_sha256_as_a_lowercase_simple_literal() {
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
        matches!(result, rtop::QueryResult::Bindings(ref rows) if *rows == vec![std::collections::BTreeMap::from([(
            "hash".into(),
        RdfTerm::Literal { value: "5534117f459c7ead3015f0f6adf2c8a7c9f577be686cd8e3a13f37176678c272".into(), datatype: None, language: None }
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
        && matches!(row.get("rand"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value.parse::<f64>().is_ok_and(|value| (0.0..1.0).contains(&value)) && datatype == "http://www.w3.org/2001/XMLSchema#double")))
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
fn filters_an_in_match_when_a_later_candidate_has_an_expression_error() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/unused/{id}> <https://example.test/type> <https://example.test/Unused> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        "<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .",
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
    let result = runtime
        .query("SELECT ?person WHERE { ?person <https://example.test/type> <https://example.test/Person> . FILTER(?person IN (<https://example.test/person/1>, 1 / 0)) }")
        .unwrap();
    assert!(matches!(result, rtop::QueryResult::Bindings(rows) if rows.is_empty()));
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
        matches!(result, rtop::QueryResult::Bindings(ref rows) if *rows == vec![
            std::collections::BTreeMap::from([("age".into(), RdfTerm::Literal { value: "36".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }), ("name".into(), RdfTerm::Literal { value: "Ada".into(), datatype: None, language: None }), ("person".into(), RdfTerm::Iri("https://example.test/a".into()))]),
            std::collections::BTreeMap::from([("name".into(), RdfTerm::Literal { value: "Bob".into(), datatype: None, language: None }), ("person".into(), RdfTerm::Iri("https://example.test/b".into()))]),
        ])
    );
}

#[test]
fn applies_ontop_values07_after_optional_without_binding_missing_values() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bindings/data07.ttl"
        ),
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
    let result = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bindings/values07.rq"
        ))
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("VALUES 结果必须是 bindings")
    };
    // `values07.srx` 的每一个 expected row 都有同一 outer VALUES binding。未带
    // foaf:knows 的 :c 仍可与 ?o2=:b 合并；这里比较 bag 的长度和完整 RDF terms，
    // 不把无 ORDER BY 的数据库/事实读取顺序当契约。
    assert_eq!(rows.len(), 5, "{rows:?}");
    let expected = [
        ("a", RdfTerm::Iri("http://example.org/b".into())),
        (
            "a",
            RdfTerm::Literal {
                value: "Alan".into(),
                datatype: None,
                language: None,
            },
        ),
        (
            "a",
            RdfTerm::Literal {
                value: "alan@example.org".into(),
                datatype: None,
                language: None,
            },
        ),
        (
            "c",
            RdfTerm::Literal {
                value: "Alice".into(),
                datatype: None,
                language: None,
            },
        ),
        (
            "c",
            RdfTerm::Literal {
                value: "alice@example.org".into(),
                datatype: None,
                language: None,
            },
        ),
    ];
    for (subject, object) in expected {
        assert!(
            rows.iter().any(|row| {
                row.get("s") == Some(&RdfTerm::Iri(format!("http://example.org/{subject}")))
                    && row.get("o1") == Some(&object)
                    && row.get("o2") == Some(&RdfTerm::Iri("http://example.org/b".into()))
            }),
            "缺少 values07.srx 结果 subject={subject} object={object:?}: {rows:?}"
        );
    }
}

#[test]
fn applies_ontop_values08_tuple_undef_in_each_column() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bindings/data08.ttl"
        ),
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
    let result = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bindings/values08.rq"
        ))
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("VALUES 结果必须是 bindings")
    };
    // `values08.srx` 的两个 tuple row 分别让 ?book 和 ?title 未绑定。验证它们
    // 只约束另一列，而不会把 UNDEF 错当 lexical RDF term。
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.contains(&std::collections::BTreeMap::from([
        (
            "book".into(),
            RdfTerm::Iri("http://example.org/book/book1".into())
        ),
        (
            "title".into(),
            RdfTerm::Literal {
                value: "SPARQL Tutorial".into(),
                datatype: None,
                language: None,
            },
        ),
        (
            "price".into(),
            RdfTerm::Literal {
                value: "42".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                language: None,
            },
        ),
    ])));
    assert!(rows.contains(&std::collections::BTreeMap::from([
        (
            "book".into(),
            RdfTerm::Iri("http://example.org/book/book2".into())
        ),
        (
            "title".into(),
            RdfTerm::Literal {
                value: "The Semantic Web".into(),
                datatype: None,
                language: None,
            },
        ),
        (
            "price".into(),
            RdfTerm::Literal {
                value: "23".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                language: None,
            },
        ),
    ])));
}

#[test]
fn applies_ontop_exists_manifest_positive_and_empty_results() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("set-data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/negation/set-data.ttl"
        ),
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

    let positive = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/negation/exists-01.rq"
        ))
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = positive else {
        panic!("EXISTS 结果必须是 bindings")
    };
    // `exists-01.srx`：相关 ?set 只有 b、d 含有 :member 9；不依赖未排序结果的行序。
    assert_eq!(rows.len(), 2, "{rows:?}");
    for set in ["b", "d"] {
        assert!(
            rows.iter().any(|row| {
                row.get("set") == Some(&RdfTerm::Iri(format!("http://example/{set}")))
            }),
            "缺少 exists-01.srx 的 ?set=:{set}：{rows:?}"
        );
    }

    let empty = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/negation/exists-02.rq"
        ))
        .unwrap();
    assert!(
        matches!(empty, rtop::QueryResult::Bindings(ref rows) if rows.is_empty()),
        "exists-02.srx 应为空：{empty:?}"
    );
}

#[test]
fn applies_ontop_negation_manifest_minus_minuend_assets() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/negation",
    );
    let temp = tempfile::tempdir().unwrap();
    let mapping = temp.path().join("empty.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/unused/{id}> <https://example.test/type> <https://example.test/Unused> .\nsource SELECT id FROM unused\n",
    )
    .unwrap();

    let run = |data: &str, query: &str| {
        let facts = temp.path().join(data);
        std::fs::write(
            &facts,
            std::fs::read_to_string(directory.join(data)).unwrap(),
        )
        .unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(&std::fs::read_to_string(directory.join(query)).unwrap())
            .unwrap_or_else(|error| panic!("{query} 应执行：{error}"))
        else {
            panic!("{query} 应返回 bindings")
        };
        rows
    };
    for (query, data, expected) in [
        ("full-minuend.rq", "full-minuend.ttl", ["a0", "a3"]),
        ("full-minuend-modified.rq", "full-minuend.ttl", ["a0", "a3"]),
        ("part-minuend.rq", "part-minuend.ttl", ["a2", "a4"]),
        ("part-minuend-modified.rq", "part-minuend.ttl", ["a2", "a4"]),
    ] {
        let rows = run(data, query);
        assert_eq!(rows.len(), expected.len(), "{query}.srx：{rows:?}");
        for subject in expected {
            assert!(
                rows.iter().any(|row| {
                    row.get("a") == Some(&RdfTerm::Iri(format!("http://example/{subject}")))
                }),
                "{query}.srx 缺少 ?a=:{subject}：{rows:?}"
            );
        }
        if data == "full-minuend.ttl" {
            for subject in ["a0", "a3"] {
                let suffix = subject.trim_start_matches('a');
                let row = rows
                    .iter()
                    .find(|row| {
                        row.get("a") == Some(&RdfTerm::Iri(format!("http://example/{subject}")))
                    })
                    .expect("subject 已验证存在");
                assert_eq!(
                    row.get("b"),
                    Some(&RdfTerm::Iri(format!("http://example/b{suffix}"))),
                    "{query}.srx 的 ?b"
                );
                assert_eq!(
                    row.get("c"),
                    Some(&RdfTerm::Iri(format!("http://example/c{suffix}"))),
                    "{query}.srx 的 ?c"
                );
            }
        } else {
            let a2 = rows
                .iter()
                .find(|row| row.get("a") == Some(&RdfTerm::Iri("http://example/a2".into())))
                .expect("part-minuend.srx 的 a2");
            assert_eq!(a2.get("b"), Some(&RdfTerm::Iri("http://example/b2".into())));
            assert!(!a2.contains_key("c"));
            let a4 = rows
                .iter()
                .find(|row| row.get("a") == Some(&RdfTerm::Iri("http://example/a4".into())))
                .expect("part-minuend.srx 的 a4");
            assert!(!a4.contains_key("b") && !a4.contains_key("c"));
        }
    }
}

#[test]
fn applies_ontop_negation_manifest_subset_by_exclusion_assets() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/negation",
    );
    let temp = tempfile::tempdir().unwrap();
    let mapping = temp.path().join("empty.obda");
    let facts = temp.path().join("subsetByExcl.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/unused/{id}> <https://example.test/type> <https://example.test/Unused> .\nsource SELECT id FROM unused\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        std::fs::read_to_string(directory.join("subsetByExcl.ttl")).unwrap(),
    )
    .unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(&std::fs::read_to_string(directory.join(query)).unwrap())
            .unwrap_or_else(|error| panic!("{query} 应执行：{error}"))
        else {
            panic!("{query} 应返回 bindings")
        };
        rows
    };
    let base = "http://www.w3.org/2009/sparql/docs/tests/data-sparql11/negation#lifeForm";
    for (query, expected) in [
        ("subsetByExcl01.rq", ["1", "2"].as_slice()),
        ("subsetByExcl02.rq", ["1"].as_slice()),
    ] {
        let rows = run(query);
        assert_eq!(rows.len(), expected.len(), "{query}.srx：{rows:?}");
        for suffix in expected {
            assert!(
                rows.iter().any(|row| {
                    row.get("animal") == Some(&RdfTerm::Iri(format!("{base}{suffix}")))
                }),
                "{query}.srx 缺少 lifeForm{suffix}：{rows:?}"
            );
        }
    }
}

#[test]
fn applies_ontop_negation_manifest_temporal_proximity_not_exists_asset() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/negation",
    );
    let temp = tempfile::tempdir().unwrap();
    let mapping = temp.path().join("empty.obda");
    let facts = temp.path().join("temporalProximity01.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/unused/{id}> <https://example.test/type> <https://example.test/Unused> .\nsource SELECT id FROM unused\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        std::fs::read_to_string(directory.join("temporalProximity01.ttl")).unwrap(),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(&std::fs::read_to_string(directory.join("temporalProximity01.rq")).unwrap())
        .unwrap_or_else(|error| panic!("temporalProximity01.rq 应执行：{error}"))
    else {
        panic!("temporalProximity01.rq 应返回 bindings")
    };
    assert_eq!(rows.len(), 1, "temporalProximity01.srx：{rows:?}");
    let row = &rows[0];
    assert_eq!(
        row.get("exam"),
        Some(&RdfTerm::Iri(
            "http://www.w3.org/2009/sparql/docs/tests/data-sparql11/negation#examination1".into()
        ))
    );
    assert_eq!(
        row.get("date"),
        Some(&RdfTerm::Literal {
            value: "2010-01-10".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#date".into()),
            language: None,
        })
    );
}

#[test]
fn applies_ontop_negation_manifest_set_subset_assets() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/negation",
    );
    let temp = tempfile::tempdir().unwrap();
    let mapping = temp.path().join("empty.obda");
    let facts = temp.path().join("set-data.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/unused/{id}> <https://example.test/type> <https://example.test/Unused> .\nsource SELECT id FROM unused\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        std::fs::read_to_string(directory.join("set-data.ttl")).unwrap(),
    )
    .unwrap();
    for (query, expected_rows) in [
        ("subset-01.rq", 11),
        ("subset-02.rq", 11),
        ("set-equals-1.rq", 2),
        ("subset-03.rq", 7),
    ] {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(&std::fs::read_to_string(directory.join(query)).unwrap())
            .unwrap_or_else(|error| panic!("{query} 应执行：{error}"))
        else {
            panic!("{query} 应返回 bindings")
        };
        assert_eq!(rows.len(), expected_rows, "{query}.srx：{rows:?}");
        assert!(
            rows.iter()
                .all(|row| row.contains_key("subset") || row.contains_key("s1")),
            "{query}.srx 应保留原始 projection variables：{rows:?}"
        );
    }
}

#[test]
fn applies_ontop_bindings_manifest_values_and_inline_assets() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bindings",
    );
    let temp = tempfile::tempdir().unwrap();
    let mapping = temp.path().join("empty.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/unused/{id}> <https://example.test/type> <https://example.test/Unused> .\nsource SELECT id FROM unused\n",
    )
    .unwrap();
    for (query, data, expected_rows) in [
        ("values01.rq", "data01.ttl", 1),
        ("values02.rq", "data02.ttl", 1),
        ("values03.rq", "data03.ttl", 1),
        ("values04.rq", "data04.ttl", 3),
        ("values05.rq", "data05.ttl", 6),
        ("values06.rq", "data06.ttl", 1),
        ("inline01.rq", "data01.ttl", 1),
        ("inline02.rq", "data02.ttl", 1),
    ] {
        let facts = temp.path().join(data);
        std::fs::write(
            &facts,
            std::fs::read_to_string(directory.join(data)).unwrap(),
        )
        .unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(&std::fs::read_to_string(directory.join(query)).unwrap())
            .unwrap_or_else(|error| panic!("{query} 应执行：{error}"))
        else {
            panic!("{query} 应返回 bindings")
        };
        assert_eq!(rows.len(), expected_rows, "{query}.srx：{rows:?}");
    }
}

#[test]
fn applies_ontop_groupconcat_manifest_default_custom_separator_and_subquery() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-groupconcat-1.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-groupconcat-1.ttl"
        ),
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

    for query in [
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-groupconcat-1.rq"
        ),
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-groupconcat-3.rq"
        ),
    ] {
        assert!(
            matches!(runtime.query(query).unwrap_or_else(|error| panic!("GROUP_CONCAT query 失败：{error}\n{query}")), rtop::QueryResult::Boolean(true)),
            "GROUP_CONCAT ASK 必须匹配其 SRX boolean true"
        );
    }
    let result = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-groupconcat-2.rq"
        ))
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([(
            "c".into(),
            RdfTerm::Literal {
                value: "2".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                language: None,
            },
        )])]),
        "agg-groupconcat-2.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_count_manifest_grouping_and_having() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg01.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg01.ttl"
        ),
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
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    for query in [
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg01.rq"
        ),
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg04.rq"
        ),
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg06.rq"
        ),
    ] {
        assert!(
            matches!(runtime.query(query).unwrap(), rtop::QueryResult::Bindings(ref rows)
                if rows == &[std::collections::BTreeMap::from([("C".into(), integer("5"))])]),
            "无分组 COUNT 必须匹配对应 SRX"
        );
    }
    for query in [
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg02.rq"
        ),
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg05.rq"
        ),
    ] {
    let grouped = runtime
        .query(query)
        .unwrap();
    let expected_grouped = [("p1", "3"), ("p2", "2")];
    assert!(matches!(grouped, rtop::QueryResult::Bindings(ref rows) if rows.len() == 2));
    let rtop::QueryResult::Bindings(rows) = grouped else {
        unreachable!()
    };
    for (predicate, count) in expected_grouped {
        assert!(
            rows.iter().any(|row| {
                row.get("P") == Some(&RdfTerm::Iri(format!("http://www.example.org/{predicate}")))
                    && row.get("C") == Some(&integer(count))
            }),
            "COUNT group SRX 缺少 {predicate}/{count}: {rows:?}"
        );
    }
    }
    for query in [
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg03.rq"
        ),
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg07.rq"
        ),
    ] {
    let having = runtime
        .query(query)
        .unwrap();
    assert!(
        matches!(having, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([
            ("P".into(), RdfTerm::Iri("http://www.example.org/p1".into())),
            ("C".into(), integer("3")),
        ])]),
        "COUNT HAVING SRX：{having:?}"
    );
    }
}

#[test]
fn applies_ontop_bind_manifest_filter_scope_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl"),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let outside_scope = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind10.rq"))
        .unwrap();
    assert!(
        matches!(outside_scope, rtop::QueryResult::Bindings(ref rows) if rows.is_empty()),
        "bind10.srx：{outside_scope:?}"
    );
    let inside_scope = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind11.rq"))
        .unwrap();
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    assert!(
        matches!(inside_scope, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([
            ("s".into(), RdfTerm::Iri("http://example.org/s4".into())),
            ("v".into(), integer("4")),
            ("z".into(), integer("4")),
        ])]),
        "bind11.srx：{inside_scope:?}"
    );
}

#[test]
fn applies_ontop_bind_manifest_alias_in_following_bgp() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl"),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind03.rq"))
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("bind03.srx 应返回 bindings")
    };
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    assert_eq!(rows.len(), 3, "bind03.srx：{rows:?}");
    for (value, subject) in [("2", "s2"), ("3", "s3"), ("4", "s4")] {
        assert!(
            rows.contains(&std::collections::BTreeMap::from([
                ("z".into(), integer(value)),
                (
                    "s1".into(),
                    RdfTerm::Iri(format!("http://example.org/{subject}")),
                ),
            ])),
            "bind03.srx result bag 缺少 {value}/{subject}：{rows:?}"
        );
    }
}

#[test]
fn applies_ontop_bind_manifest_unbound_expression_error() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl"),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind04.rq"))
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("bind04.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 4, "bind04.srx：{rows:?}");
    for (subject, value) in [("s1", "1"), ("s2", "2"), ("s3", "3"), ("s4", "4")] {
        let row = rows
            .iter()
            .find(|row| {
                row.get("s") == Some(&RdfTerm::Iri(format!("http://example.org/{subject}")))
            })
            .unwrap_or_else(|| panic!("bind04.srx 缺少 {subject}：{rows:?}"));
        assert!(
            !row.contains_key("z"),
            "未绑定 ?nova 的 BIND 不应绑定 ?z：{row:?}"
        );
        assert_eq!(
            row.get("o"),
            Some(&RdfTerm::Literal {
                value: value.into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                language: None,
            })
        );
    }
}

#[test]
fn applies_ontop_bind_manifest_alias_in_following_filter() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl"),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind05.rq"))
        .unwrap();
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([
            ("s".into(), RdfTerm::Iri("http://example.org/s2".into())),
            ("p".into(), RdfTerm::Iri("http://example.org/p".into())),
            ("o".into(), integer("2")),
            ("z".into(), integer("3")),
        ])]),
        "bind05.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_bind_manifest_filter_before_bind_alias() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind08.rq")).unwrap();
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([("s".into(), RdfTerm::Iri("http://example.org/s2".into())), ("p".into(), RdfTerm::Iri("http://example.org/p".into())), ("o".into(), integer("2")), ("z".into(), integer("3"))])]),
        "bind08.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_bind_manifest_select_star_projection() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind06.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("bind06.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 4, "bind06.srx：{rows:?}");
    for (subject, object, alias) in [
        ("s1", "1", "11"),
        ("s2", "2", "12"),
        ("s3", "3", "13"),
        ("s4", "4", "14"),
    ] {
        let row = rows
            .iter()
            .find(|row| {
                row.get("s") == Some(&RdfTerm::Iri(format!("http://example.org/{subject}")))
            })
            .unwrap_or_else(|| panic!("缺少 {subject}：{rows:?}"));
        let integer = |value: &str| RdfTerm::Literal {
            value: value.into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        };
        assert_eq!(row.get("o"), Some(&integer(object)));
        assert_eq!(row.get("z"), Some(&integer(alias)));
    }
}

#[test]
fn applies_ontop_bind_manifest_union_branch_alias_scope() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind07.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("bind07.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 8, "bind07.srx：{rows:?}");
    for (subject, value) in [("s1", "1"), ("s2", "2"), ("s3", "3"), ("s4", "4")] {
        let matches = rows
            .iter()
            .filter(|row| {
                row.get("s") == Some(&RdfTerm::Iri(format!("http://example.org/{subject}")))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            2,
            "bind07.srx {subject} bag multiplicity：{rows:?}"
        );
        for row in matches {
            assert!(
                !row.contains_key("z"),
                "UNION branch alias 不应泄漏：{row:?}"
            );
            assert_eq!(
                row.get("o"),
                Some(&RdfTerm::Literal {
                    value: value.into(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                    language: None
                })
            );
        }
    }
}

#[test]
fn applies_ontop_bind_manifest_basic_numeric_bind() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind01.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("bind01.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 4, "bind01.srx：{rows:?}");
    for value in ["11", "12", "13", "14"] {
        assert!(
            rows.contains(&std::collections::BTreeMap::from([(
                "z".into(),
                RdfTerm::Literal {
                    value: value.into(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                    language: None
                }
            )])),
            "bind01.srx 缺少 {value}：{rows:?}"
        );
    }
}

#[test]
fn applies_ontop_bind_manifest_parallel_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/bind/bind02.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("bind02.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 4, "bind02.srx：{rows:?}");
    for (object, first, second) in [
        ("1", "11", "101"),
        ("2", "12", "102"),
        ("3", "13", "103"),
        ("4", "14", "104"),
    ] {
        let integer = |value: &str| RdfTerm::Literal {
            value: value.into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        };
        assert!(
            rows.contains(&std::collections::BTreeMap::from([
                ("o".into(), integer(object)),
                ("z".into(), integer(first)),
                ("z2".into(), integer(second))
            ])),
            "bind02.srx 缺少 {object}/{first}/{second}：{rows:?}"
        );
    }
}

#[test]
fn applies_ontop_project_expression_equality_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("projexp01.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp01.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp01.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("projexp01.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 2, "projexp01.srx：{rows:?}");
    for (z, eq) in [("1", "true"), ("2", "false")] {
        let integer = |value: &str| RdfTerm::Literal {
            value: value.into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        };
        assert!(
            rows.contains(&std::collections::BTreeMap::from([
                (
                    "x".into(),
                    RdfTerm::Iri("http://www.example.org/instance#a".into())
                ),
                ("y".into(), integer("1")),
                ("z".into(), integer(z)),
                (
                    "eq".into(),
                    RdfTerm::Literal {
                        value: eq.into(),
                        datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()),
                        language: None
                    }
                )
            ])),
            "projexp01.srx 缺少 z={z}：{rows:?}"
        );
    }
}

#[test]
fn applies_ontop_project_expression_error_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("projexp02.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp02.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp02.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("projexp02.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 2, "projexp02.srx：{rows:?}");
    let numeric = rows
        .iter()
        .find(|row| {
            row.get("z")
                == Some(&RdfTerm::Literal {
                    value: "1".into(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                    language: None,
                })
        })
        .unwrap();
    assert_eq!(
        numeric.get("sum"),
        Some(&RdfTerm::Literal {
            value: "2".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None
        })
    );
    let error = rows
        .iter()
        .find(|row| {
            row.get("z")
                == Some(&RdfTerm::Literal {
                    value: "foobar".into(),
                    datatype: None,
                    language: None,
                })
        })
        .unwrap();
    assert!(
        !error.contains_key("sum"),
        "projexp02 expression error 应使 ?sum 未绑定：{error:?}"
    );
}

#[test]
fn applies_ontop_project_expression_alias_reuse_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("projexp03.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp03.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp03.rq")).unwrap();
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("http://www.example.org/instance#a".into())), ("y".into(), integer("1")), ("z".into(), integer("2")), ("sum".into(), integer("3")), ("twice".into(), integer("6"))])]),
        "projexp03.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_project_expression_alias_order_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("projexp04.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp04.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp04.rq")).unwrap();
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[
            std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("http://www.example.org/instance#a".into())), ("y".into(), integer("1")), ("sum".into(), integer("2"))]),
            std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("http://www.example.org/instance#a".into())), ("y".into(), integer("2")), ("sum".into(), integer("4"))]),
        ]),
        "projexp04.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_project_expression_datatype_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("projexp05.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp05.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp05.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("projexp05.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 2, "projexp05.srx：{rows:?}");
    let integer_row = rows.iter().find(|row| matches!(row.get("l"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value == "1" && datatype == "http://www.w3.org/2001/XMLSchema#integer")).unwrap();
    assert_eq!(
        integer_row.get("dt"),
        Some(&RdfTerm::Iri(
            "http://www.w3.org/2001/XMLSchema#integer".into()
        ))
    );
    let iri_row = rows
        .iter()
        .find(|row| row.get("l") == Some(&RdfTerm::Iri("http://www.example.org/schema#a".into())))
        .unwrap();
    assert!(
        !iri_row.contains_key("dt"),
        "IRI 的 DATATYPE expression error 应保留行但不绑定 ?dt：{iri_row:?}"
    );
}

#[test]
fn applies_ontop_project_expression_unbound_datatype_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |name: &str, facts_contents: &str, query: &str| {
        let facts = dir.path().join(format!("{name}.ttl"));
        std::fs::write(&facts, facts_contents).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: Some("http://example.org/#".into()),
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        runtime
            .query(query)
            .unwrap_or_else(|error| panic!("{name} 查询失败：{error}\n{query}"))
    };
    let result = run(
        "projexp06",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp06.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp06.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("http://www.example.org/instance#a".into())), ("l".into(), RdfTerm::Literal { value: "1".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None })])]),
        "projexp06.srx：{result:?}"
    );
    let result = run(
        "projexp07",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp07.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/project-expression/projexp07.rq"),
    );
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("projexp07.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 2, "projexp07.srx：{rows:?}");
    let a = rows
        .iter()
        .find(|row| row.get("x") == Some(&RdfTerm::Iri("http://www.example.org/instance#a".into())))
        .unwrap();
    assert_eq!(
        a.get("dt"),
        Some(&RdfTerm::Iri(
            "http://www.w3.org/2001/XMLSchema#integer".into()
        ))
    );
    let b = rows
        .iter()
        .find(|row| row.get("x") == Some(&RdfTerm::Iri("http://www.example.org/instance#b".into())))
        .unwrap();
    assert!(
        !b.contains_key("dt"),
        "OPTIONAL 未绑定变量的 DATATYPE error 应保留 ?x=b：{b:?}"
    );
}

#[test]
fn applies_ontop_strdt_and_strlang_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("boolean-effective-value query 失败：{error}\n{query}"))
        else {
            panic!("STRDT/STRLANG manifest 应返回 bindings")
        };
        rows
    };
    let iri = |local: &str| RdfTerm::Iri(format!("http://example.org/{local}"));
    let xsd_string = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
        language: None,
    };
    for (asset, variable) in [("strdt01.rq", "str1"), ("strlang01.rq", "s2")] {
        let query = match asset {
            "strdt01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strdt01.rq"),
            "strlang01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strlang01.rq"),
            _ => unreachable!(),
        };
        let rows = run(query);
        assert_eq!(
            rows,
            vec![std::collections::BTreeMap::from([("s".into(), iri("s2"))])],
            "{asset}.srx：语言 literal 作为第一参数必须产生 expression error"
        );
        assert!(!rows[0].contains_key(variable));
    }
    assert_eq!(
        run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strdt02.rq")),
        vec![std::collections::BTreeMap::from([("s".into(), iri("s2")), ("str1".into(), xsd_string("bar"))])],
        "strdt02.srx"
    );
    assert_eq!(
        run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strlang02.rq")),
        vec![std::collections::BTreeMap::from([("s".into(), iri("s2")), ("s2".into(), RdfTerm::Literal { value: "bar".into(), datatype: None, language: Some("en-us".into()) })])],
        "strlang02.srx"
    );
    for (asset, variable) in [("strdt03.rq", "str1"), ("strlang03.rq", "str1")] {
        let query = match asset {
            "strdt03.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strdt03.rq"),
            "strlang03.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strlang03.rq"),
            _ => unreachable!(),
        };
        let rows = run(query);
        assert_eq!(rows.len(), 16, "{asset}.srx：应保留全部 source terms");
        assert_eq!(
            rows.iter().filter(|row| row.contains_key(variable)).count(),
            6,
            "{asset}.srx：仅六个 simple/xsd:string string rows 应绑定转换结果"
        );
        let expected = if asset == "strdt03.rq" {
            xsd_string("foo")
        } else {
            RdfTerm::Literal {
                value: "foo".into(),
                datatype: None,
                language: Some("en-us".into()),
            }
        };
        assert_eq!(
            rows.iter()
                .find(|row| row.get("s") == Some(&iri("s1")))
                .and_then(|row| row.get(variable)),
            Some(&expected),
            "{asset}.srx：simple literal 应成功转换"
        );
        for subject in ["n1", "s2", "d1"] {
            let row = rows
                .iter()
                .find(|row| row.get("s") == Some(&iri(subject)))
                .unwrap();
            assert!(
                !row.contains_key(variable),
                "{asset}.srx：{subject} 非 simple/xsd:string 输入应保持未绑定：{row:?}"
            );
        }
    }
}

#[test]
fn applies_ontop_datetime_day_hours_timezone_tz_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("boolean-effective-value query 失败：{error}\n{query}"))
        else {
            panic!("dateTime function manifest 应返回 bindings")
        };
        rows
    };
    let iri = |local: &str| RdfTerm::Iri(format!("http://example.org/{local}"));
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    for (asset, expected) in [
        ("day-01.rq", ["21", "21", "20", "1"]),
        ("hours-01.rq", ["11", "15", "23", "1"]),
    ] {
        let query = match asset {
            "day-01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/day-01.rq"),
            "hours-01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/hours-01.rq"),
            _ => unreachable!(),
        };
        assert_eq!(
            run(query),
            ["d1", "d2", "d3", "d4"]
                .into_iter()
                .zip(expected)
                .map(|(subject, value)| std::collections::BTreeMap::from([
                    ("s".into(), iri(subject)),
                    ("x".into(), integer(value))
                ]))
                .collect::<Vec<_>>(),
            "{asset}.srx"
        );
    }
    let timezone = run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/timezone-01.rq"));
    assert_eq!(timezone.len(), 4, "timezone-01.srx");
    for (subject, value) in [("d1", "PT0S"), ("d2", "-PT8H"), ("d3", "PT0S")] {
        assert_eq!(
            timezone
                .iter()
                .find(|row| row.get("s") == Some(&iri(subject)))
                .and_then(|row| row.get("x")),
            Some(&RdfTerm::Literal {
                value: value.into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#dayTimeDuration".into()),
                language: None
            }),
            "timezone-01.srx：{subject}"
        );
    }
    assert!(
        !timezone
            .iter()
            .find(|row| row.get("s") == Some(&iri("d4")))
            .unwrap()
            .contains_key("x"),
        "timezone-01.srx：无 offset 的 d4 应保留而不绑定"
    );
    assert_eq!(
        run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/tz-01.rq")),
        ["Z", "-08:00", "Z", ""]
            .into_iter()
            .zip(["d1", "d2", "d3", "d4"])
            .map(|(value, subject)| std::collections::BTreeMap::from([("s".into(), iri(subject)), ("x".into(), RdfTerm::Literal { value: value.into(), datatype: None, language: None })]))
            .collect::<Vec<_>>(),
        "tz-01.srx"
    );
}

#[test]
fn applies_ontop_positive_exists_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |name: &str, facts_contents: &str, query: &str| {
        let facts = dir.path().join(name);
        std::fs::write(&facts, facts_contents).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: Some("http://example.org/#".into()),
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        runtime
            .query(query)
            .unwrap_or_else(|error| panic!("{name} 查询失败：{error}\n{query}"))
    };
    let facts = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/exists/exists01.ttl");
    let iri = |local: &str| RdfTerm::Iri(format!("http://www.example.org/{local}"));
    for (asset, expected_subjects) in [
        ("exists01.rq", vec!["s", "s", "s"]),
        ("exists02.rq", vec!["s", "t"]),
        ("exists04.rq", vec!["s"]),
    ] {
        let query = match asset {
            "exists01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/exists/exists01.rq"),
            "exists02.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/exists/exists02.rq"),
            "exists04.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/exists/exists04.rq"),
            _ => unreachable!(),
        };
        let rtop::QueryResult::Bindings(rows) = run("exists01.ttl", facts, query) else {
            panic!("{asset}.srx 应返回 bindings")
        };
        assert_eq!(rows.len(), expected_subjects.len(), "{asset}.srx：{rows:?}");
        for subject in expected_subjects {
            assert!(
                rows.iter().any(|row| row.get("s") == Some(&iri(subject))),
                "{asset}.srx 缺少 ?s={subject}：{rows:?}"
            );
        }
    }
    assert!(
        matches!(
            run("exists01.ttl", facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/exists/exists05.rq")),
            rtop::QueryResult::Bindings(ref rows) if rows.is_empty()
        ),
        "exists05.srx 应为空"
    );
    let named_graph = "<http://www.example.org/a> <http://www.example.org/p> <http://www.example.org/o1> <https://example.test/exists02.ttl> .\n<http://www.example.org/b> <http://www.example.org/p> <http://www.example.org/o1> <https://example.test/exists02.ttl> .\n<http://www.example.org/b> <http://www.example.org/p> <http://www.example.org/o2> <https://example.test/exists02.ttl> .\n";
    let query = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/exists/exists03.rq")
        .replace("<exists02.ttl>", "<https://example.test/exists02.ttl>");
    assert!(
        matches!(run("exists03.nq", named_graph, &query), rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([("s".into(), iri("b")), ("p".into(), iri("p"))])]),
        "exists03.srx：named graph 中的关联 EXISTS 必须局限在同一 graph"
    );
}

#[test]
fn applies_ontop_subquery_aggregate_nested_and_exists_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |asset: &str, facts_contents: &str, query: &str| {
        let facts = dir.path().join(format!("{asset}.rdf"));
        std::fs::write(&facts, facts_contents).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        runtime.query(query).unwrap()
    };
    let result = run(
        "sq08",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq08.rdf"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq08.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([
            ("x".into(), RdfTerm::Iri("http://www.example.org/instance#b".into())),
            ("max".into(), RdfTerm::Literal { value: "3".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None }),
        ])]),
        "sq08.srx：{result:?}"
    );
    let result = run(
        "sq09",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq09.rdf"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq09.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([
            ("x".into(), RdfTerm::Iri("http://www.example.org/instance#a".into())),
            ("y".into(), RdfTerm::Iri("http://www.example.org/instance#b".into())),
        ])]),
        "sq09.srx：{result:?}"
    );
    let result = run(
        "sq10",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq10.rdf"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq10.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if rows.is_empty()),
        "sq10.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_subquery_limit_per_resource_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("sq11.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq11.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq11.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("sq11.srx 应返回 bindings")
    };
    let labels = rows
        .iter()
        .map(|row| match row.get("L") {
            Some(RdfTerm::Literal {
                value,
                datatype: None,
                language: None,
            }) => value.as_str(),
            other => panic!("sq11.srx 应为 simple literal label：{other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "Ice Cream",
            "Ice Cream",
            "Pasta",
            "Pizza",
            "Soft Drink",
            "Wine"
        ],
        "sq11.srx：子查询 ORDER BY ?O 后 LIMIT 2 只能选择 order1/order2"
    );
}

#[test]
fn applies_ontop_subquery_default_graph_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("sq05.rdf");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq05.rdf")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: Some(
                "http://www.w3.org/2009/sparql/docs/tests/data-sparql11/subquery/sq05.rdf".into(),
            ),
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq06.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("sq06.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 2, "sq06.srx：{rows:?}");
    for subject in ["a", "c"] {
        assert!(
            rows.iter().any(|row| row.get("x")
                == Some(&RdfTerm::Iri(format!(
                    "http://www.example.org/instance#{subject}"
                )))),
            "sq06.srx 缺少 x={subject}：{rows:?}"
        );
    }
}

#[test]
fn applies_ontop_subquery_manifest_named_graph_scope_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let base = "http://www.w3.org/2009/sparql/docs/tests/data-sparql11/subquery/";
    let run = |name: &str, query: &str| {
        let facts = dir.path().join(format!("{name}.nq"));
        std::fs::write(
            &facts,
            format!(
                "<http://www.example.org/instance#a> <http://www.example.org/schema#p> <http://www.example.org/instance#b> <{base}{name}.rdf> .\n<http://www.example.org/instance#c> <http://www.example.org/schema#p> <{base}{name}.rdf> <{base}{name}.rdf> .\n"
            ),
        )
        .unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) =
            runtime.query(&format!("BASE <{base}>\n{query}")).unwrap()
        else {
            panic!("subquery manifest action 必须返回 bindings")
        };
        rows
    };
    let iri = |local: &str| RdfTerm::Iri(format!("http://www.example.org/instance#{local}"));
    let predicate = RdfTerm::Iri("http://www.example.org/schema#p".into());

    let sq01 = run("sq01", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq01.rq"));
    assert_eq!(
        sq01.len(),
        2,
        "sq01.srx 的具名图子查询 result bag：{sq01:?}"
    );
    assert!(
        sq01.contains(&std::collections::BTreeMap::from([
            ("x".into(), iri("a")),
            ("p".into(), predicate.clone())
        ])) && sq01.contains(&std::collections::BTreeMap::from([
            ("x".into(), iri("c")),
            ("p".into(), predicate.clone())
        ])),
        "sq01.srx：{sq01:?}"
    );

    let sq02 = run("sq01", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq02.rq"));
    assert_eq!(
        sq02,
        vec![std::collections::BTreeMap::from([
            ("x".into(), iri("c")),
            ("p".into(), predicate.clone())
        ])],
        "sq02.srx：图变量约束必须保留 c/p"
    );

    for (asset, data, expected) in [
        ("sq03.rq", "sq01", vec![iri("a"), iri("c")]),
        ("sq04.rq", "sq01", vec![iri("a"), iri("c")]),
        ("sq05.rq", "sq05", vec![iri("a"), iri("c")]),
        ("sq07.rq", "sq05", vec![iri("a"), iri("c")]),
    ] {
        let query = match asset {
            "sq03.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq03.rq"),
            "sq04.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq04.rq"),
            "sq05.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq05.rq"),
            "sq07.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq07.rq"),
            _ => unreachable!("固定 subquery manifest asset"),
        };
        let rows = run(data, query);
        assert_eq!(rows.len(), 2, "{asset} result bag：{rows:?}");
        assert!(
            rows.contains(&std::collections::BTreeMap::from([(
                "x".into(),
                expected[0].clone()
            )])) && rows.contains(&std::collections::BTreeMap::from([(
                "x".into(),
                expected[1].clone()
            )])),
            "{asset} 必须匹配原始 SRX：{rows:?}"
        );
    }
}

#[test]
fn applies_ontop_construct_subquery_projection_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("sq12.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq12.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq12.rq")).unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Graph(ref graph)
        if graph == &[rtop::RdfFact {
            subject: RdfTerm::Iri("http://p1".into()),
            predicate: "http://xmlns.com/foaf/0.1/name".into(),
            object: RdfTerm::Literal { value: "John Doe".into(), datatype: None, language: None },
            graph: None,
        }]),
        "sq12_out.ttl：{result:?}"
    );
}

#[test]
fn applies_ontop_construct_subquery_limit_optional_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("sq14.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq14.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq14.rq")).unwrap();
    let rtop::QueryResult::Graph(graph) = result else {
        panic!("sq14-out.ttl 应返回 graph")
    };
    let iri = |value: &str| RdfTerm::Iri(value.into());
    let literal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    let fact = |subject: &str, predicate: &str, object: RdfTerm| rtop::RdfFact {
        subject: iri(&format!("http://example.org/ns#{subject}")),
        predicate: format!("http://xmlns.com/foaf/0.1/{predicate}"),
        object,
        graph: None,
    };
    assert_eq!(graph.len(), 11, "sq14-out.ttl：{graph:?}");
    for (subject, name, homepage, mbox) in [
        ("a", "Alan", vec!["http://example.org/alan"], None),
        (
            "b",
            "Ben",
            vec!["http://example.org/ben", "http://example.com/ben"],
            Some("mailto:ben@example.org"),
        ),
        ("c", "Chris", vec!["http://example.org/chris"], None),
    ] {
        assert!(
            graph.contains(&rtop::RdfFact {
                subject: iri(&format!("http://example.org/ns#{subject}")),
                predicate: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".into(),
                object: iri("http://xmlns.com/foaf/0.1/Person"),
                graph: None,
            }),
            "sq14-out.ttl 缺少 {subject} type"
        );
        assert!(
            graph.contains(&fact(subject, "name", literal(name))),
            "sq14-out.ttl 缺少 {subject} name"
        );
        for homepage in homepage {
            assert!(
                graph.contains(&fact(subject, "homepage", iri(homepage))),
                "sq14-out.ttl 缺少 {subject} homepage={homepage}"
            );
        }
        if let Some(mbox) = mbox {
            assert!(
                graph.contains(&fact(subject, "mbox", iri(mbox))),
                "sq14-out.ttl 缺少 {subject} mbox"
            );
        }
    }
}

#[test]
fn applies_ontop_subquery_does_not_inject_outer_bindings_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("sq13.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq13.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/subquery/sq13.rq")).unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("sq13.srx 应返回 bindings")
    };
    assert_eq!(
        rows.len(),
        16,
        "sq13.srx：子查询不应接收 outer ?L binding：{rows:?}"
    );
    for left in 1..=4 {
        for right in 1..=4 {
            assert!(
                rows.contains(&std::collections::BTreeMap::from([
                    (
                        "O1".into(),
                        RdfTerm::Iri(format!("http://www.example.orgorder{left}"))
                    ),
                    (
                        "O2".into(),
                        RdfTerm::Iri(format!("http://www.example.orgorder{right}"))
                    ),
                ])),
                "sq13.srx 缺少 order{left}/order{right} 笛卡尔对：{rows:?}"
            );
        }
    }
}

#[test]
fn applies_ontop_sparql11_subquery_manifest_default_graph_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |name: &str, facts_contents: &str, query: &str| {
        let facts = dir.path().join(format!("{name}.ttl"));
        std::fs::write(&facts, facts_contents).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        runtime
            .query(query)
            .unwrap_or_else(|error| panic!("{name} 查询失败：{error}\n{query}"))
    };
    let iri = |local: &str| RdfTerm::Iri(format!("http://example/{local}"));
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };

    let result = run(
        "data-01",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/data-01.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/sparql11-subquery-01.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([
            ("y".into(), iri("bob")), ("ageNextYear".into(), integer("43")),
        ])]),
        "sparql11-subquery-01.srx：{result:?}"
    );

    let result = run(
        "data-02",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/data-02.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/sparql11-subquery-02.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows.len() == 3 && rows.iter().any(|row| row.get("s") == Some(&iri("a")) && row.get("p") == Some(&iri("p")) && row.get("o") == Some(&iri("b")))
        && rows.iter().any(|row| row.get("s") == Some(&iri("b")) && row.get("p") == Some(&iri("p")) && row.get("o") == Some(&iri("c")))
        && rows.iter().any(|row| row.get("s") == Some(&iri("b")) && row.get("p") == Some(&iri("q")) && row.get("o") == Some(&iri("d")))),
        "sparql11-subquery-02.srx：{result:?}"
    );

    let data_03 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/data-03.ttl");
    let result = run(
        "data-03-empty",
        data_03,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/sparql11-subquery-03.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if rows.is_empty()),
        "sparql11-subquery-03.srx：{result:?}"
    );
    let result = run(
        "data-03-limit",
        data_03,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/sparql11-subquery-04.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[
            std::collections::BTreeMap::from([("friend".into(), RdfTerm::Iri("http://example.org/ringo".into()))]),
            std::collections::BTreeMap::from([("friend".into(), RdfTerm::Iri("http://example.org/john".into()))]),
        ]),
        "sparql11-subquery-04.srx：{result:?}"
    );

    let result = run(
        "data-04-union",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/data-04.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/sparql11-subquery-05.rq"),
    );
    let a0 = RdfTerm::Iri("http://example.org/a0".into());
    let simple_literal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows.len() == 3
        && rows.contains(&std::collections::BTreeMap::from([("s".into(), a0.clone())]))
        && rows.contains(&std::collections::BTreeMap::from([
            ("s".into(), a0.clone()),
            ("p".into(), RdfTerm::Iri("http://example.org/p0".into())),
            ("o".into(), simple_literal("a0+p0")),
        ]))
        && rows.contains(&std::collections::BTreeMap::from([
            ("s".into(), a0),
            ("p".into(), RdfTerm::Iri("http://example.org/p1".into())),
            ("o".into(), simple_literal("a0+p1")),
        ]))),
        "sparql11-subquery-05.srx：{result:?}"
    );

    let result = run(
        "data-01-product",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/data-01.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-sparql-1.1/subquery/sparql11-subquery-06.rq"),
    );
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows.len() == 4 && rows.iter().all(|row| row == &std::collections::BTreeMap::from([("s".into(), iri("alice")), ("o".into(), iri("alice"))]))),
        "sparql11-subquery-06.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_dawg_type_promotion_numeric_root_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("tP.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP.ttl"),
    )
    .unwrap();
    for (asset, query) in [
        ("tP-double-double.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-double-double.rq")),
        ("tP-double-float.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-double-float.rq")),
        ("tP-double-decimal.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-double-decimal.rq")),
        ("tP-float-float.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-float-float.rq")),
        ("tP-float-decimal.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-float-decimal.rq")),
        ("tP-decimal-decimal.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-decimal-decimal.rq")),
        ("tP-integer-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-integer-short.rq")),
        ("tP-nonPositiveInteger-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-nonPositiveInteger-short.rq")),
        ("tP-negativeInteger-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-negativeInteger-short.rq")),
        ("tP-long-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-long-short.rq")),
        ("tP-int-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-int-short.rq")),
        ("tP-short-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-short.rq")),
        ("tP-byte-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-byte-short.rq")),
        ("tP-nonNegativeInteger-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-nonNegativeInteger-short.rq")),
        ("tP-unsignedLong-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-unsignedLong-short.rq")),
        ("tP-unsignedInt-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-unsignedInt-short.rq")),
        ("tP-unsignedShort-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-unsignedShort-short.rq")),
        ("tP-unsignedByte-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-unsignedByte-short.rq")),
        ("tP-positiveInteger-short.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-positiveInteger-short.rq")),
        ("tP-short-double.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-double.rq")),
        ("tP-short-float.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-float.rq")),
        ("tP-short-decimal.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-decimal.rq")),
    ] {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        assert_eq!(
            runtime.query(query).unwrap(),
            rtop::QueryResult::Boolean(true),
            "固定 DAWG {asset} 应匹配 true.ttl"
        );
    }
    for (asset, query) in [
        ("tP-byte-short-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-byte-short-fail.rq")),
        ("tP-short-short-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-short-fail.rq")),
        ("tP-short-int-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-int-fail.rq")),
        ("tP-short-byte-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-byte-fail.rq")),
        ("tP-short-long-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-short-long-fail.rq")),
        ("tP-double-float-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-double-float-fail.rq")),
        ("tP-double-decimal-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-double-decimal-fail.rq")),
        ("tP-float-decimal-fail.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/type-promotion/tP-float-decimal-fail.rq")),
    ] {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        assert_eq!(
            runtime.query(query).unwrap(),
            rtop::QueryResult::Boolean(false),
            "固定 DAWG {asset} 应匹配 false.ttl"
        );
    }
}

#[test]
fn applies_ontop_grouping_manifest_simple_and_unbound_groups() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n",
    )
    .unwrap();
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    let run = |facts_name: &str, facts_contents: &str, query: &str| {
        let facts = dir.path().join(facts_name);
        std::fs::write(&facts, facts_contents).unwrap();
        VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap()
        .query(query)
        .unwrap()
    };

    let simple = run(
        "group-data-1.ttl",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group-data-1.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group01.rq"),
    );
    assert!(
        matches!(simple, rtop::QueryResult::Bindings(ref rows)
        if rows == &[
            std::collections::BTreeMap::from([("s".into(), RdfTerm::Iri("http://example/s1".into()))]),
            std::collections::BTreeMap::from([("s".into(), RdfTerm::Iri("http://example/s2".into()))]),
        ]),
        "group01.srx：{simple:?}"
    );

    let unbound = run(
        "group-data-1-unbound.ttl",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group-data-1.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group03.rq"),
    );
    assert!(
        matches!(unbound, rtop::QueryResult::Bindings(ref rows)
        if rows == &[
            std::collections::BTreeMap::from([("w".into(), integer("9")), ("S".into(), integer("1"))]),
            std::collections::BTreeMap::from([("S".into(), integer("2"))]),
        ]),
        "group03.srx：{unbound:?}"
    );

    let expression = run(
        "group-data-1-expression.ttl",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group-data-1.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group04.rq"),
    );
    let date = RdfTerm::Literal {
        value: "1605-11-05".into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#date".into()),
        language: None,
    };
    let rtop::QueryResult::Bindings(rows) = expression else {
        panic!("group04.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 2, "group04.srx：{rows:?}");
    assert!(
        rows.contains(&std::collections::BTreeMap::from([
            ("X".into(), date),
            ("S".into(), integer("2")),
        ])) && rows.contains(&std::collections::BTreeMap::from([
            ("X".into(), integer("9")),
            ("S".into(), integer("1")),
        ])),
        "group04.srx result bag：{rows:?}"
    );

    let multi = run(
        "group-data-2.ttl",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group-data-2.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/grouping/group05.rq"),
    );
    let rtop::QueryResult::Bindings(rows) = multi else {
        panic!("group05.srx 应返回 bindings")
    };
    assert_eq!(rows.len(), 3, "group05.srx：{rows:?}");
    assert!(
        rows.contains(&std::collections::BTreeMap::from([
            ("s".into(), RdfTerm::Iri("http://example/s1".into())),
            ("w".into(), integer("9")),
        ])) && rows.contains(&std::collections::BTreeMap::from([(
            "s".into(),
            RdfTerm::Iri("http://example/s2".into()),
        )])) && rows.contains(&std::collections::BTreeMap::from([(
            "s".into(),
            RdfTerm::Iri("http://example/s3".into()),
        )])),
        "group05.srx result bag：{rows:?}"
    );
}

#[test]
fn applies_ontop_group_by_expression_manifest_result_bag() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg08.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg08.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg08b.rq"
        ))
        .unwrap()
    else {
        panic!("agg08b 结果必须是 bindings")
    };
    let integer = |value: i32| RdfTerm::Literal {
        value: value.to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    assert_eq!(rows.len(), 5, "{rows:?}");
    for (index, (sum, count)) in [(0, 1), (1, 2), (2, 3), (3, 2), (4, 1)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            rows[index].get("O12"),
            Some(&integer(sum)),
            "agg08b.srx group {sum}: {rows:?}"
        );
        assert_eq!(
            rows[index].get("C"),
            Some(&integer(count)),
            "agg08b.srx group {sum}: {rows:?}"
        );
    }
}

#[test]
fn applies_ontop_empty_group_manifest_as_an_empty_result_bag() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("empty.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/empty.ttl"
        ),
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
    let result = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-empty-group.rq"
        ))
        .unwrap();
    // 修订后的 `agg-empty-group.srx` 明确没有 <result>：带 grouping variable 的
    // 空输入没有 group，不能产生一个 ?max 未绑定的 synthetic row。
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if rows.is_empty()),
        "agg-empty-group.srx 应为空：{result:?}"
    );
}

#[test]
fn applies_ontop_aggregate_error_manifest_without_dropping_its_group() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-err-01.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-err-01.ttl"
        ),
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
    let result = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-err-01.rq"
        ))
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("aggregate 结果必须是 bindings")
    };
    assert_eq!(rows.len(), 3, "{rows:?}");
    let decimal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    };
    for group in ["x", "z"] {
        assert!(
            rows.iter().any(|row| {
                row.get("g") == Some(&RdfTerm::Iri(format!("http://example.com/data/#{group}")))
                    && row.get("avg") == Some(&decimal("2.5"))
                    && row.get("c") == Some(&decimal("2.5"))
            }),
            "agg-err-01.srx 缺少 {group} 的完整聚合 binding：{rows:?}"
        );
    }
    assert!(
        rows.iter().any(|row| {
            row.get("g") == Some(&RdfTerm::Iri("http://example.com/data/#y".into()))
                && !row.contains_key("avg")
                && !row.contains_key("c")
        }),
        "aggregate error 不得丢弃 y group：{rows:?}"
    );
}

#[test]
fn applies_ontop_sum_grouped_numeric_manifest_result_bag() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-numeric2.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-numeric2.ttl"
        ),
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
    let result = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-sum-02.rq"
        ))
        .unwrap();
    let rtop::QueryResult::Bindings(rows) = result else {
        panic!("SUM 结果必须是 bindings")
    };
    assert_eq!(rows.len(), 5, "{rows:?}");
    let expected = [
        ("ints", "6", "integer"),
        ("decimals", "6.7", "decimal"),
        ("doubles", "3.21E4", "double"),
        ("mixed1", "3.2", "decimal"),
        ("mixed2", "4.0E-1", "double"),
    ];
    for (subject, value, datatype) in expected {
        let datatype = format!("http://www.w3.org/2001/XMLSchema#{datatype}");
        assert!(
            rows.iter().any(|row| {
                row.get("s") == Some(&RdfTerm::Iri(format!("http://www.example.org/{subject}")))
                    && row.get("sum")
                        == Some(&RdfTerm::Literal {
                            value: value.into(),
                            datatype: Some(datatype.clone()),
                            language: None,
                        })
            }),
            "agg-sum-02.srx 缺少 {subject}: {rows:?}"
        );
    }
}

#[test]
fn applies_ontop_sum_ungrouped_numeric_manifest_result() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-numeric.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-numeric.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert_eq!(
        runtime
            .query(include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-sum-01.rq"
            ))
            .unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::from([(
            "sum".into(),
            RdfTerm::Literal {
                value: "11.1".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
                language: None,
            },
        )])]),
        "agg-sum-01.srx"
    );
}

#[test]
fn applies_ontop_sum_order_manifest_result_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-sum-order-01.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-sum-order-01.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-sum-order-01.rq"
        ))
        .unwrap()
    else {
        panic!("agg-sum-order-01.srx 必须返回 bindings")
    };
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    let decimal = RdfTerm::Literal {
        value: "7.1".into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    };
    assert_eq!(
        rows,
        vec![
            std::collections::BTreeMap::from([
                ("org".into(), RdfTerm::Iri("http://example/org-2".into())),
                ("totalPrice".into(), integer("100")),
            ]),
            std::collections::BTreeMap::from([
                ("org".into(), RdfTerm::Iri("http://example/org-3".into())),
                ("totalPrice".into(), decimal),
            ]),
            std::collections::BTreeMap::from([
                ("org".into(), RdfTerm::Iri("http://example/org-1".into())),
                ("totalPrice".into(), integer("2")),
            ]),
        ],
        "agg-sum-order-01-modified.srx：SUM 后必须按 DESC(?totalPrice) 排序"
    );
}

#[test]
fn applies_ontop_avg_numeric_manifest_result_bags() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-numeric2.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-numeric2.ttl"
        ),
    )
    .unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping.clone(),
        facts_file: Some(facts.clone()),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-avg-02.rq"
        ))
        .unwrap()
    else {
        panic!("AVG 结果必须是 bindings")
    };
    assert_eq!(rows.len(), 3, "{rows:?}");
    for (subject, value, datatype) in [
        ("mixed1", "1.6", "decimal"),
        ("mixed2", "2.0E-1", "double"),
        ("ints", "2.0", "decimal"),
    ] {
        assert!(
            rows.iter().any(|row| {
                row.get("s") == Some(&RdfTerm::Iri(format!("http://www.example.org/{subject}")))
                    && row.get("avg")
                        == Some(&RdfTerm::Literal {
                            value: value.into(),
                            datatype: Some(format!("http://www.w3.org/2001/XMLSchema#{datatype}")),
                            language: None,
                        })
            }),
            "agg-avg-02.srx 缺少 {subject}: {rows:?}"
        );
    }

    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-numeric.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-avg-01.rq"
        ))
        .unwrap()
    else {
        panic!("AVG 结果必须是 bindings")
    };
    assert_eq!(rows.len(), 1, "agg-avg-01.srx: {rows:?}");
    assert_eq!(
        rows[0].get("avg"),
        Some(&RdfTerm::Literal {
            value: "2.22".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
            language: None,
        }),
        "agg-avg-01.srx"
    );
}

#[test]
fn applies_ontop_min_max_numeric_manifest_result_bags() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-numeric.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-numeric.ttl"
        ),
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
    let literal = |value: &str, datatype: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some(format!("http://www.w3.org/2001/XMLSchema#{datatype}")),
        language: None,
    };
    for (query, variable, expected) in [
        (
            include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-min-01.rq"
            ),
            "min",
            literal("1.0", "decimal"),
        ),
        (
            include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-max-01.rq"
            ),
            "max",
            literal("3.0E4", "double"),
        ),
    ] {
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("MIN/MAX 结果必须是 bindings")
        };
        assert_eq!(rows.len(), 1, "{query}: {rows:?}");
        assert_eq!(rows[0].get(variable), Some(&expected), "{query}: {rows:?}");
    }
    for (query, variable, expected) in [
        (
            include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-min-02.rq"
            ),
            "min",
            [
                ("ints", literal("1", "integer")),
                ("decimals", literal("1.0", "decimal")),
                ("doubles", literal("1.0E2", "double")),
                ("mixed1", literal("1", "integer")),
                ("mixed2", literal("2.0E-1", "double")),
            ],
        ),
        (
            include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-max-02.rq"
            ),
            "max",
            [
                ("ints", literal("3", "integer")),
                ("decimals", literal("3.5", "decimal")),
                ("doubles", literal("3.0E4", "double")),
                ("mixed1", literal("2.2", "decimal")),
                ("mixed2", literal("2.2", "decimal")),
            ],
        ),
    ] {
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("MIN/MAX 分组结果必须是 bindings")
        };
        assert_eq!(rows.len(), 5, "{query}: {rows:?}");
        for (subject, expected) in expected {
            assert!(
                rows.iter().any(|row| {
                    row.get("s")
                        == Some(&RdfTerm::Iri(format!("http://www.example.org/{subject}")))
                        && row.get(variable) == Some(&expected)
                }),
                "{query} 缺少 {subject}: {rows:?}"
            );
        }
    }
}

#[test]
fn applies_ontop_sample_manifest_ask() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-numeric.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-numeric.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert_eq!(
        runtime
            .query(include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-sample-01.rq"
            ))
            .unwrap(),
        rtop::QueryResult::Boolean(true),
        "agg-sample-01.srx"
    );
}

#[test]
fn applies_ontop_aggregate_error_coalesce_manifest_result_bag() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("agg-err-02.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-err-02.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/aggregates/agg-err-02.rq"
        ))
        .unwrap()
    else {
        panic!("agg-err-02 结果必须是 bindings")
    };
    assert_eq!(rows.len(), 3, "{rows:?}");
    for (group, value, datatype) in [
        ("x", "2.5E0", "double"),
        ("y", "2.0", "decimal"),
        ("z", "2.5E0", "double"),
    ] {
        assert!(
            rows.iter().any(|row| {
                row.get("g") == Some(&RdfTerm::Iri(format!("http://example.com/data/#{group}")))
                    && row.get("avg")
                        == Some(&RdfTerm::Literal {
                            value: value.into(),
                            datatype: Some(format!("http://www.w3.org/2001/XMLSchema#{datatype}")),
                            language: None,
                        })
            }),
            "agg-err-02.srx 缺少 {group}: {rows:?}"
        );
    }
}

#[test]
fn keeps_values_tuple_undef_unbound_at_runtime() {
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
        .query("SELECT ?left ?right WHERE { } VALUES (?left ?right) { (\"Alan\" UNDEF) (UNDEF \"Rachel\") }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![
            std::collections::BTreeMap::from([("left".into(), RdfTerm::Literal { value: "Alan".into(), datatype: None, language: None })]),
            std::collections::BTreeMap::from([("right".into(), RdfTerm::Literal { value: "Rachel".into(), datatype: None, language: None })]),
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
fn preserves_an_empty_solution_mapping_when_all_bind_casts_error() {
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
        .query("PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\nSELECT ?decimal ?float WHERE { BIND(xsd:decimal(\"not-a-number\"^^xsd:string) AS ?decimal) BIND(xsd:float(\"not-a-number\"^^xsd:string) AS ?float) }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::new()])
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
fn evaluates_datatype_as_an_iri_including_rdf_lang_string() {
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
            std::collections::BTreeMap::from([("type".into(), RdfTerm::Iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into())), ("value".into(), RdfTerm::Literal { value: "english".into(), datatype: None, language: Some("en".into()) })]),
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
fn preserves_ontop_language_literal_comparison_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\nmappingId left\ntarget <https://example.test/book> <https://example.test/left> \"{title}\"@en .\nsource SELECT title FROM left_titles\n\nmappingId right\ntarget <https://example.test/book> <https://example.test/right> \"{title}\"@EN .\nsource SELECT title FROM right_titles\n").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: None,
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, LanguagePairSource).unwrap();
    let result = runtime.query("SELECT ?equal ?not_equal ?string_not_equal ?same WHERE { <https://example.test/book> <https://example.test/left> ?left . <https://example.test/book> <https://example.test/right> ?right . BIND(?left = ?right AS ?equal) BIND(?left != ?right AS ?not_equal) BIND(STR(?left) != STR(?right) AS ?string_not_equal) BIND(sameTerm(?left, ?right) AS ?same) }").unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("equal".into(), RdfTerm::Literal { value: "true".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }),
            ("not_equal".into(), RdfTerm::Literal { value: "false".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }),
            ("string_not_equal".into(), RdfTerm::Literal { value: "true".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }),
            ("same".into(), RdfTerm::Literal { value: "false".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None }),
        ])])
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
fn groups_non_aggregate_solutions_before_offset_and_limit() {
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
        .query(
            "SELECT ?group WHERE { \
             VALUES ?group { <https://example.test/a> <https://example.test/a> <https://example.test/b> } \
             } GROUP BY ?group ORDER BY ?group OFFSET 1 LIMIT 1",
        )
        .unwrap();
    assert!(matches!(
        result,
        rtop::QueryResult::Bindings(rows)
            if rows == vec![std::collections::BTreeMap::from([(
                "group".into(),
                RdfTerm::Iri("https://example.test/b".into()),
            )])]
    ));
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

    let correlated = runtime.query("PREFIX ex: <https://example.test/>\nSELECT ?subject ?value { ?subject ex:value ?value FILTER NOT EXISTS { ?subject ex:other ?other . FILTER(?value = ?other) } }");
    assert!(
        matches!(correlated, Err(RuntimeError::NotFullyTranslatable(message)) if message.contains("EXISTS subquery"))
    );
}

#[test]
fn applies_ontop_property_path_named_graph_star_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("property-path.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let base = "http://www.w3.org/2009/sparql/docs/tests/data-sparql11/property-path/";
    let mut quads = String::new();
    for (name, asset) in [
        ("ng-01.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/ng-01.ttl")),
        ("ng-02.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/ng-02.ttl")),
        ("ng-03.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/ng-03.ttl")),
    ] {
        let triple = asset
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with(':'))
            .expect("固定 DAWG ng asset 含一个 :a :p1 :target triple");
        let terms = triple.split_whitespace().collect::<Vec<_>>();
        assert_eq!(terms.len(), 4, "固定 DAWG ng asset 应为一个完整 triple");
        let iri = |term: &str| format!("<http://www.example.org/{}>", term.trim_start_matches(':'));
        quads.push_str(&format!("{} {} {} <{base}{name}> .\n", iri(terms[0]), iri(terms[1]), iri(terms[2])));
    }
    std::fs::write(&facts, quads).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let expected = vec![
        RdfTerm::Iri("http://www.example.org/a".into()),
        RdfTerm::Iri("http://www.example.org/b".into()),
        RdfTerm::Iri("http://www.example.org/b".into()),
    ];
    for query in [
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-ng-01.rq"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-ng-02.rq"),
    ] {
        let result = runtime.query(&format!("BASE <{base}>\n{query}")).unwrap();
        let rtop::QueryResult::Bindings(rows) = result else {
            panic!("path-ng 查询必须返回 bindings")
        };
        assert_eq!(
            rows.iter().map(|row| row["t"].clone()).collect::<Vec<_>>(),
            expected,
            "path-ng-01.srx 的 result bag"
        );
    }
}

#[test]
fn applies_ontop_property_path_star_identity_and_cycle_assets() {
    let base = "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/";
    let query = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp14.rq");
    for (asset, expected_pairs) in [
        (
            "pp14.ttl",
            vec![
                ("a", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "b"),
                ("b", "c"),
                ("c", "c"),
            ],
        ),
        (
            "pp16.ttl",
            vec![
                ("a", "a"),
                ("a", "b"),
                ("a", "c"),
                ("b", "b"),
                ("b", "c"),
                ("c", "c"),
                ("d", "d"),
                ("d", "e"),
                ("d", "f"),
                ("e", "e"),
                ("e", "f"),
                ("f", "e"),
                ("f", "f"),
                ("h", "h"),
                ("test", "test"),
            ],
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mapping = dir.path().join("x.obda");
        let facts = dir.path().join(asset);
        std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
        let source = match asset {
            "pp14.ttl" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp14.ttl"),
            "pp16.ttl" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp16.ttl"),
            _ => unreachable!("固定 asset 列表"),
        };
        std::fs::write(&facts, source).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping,
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("{asset} 的 property path 必须返回 bindings")
        };
        let actual = rows
            .iter()
            .map(|row| (row["X"].clone(), row["Y"].clone()))
            .collect::<Vec<_>>();
        let expected = expected_pairs
            .into_iter()
            .map(|(left, right)| {
                let term = |value: &str| match value {
                    "test" => RdfTerm::Literal {
                        value: value.into(),
                        datatype: None,
                        language: None,
                    },
                    _ => RdfTerm::Iri(format!("http://example.org/{value}")),
                };
                (term(left), term(right))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "{base}{asset} 必须匹配对应 SRX result bag"
        );
    }
}

#[test]
fn applies_ontop_pp02_sequence_star_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp01.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp01.ttl"),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert_eq!(
        runtime
            .query(include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp02.rq"
            ))
            .unwrap(),
        rtop::QueryResult::Bindings(
            ["a", "c"]
                .into_iter()
                .map(|local| std::collections::BTreeMap::from([(
                    "x".into(),
                    RdfTerm::Iri(format!("http://www.example.org/instance#{local}")),
                )]))
                .collect(),
        ),
        "固定 Ontop pp02.srx：sequence 的一次关系闭包只返回 in:a 与 in:c"
    );
}

#[test]
fn scopes_sequence_star_paths_to_dataset_and_named_graphs() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("paths.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        "<https://example.test/a> <https://example.test/p1> <https://example.test/b> <https://example.test/g1> .\n<https://example.test/b> <https://example.test/p2> <https://example.test/a> <https://example.test/g2> .\n<https://example.test/a> <https://example.test/p3> <https://example.test/c> <https://example.test/g2> .\n<https://example.test/a> <https://example.test/p1> <https://example.test/b> <https://example.test/g3> .\n<https://example.test/b> <https://example.test/p2> <https://example.test/a> <https://example.test/g3> .\n<https://example.test/a> <https://example.test/p3> <https://example.test/c> <https://example.test/g3> .\n",
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let c = RdfTerm::Iri("https://example.test/c".into());
    assert_eq!(
        runtime.query("BASE <https://example.test/>\nSELECT ?x FROM <g1> FROM <g2> { <a> (<p1>/<p2>/<p3>)* ?x }").unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("https://example.test/a".into()))]), std::collections::BTreeMap::from([("x".into(), c.clone())])]),
        "两个 FROM 默认图必须在 sequence 一次关系和闭包前合并"
    );
    assert_eq!(
        runtime.query("SELECT * FROM <https://example.test/g1> { <https://example.test/a> <https://example.test/p1>* ?x }").unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("https://example.test/a".into()))]), std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("https://example.test/b".into()))])]),
        "dataset 也必须作用于既有单 IRI star，且 SELECT * 不泄漏内部变量"
    );
    assert_eq!(
        runtime.query("SELECT ?x { GRAPH <https://example.test/g1> { <https://example.test/a> (<https://example.test/p1>/<https://example.test/p2>/<https://example.test/p3>)* ?x } }").unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::from([("x".into(), RdfTerm::Iri("https://example.test/a".into()))])]),
        "固定具名图不能跨 g1/g2 拼接 sequence"
    );
    assert_eq!(
        runtime.query("SELECT ?g ?x FROM NAMED <https://example.test/g1> FROM NAMED <https://example.test/g2> { GRAPH ?g { <https://example.test/a> (<https://example.test/p1>/<https://example.test/p2>/<https://example.test/p3>)* ?x } }").unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::from([("g".into(), RdfTerm::Iri("https://example.test/g1".into())), ("x".into(), RdfTerm::Iri("https://example.test/a".into()))]), std::collections::BTreeMap::from([("g".into(), RdfTerm::Iri("https://example.test/g2".into())), ("x".into(), RdfTerm::Iri("https://example.test/a".into()))])]),
        "FROM NAMED 只暴露声明图，且 graph variable 必须绑定"
    );
}

#[test]
fn applies_ontop_property_path_negated_predicate_set_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp10.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp10.ttl"),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp10.rq"))
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if *rows == vec![std::collections::BTreeMap::from([(
            "x".into(), RdfTerm::Iri("http://www.example.org/instance#d".into())
        )])]),
        "pp10.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_property_path_inverse_sequence_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp09.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp09.ttl"),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp09.rq"))
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows) if *rows == vec![std::collections::BTreeMap::from([(
            "x".into(), RdfTerm::Iri("http://www.example.org/instance#a".into())
        )])]),
        "pp09.srx：{result:?}"
    );
}

#[test]
fn applies_ontop_property_path_single_named_graph_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp07.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let graph = "http://www.w3.org/2009/sparql/docs/tests/data-sparql11/property-path/pp07.ttl";
    std::fs::write(
        &facts,
        format!(
            "<http://www.example.org/instance#a> <http://www.example.org/schema#p1> <http://www.example.org/instance#b> <{graph}> .\n<http://www.example.org/instance#b> <http://www.example.org/schema#p2> <http://www.example.org/instance#c> <{graph}> .\n"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert_eq!(
        runtime
            .query(include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp06.rq"
            ))
            .unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::from([(
            "x".into(),
            RdfTerm::Iri("http://www.example.org/instance#c".into()),
        )])]),
        "pp07.srx：同一具名图内的 sequence path 必须匹配 c"
    );
}

#[test]
fn applies_ontop_property_path_bound_star_endpoints_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("clique3.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/clique3.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert_eq!(
        runtime
            .query(include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp36.rq"
            ))
            .unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::new()]),
        "pp36.srx：clique3 中 :a0 经 :p* 可达绑定端点 :a1"
    );
}

#[test]
fn applies_ontop_property_path_nested_star_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp37.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp37.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp37.rq"
        ))
        .unwrap()
    else {
        panic!("pp37.srx 必须返回 bindings")
    };
    assert_eq!(
        rows,
        ["A0", "A1", "A2"]
            .into_iter()
            .map(|local| {
                std::collections::BTreeMap::from([(
                    "X".into(),
                    RdfTerm::Iri(format!("http://example.org/{local}")),
                )])
            })
            .collect::<Vec<_>>(),
        "pp37.srx：`((:P)*)*` 必须保持 :A0 的三个可达节点及 ORDER BY 序列"
    );
}

#[test]
fn applies_ontop_property_path_zero_length_literal_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp13.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp13.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp13.rq"
        ))
        .unwrap()
    else {
        panic!("pp13.srx 必须返回 bindings")
    };
    let literal = RdfTerm::Literal {
        value: "o".into(),
        datatype: None,
        language: None,
    };
    assert_eq!(
        rows,
        vec![
            std::collections::BTreeMap::from([
                ("X".into(), RdfTerm::Iri("http://ex.org/s".into())),
                ("Y".into(), RdfTerm::Iri("http://ex.org/s".into())),
            ]),
            std::collections::BTreeMap::from([
                ("X".into(), literal.clone()),
                ("Y".into(), literal),
            ]),
        ],
        "pp13.srx：`p{{0}}` 必须产生所有 RDF term 的 identity mapping"
    );
}

#[test]
fn applies_ontop_property_path_zero_length_empty_graph_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("empty.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!(
            "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/empty.ttl"
        ),
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert_eq!(
        runtime
            .query(include_str!(
                "../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp15.rq"
            ))
            .unwrap(),
        rtop::QueryResult::Bindings(vec![std::collections::BTreeMap::from([
            (
                "X".into(),
                RdfTerm::Literal {
                    value: "o".into(),
                    datatype: None,
                    language: None,
                },
            ),
            ("Y".into(), RdfTerm::Iri("http://www.example.org/o".into())),
            ("Z".into(), RdfTerm::Iri("http://www.example.org/s".into())),
        ])]),
        "pp15.srx：空图中的 `{{0}}` 仍须把各常量端点绑定为 identity"
    );
}

#[test]
fn applies_ontop_property_path_operator_precedence_assets() {
    for (query_asset, facts_asset, expected) in [
        ("path-p1.rq", "path-p1.ttl", vec!["b", "c", "e"]),
        ("path-p2.rq", "path-p1.ttl", vec!["c", "c"]),
        ("path-p3.rq", "path-p3.ttl", vec!["c", "e", "b"]),
        ("path-p4.rq", "path-p3.ttl", vec!["e", "f", "b"]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mapping = dir.path().join("x.obda");
        let facts = dir.path().join(facts_asset);
        std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
        let fact_source = match facts_asset {
            "path-p1.ttl" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-p1.ttl"),
            "path-p3.ttl" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-p3.ttl"),
            _ => unreachable!("固定 property-path facts asset"),
        };
        let query = match query_asset {
            "path-p1.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-p1.rq"),
            "path-p2.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-p2.rq"),
            "path-p3.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-p3.rq"),
            "path-p4.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/path-p4.rq"),
            _ => unreachable!("固定 property-path query asset"),
        };
        std::fs::write(&facts, fact_source).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping,
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("{query_asset} 必须返回 bindings")
        };
        let mut actual = rows.iter().map(|row| row["t"].clone()).collect::<Vec<_>>();
        let mut expected = expected
            .into_iter()
            .map(|value| RdfTerm::Iri(format!("http://www.example.org/{value}")))
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        assert_eq!(
            actual, expected,
            "{query_asset} 必须匹配其原始 SRX result bag"
        );
    }
}

#[test]
fn applies_ontop_property_path_sequence_and_inverse_ask_assets() {
    for (query_asset, facts_asset, expected) in [
        ("pp01.rq", "pp01.ttl", vec!["c"]),
        ("pp03.rq", "pp03.ttl", vec!["a"]),
        ("pp11.rq", "pp11.ttl", vec!["c", "c"]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mapping = dir.path().join("x.obda");
        let facts = dir.path().join(facts_asset);
        std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
        let fact_source = match facts_asset {
            "pp01.ttl" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp01.ttl"),
            "pp03.ttl" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp03.ttl"),
            "pp11.ttl" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp11.ttl"),
            _ => unreachable!("固定 property-path facts asset"),
        };
        let query = match query_asset {
            "pp01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp01.rq"),
            "pp03.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp03.rq"),
            "pp11.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp11.rq"),
            _ => unreachable!("固定 property-path query asset"),
        };
        std::fs::write(&facts, fact_source).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping,
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("{query_asset} 必须返回 bindings")
        };
        assert_eq!(
            rows.iter().map(|row| row["x"].clone()).collect::<Vec<_>>(),
            expected
                .into_iter()
                .map(|value| RdfTerm::Iri(format!("http://www.example.org/instance#{value}")))
                .collect::<Vec<_>>(),
            "{query_asset} 必须匹配其原始 SRX result bag"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp08.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp08.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert!(
        matches!(runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp08.rq")), Ok(rtop::QueryResult::Boolean(true))),
        "pp08.srx 必须为 true"
    );
}

#[test]
fn applies_ontop_property_path_two_named_graphs_do_not_join_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("pp06.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    // pp06 的两个 graphData 各自仅含一跳；N-Quads 适配层保留 manifest 的
    // graphData 身份，确保 sequence 不会跨 named graph 拼接。
    std::fs::write(
        &facts,
        "<http://www.example.org/instance#a> <http://www.example.org/schema#p1> <http://www.example.org/instance#b> <http://www.w3.org/2009/sparql/docs/tests/data-sparql11/property-path/pp061.ttl> .\n<http://www.example.org/instance#b> <http://www.example.org/schema#p2> <http://www.example.org/instance#c> <http://www.w3.org/2009/sparql/docs/tests/data-sparql11/property-path/pp062.ttl> .\n",
    )
    .unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    assert!(
        matches!(runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/property-path/pp06.rq")), Ok(rtop::QueryResult::Bindings(rows)) if rows.is_empty()),
        "pp06.srx 必须为空 result bag"
    );
}

#[test]
fn applies_ontop_numeric_function_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data.ttl"),
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
    let decimal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    };
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    for (asset, variable, expected) in [
        (
            "ceil01.rq",
            "ceil",
            vec![
                ("n1", integer("-1")),
                ("n2", decimal("-1")),
                ("n3", decimal("2")),
                ("n4", integer("-2")),
                ("n5", decimal("3")),
            ],
        ),
        (
            "floor01.rq",
            "floor",
            vec![
                ("n1", integer("-1")),
                ("n2", decimal("-2")),
                ("n3", decimal("1")),
                ("n4", integer("-2")),
                ("n5", decimal("2")),
            ],
        ),
        (
            "round01.rq",
            "round",
            vec![
                ("n1", integer("-1")),
                ("n2", decimal("-2")),
                ("n3", decimal("1")),
                ("n4", integer("-2")),
                ("n5", decimal("3")),
            ],
        ),
    ] {
        let query = match asset {
            "ceil01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/ceil01.rq"),
            "floor01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/floor01.rq"),
            "round01.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/round01.rq"),
            _ => unreachable!("固定 numeric function asset"),
        };
        let mut runtime = VkgRuntime::new(spec.clone(), NullSource).unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("{asset} 必须返回 bindings")
        };
        let mut actual = rows
            .iter()
            .map(|row| (row["s"].clone(), row[variable].clone()))
            .collect::<Vec<_>>();
        let mut expected = expected
            .into_iter()
            .map(|(subject, value)| (RdfTerm::Iri(format!("http://example.org/{subject}")), value))
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        assert_eq!(
            actual, expected,
            "{asset} 必须匹配原始 SRX 的 datatype 与 lexical"
        );
    }
    let mut runtime = VkgRuntime::new(spec, NullSource).unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/abs01.rq"))
        .unwrap()
    else {
        panic!("abs01 必须返回 bindings")
    };
    let mut actual = rows
        .iter()
        .map(|row| (row["s"].clone(), row["num"].clone()))
        .collect::<Vec<_>>();
    let mut expected = vec![
        (RdfTerm::Iri("http://example.org/n4".into()), integer("-2")),
        (RdfTerm::Iri("http://example.org/n5".into()), decimal("2.5")),
    ];
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "abs01.srx");
}

#[test]
fn applies_ontop_string_function_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data.ttl"),
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
    let query = |asset: &str| {
        match asset {
        "concat01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/concat01.rq"),
        "substring01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/substring01.rq"),
        "substring02" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/substring02.rq"),
        "length01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/length01.rq"),
        "ucase01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/ucase01.rq"),
        "lcase01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/lcase01.rq"),
        "encode01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/encode01.rq"),
        "contains01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/contains01.rq"),
        "starts01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/starts01.rq"),
        "ends01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/ends01.rq"),
        _ => unreachable!("固定 Ontop string function asset"),
    }
    };
    for (asset, count) in [
        ("concat01", 1),
        ("substring01", 7),
        ("substring02", 7),
        ("length01", 7),
        ("ucase01", 7),
        ("lcase01", 7),
        ("encode01", 7),
        ("contains01", 2),
        ("starts01", 2),
        ("ends01", 1),
    ] {
        let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec.clone(), NullSource)
            .unwrap()
            .query(query(asset))
            .unwrap()
        else {
            panic!("{asset}.rq 必须返回 bindings")
        };
        assert_eq!(rows.len(), count, "{asset}.srx result bag 大小");
        match asset {
            "concat01" => assert_eq!(rows[0]["str"], RdfTerm::Literal { value: "abcDEF".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None }),
            "substring01" => assert!(rows.iter().any(|row| row["s"] == RdfTerm::Iri("http://example.org/s4".into()) && row["substr"] == RdfTerm::Literal { value: "食".into(), datatype: None, language: None })),
            "substring02" => assert!(rows.iter().any(|row| row["s"] == RdfTerm::Iri("http://example.org/s2".into()) && row["substr"] == RdfTerm::Literal { value: "ar".into(), datatype: None, language: Some("en".into()) })),
            "length01" => assert!(rows.iter().any(|row| row["str"] == RdfTerm::Literal { value: "食べ物".into(), datatype: None, language: None } && row["len"] == RdfTerm::Literal { value: "3".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()), language: None })),
            "ucase01" => assert!(rows.iter().any(|row| row["s"] == RdfTerm::Iri("http://example.org/s2".into()) && row["ustr"] == RdfTerm::Literal { value: "BAR".into(), datatype: None, language: Some("en".into()) })),
            "lcase01" => assert!(rows.iter().any(|row| row["s"] == RdfTerm::Iri("http://example.org/s7".into()) && row["lstr"] == RdfTerm::Literal { value: "def".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()), language: None })),
            "encode01" => assert!(rows.iter().any(|row| row["s"] == RdfTerm::Iri("http://example.org/s4".into()) && row["encoded"] == RdfTerm::Literal { value: "%E9%A3%9F%E3%81%B9%E7%89%A9".into(), datatype: None, language: None })),
            "contains01" => assert!(rows.iter().all(|row| matches!(&row["s"], RdfTerm::Iri(value) if value == "http://example.org/s2" || value == "http://example.org/s6"))),
            "starts01" => assert!(rows.iter().any(|row| row["s"] == RdfTerm::Iri("http://example.org/n3".into())) && rows.iter().any(|row| row["s"] == RdfTerm::Iri("http://example.org/s5".into()))),
            "ends01" => assert_eq!(rows[0]["s"], RdfTerm::Iri("http://example.org/s6".into())),
            _ => unreachable!(),
        }
    }
}

#[test]
fn applies_ontop_string_slice_and_replace_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec_for = |name: &str, data: &str| {
        let facts = dir.path().join(format!("{name}.ttl"));
        std::fs::write(facts.clone(), data).unwrap();
        KnowledgeGraphSpec {
            mapping_file: mapping.clone(),
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        }
    };
    let data2 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data2.ttl");
    for (asset, variable, expected) in [
        (
            "strbefore01",
            "prefix",
            vec![
                (
                    "s1",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                (
                    "s2",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                (
                    "s3",
                    RdfTerm::Literal {
                        value: "engli".into(),
                        datatype: None,
                        language: Some("en".into()),
                    },
                ),
                (
                    "s4",
                    RdfTerm::Literal {
                        value: "françai".into(),
                        datatype: None,
                        language: Some("fr".into()),
                    },
                ),
                (
                    "s5",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                (
                    "s6",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
            ],
        ),
        (
            "strafter01",
            "suffix",
            vec![
                (
                    "s1",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                (
                    "s2",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                (
                    "s3",
                    RdfTerm::Literal {
                        value: "nglish".into(),
                        datatype: None,
                        language: Some("en".into()),
                    },
                ),
                (
                    "s4",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                (
                    "s5",
                    RdfTerm::Literal {
                        value: "".into(),
                        datatype: None,
                        language: None,
                    },
                ),
                (
                    "s6",
                    RdfTerm::Literal {
                        value: "f".into(),
                        datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
                        language: None,
                    },
                ),
            ],
        ),
    ] {
        let query = match asset {
            "strbefore01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strbefore01.rq"),
            "strafter01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strafter01.rq"),
            _ => unreachable!(),
        };
        let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec_for(asset, data2), NullSource)
            .unwrap()
            .query(query)
            .unwrap()
        else {
            panic!("{asset}.rq 必须返回 bindings")
        };
        assert_eq!(rows.len(), 7, "{asset}.srx result bag 大小");
        for (subject, value) in expected {
            assert!(
                rows.iter().any(|row| row["s"]
                    == RdfTerm::Iri(format!("http://example.org/{subject}"))
                    && row.get(variable) == Some(&value)),
                "{asset}.srx 的 {subject}"
            );
        }
        assert!(
            rows.iter().any(
                |row| row["s"] == RdfTerm::Iri("http://example.org/s7".into())
                    && !row.contains_key(variable)
            ),
            "{asset}.srx 必须保留 numeric expression error 的 solution"
        );
    }
    let data3 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data3.ttl");
    for (asset, expected) in [("replace01", 9usize), ("replace02", 1), ("replace03", 1)] {
        let query = match asset {
            "replace01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/replace01.rq"),
            "replace02" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/replace02.rq"),
            "replace03" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/replace03.rq"),
            _ => unreachable!(),
        };
        let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec_for(asset, data3), NullSource)
            .unwrap()
            .query(query)
            .unwrap()
        else {
            panic!("{asset}.rq 必须返回 bindings")
        };
        assert_eq!(rows.len(), expected, "{asset}.srx result bag 大小");
        let expected_value = match asset {
            "replace02" => "b*na",
            "replace03" => "[1=ab][2=]cd",
            _ => "-nglish",
        };
        assert!(
            rows.iter().any(|row| row.get("new")
                == Some(&RdfTerm::Literal {
                    value: expected_value.into(),
                    datatype: None,
                    language: (asset == "replace01").then(|| "en".into())
                })),
            "{asset}.srx 关键 replacement lexical 与 literal 身份"
        );
    }
}

#[test]
fn applies_ontop_string_slice_datatype_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data4.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data4.ttl")).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    for (asset, matching, empty) in [("strbefore02", "a", ""), ("strafter02", "c", "")] {
        let query = match asset {
            "strbefore02" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strbefore02.rq"),
            "strafter02" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/strafter02.rq"),
            _ => unreachable!(),
        };
        let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec.clone(), NullSource)
            .unwrap()
            .query(query)
            .unwrap()
        else {
            panic!("{asset}.rq 必须返回 bindings")
        };
        assert_eq!(rows.len(), 3, "{asset}.srx result bag 大小");
        let primary = if asset == "strbefore02" { "bb" } else { "ab" };
        for subject in ["s1", "s3"] {
            let row = rows
                .iter()
                .find(|row| row["s"] == RdfTerm::Iri(format!("http://example.org/{subject}")))
                .unwrap();
            assert_eq!(
                row[primary],
                RdfTerm::Literal {
                    value: matching.into(),
                    datatype: (subject == "s3")
                        .then(|| "http://www.w3.org/2001/XMLSchema#string".into()),
                    language: None
                }
            );
            assert!(!row.contains_key(if asset == "strbefore02" {
                "bbcy"
            } else {
                "abcy"
            }));
            assert!(!row.contains_key(if asset == "strbefore02" { "ben" } else { "aen" }));
            let no_match = if asset == "strbefore02" {
                "bxyzx"
            } else {
                "axyzx"
            };
            assert_eq!(
                row[no_match],
                RdfTerm::Literal {
                    value: empty.into(),
                    datatype: None,
                    language: None
                }
            );
        }
        let row = rows
            .iter()
            .find(|row| row["s"] == RdfTerm::Iri("http://example.org/s2".into()))
            .unwrap();
        assert_eq!(
            row[primary],
            RdfTerm::Literal {
                value: matching.into(),
                datatype: None,
                language: Some("en".into())
            }
        );
        assert!(!row.contains_key(if asset == "strbefore02" {
            "bbcy"
        } else {
            "abcy"
        }));
        let same_language_empty = if asset == "strbefore02" { "ben" } else { "aen" };
        assert!(
            row.contains_key(same_language_empty),
            "{asset}.srx 的 @en 参数应兼容 @en input"
        );
    }
}

#[test]
fn applies_ontop_datetime_and_isnumeric_function_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data.ttl")).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    let decimal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    };
    for (asset, expected) in [
        (
            "minutes-01",
            vec![integer("28"), integer("38"), integer("59"), integer("2")],
        ),
        (
            "seconds-01",
            vec![decimal("1"), decimal("2"), decimal("0"), decimal("3")],
        ),
        (
            "month-01",
            vec![integer("6"), integer("12"), integer("6"), integer("2")],
        ),
        (
            "year-01",
            vec![
                integer("2010"),
                integer("2010"),
                integer("2008"),
                integer("2011"),
            ],
        ),
    ] {
        let query = match asset {
            "minutes-01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/minutes-01.rq"),
            "seconds-01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/seconds-01.rq"),
            "month-01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/month-01.rq"),
            "year-01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/year-01.rq"),
            _ => unreachable!(),
        };
        let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec.clone(), NullSource)
            .unwrap()
            .query(query)
            .unwrap()
        else {
            panic!("{asset}.rq 必须返回 bindings")
        };
        let mut actual = rows
            .into_iter()
            .map(|row| (row["s"].clone(), row["x"].clone()))
            .collect::<Vec<_>>();
        let mut expected = expected
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                (
                    RdfTerm::Iri(format!("http://example.org/d{}", index + 1)),
                    value,
                )
            })
            .collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "{asset}.srx 的完整 RDF term bag");
    }
    let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec, NullSource).unwrap().query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/isnumeric01.rq")).unwrap() else { panic!("isnumeric01.rq 必须返回 bindings") };
    let mut actual = rows
        .into_iter()
        .map(|row| (row["s"].clone(), row["num"].clone()))
        .collect::<Vec<_>>();
    let mut expected = vec![
        ("n1", integer("-1")),
        ("n2", decimal("-1.6")),
        ("n3", decimal("1.1")),
        ("n4", integer("-2")),
        ("n5", decimal("2.5")),
    ]
    .into_iter()
    .map(|(subject, number)| {
        (
            RdfTerm::Iri(format!("http://example.org/{subject}")),
            number,
        )
    })
    .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "isnumeric01.srx 的完整 RDF term bag");
}

#[test]
fn applies_ontop_concat02_type_matrix_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data2.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data2.ttl")).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec, NullSource).unwrap().query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/concat02.rq")).unwrap() else { panic!("concat02.rq 必须返回 bindings") };
    assert_eq!(
        rows.len(),
        49,
        "concat02.srx 包含 49 个 cross-product solution mappings"
    );
    assert_eq!(
        rows.iter().filter(|row| !row.contains_key("str")).count(),
        13,
        "涉及 numeric s7 的 13 个结果必须为未绑定 expression error"
    );
    let mut actual = rows
        .into_iter()
        .filter_map(|row| row.get("str").cloned())
        .collect::<Vec<_>>();
    let mut expected = [
        "123123",
        "123日本語",
        "123english",
        "123français",
        "123abc",
        "123def",
        "日本語123",
        "日本語english",
        "日本語français",
        "日本語abc",
        "日本語def",
        "english123",
        "english日本語",
        "englishfrançais",
        "englishabc",
        "englishdef",
        "français123",
        "français日本語",
        "françaisenglish",
        "françaisabc",
        "françaisdef",
        "abc123",
        "abc日本語",
        "abcenglish",
        "abcfrançais",
        "def123",
        "def日本語",
        "defenglish",
        "deffrançais",
    ]
    .into_iter()
    .map(|value| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    })
    .collect::<Vec<_>>();
    expected.extend([
        RdfTerm::Literal {
            value: "日本語日本語".into(),
            datatype: None,
            language: Some("ja".into()),
        },
        RdfTerm::Literal {
            value: "englishenglish".into(),
            datatype: None,
            language: Some("en".into()),
        },
        RdfTerm::Literal {
            value: "françaisfrançais".into(),
            datatype: None,
            language: Some("fr".into()),
        },
        RdfTerm::Literal {
            value: "abcabc".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
        RdfTerm::Literal {
            value: "abcdef".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
        RdfTerm::Literal {
            value: "defabc".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
        RdfTerm::Literal {
            value: "defdef".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
    ]);
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "concat02.srx 的完整 literal identity bag");
}

#[test]
fn applies_ontop_sha256_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    for (asset, data, expected) in [
        ("sha256-01", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data.ttl"), "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae"),
        ("sha256-02", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/hash-unicode.ttl"), "0fbe868d1df356ca9df7ebff346da3a56280e059a7ea81186ef885b140d254ee"),
    ] {
        let facts = dir.path().join(format!("{asset}.ttl"));
        std::fs::write(&facts, data).unwrap();
        let spec = KnowledgeGraphSpec { mapping_file: mapping.clone(), facts_file: Some(facts), facts_format: None, facts_base_iri: None, ontology_file: None, xml_catalog_file: None };
        let query = match asset {
            "sha256-01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/sha256-01.rq"),
            "sha256-02" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/sha256-02.rq"),
            _ => unreachable!(),
        };
        assert!(matches!(VkgRuntime::new(spec, NullSource).unwrap().query(query), Ok(rtop::QueryResult::Bindings(rows)) if rows == vec![std::collections::BTreeMap::from([("hash".into(), RdfTerm::Literal { value: expected.into(), datatype: None, language: None })])]), "{asset}.srx 的 SHA256 simple literal");
    }
}

#[test]
fn applies_ontop_bnode_and_in_function_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data.ttl")).unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec.clone(), NullSource).unwrap().query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/bnode01.rq")).unwrap() else { panic!("bnode01.rq 必须返回 bindings") };
    assert_eq!(rows.len(), 4, "bnode01.srx result bag 大小");
    for row in &rows {
        let (RdfTerm::Literal { value: left, .. }, RdfTerm::Literal { value: right, .. }) =
            (&row["s1"], &row["s2"])
        else {
            panic!("bnode01 的输入必须为 literal")
        };
        match (&row["b1"], &row["b2"]) {
            (RdfTerm::BlankNode(left_bnode), RdfTerm::BlankNode(right_bnode)) => {
                assert_eq!(
                    left_bnode == right_bnode,
                    left == right,
                    "bnode01 对相同输入必须稳定，对不同输入必须 fresh"
                );
            }
            _ => panic!("bnode01 必须产生 blank node"),
        }
    }
    let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec.clone(), NullSource).unwrap().query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/bnode02.rq")).unwrap() else { panic!("bnode02.rq 必须返回 bindings") };
    assert!(
        matches!(&rows[..], [row] if matches!((&row["b1"], &row["b2"]), (RdfTerm::BlankNode(left), RdfTerm::BlankNode(right)) if left != right)),
        "bnode02.srx 的两次零参数调用必须生成不同 blank node"
    );
    for (asset, expected) in [
        ("in01", true),
        ("in02", false),
        ("notin01", true),
        ("notin02", false),
    ] {
        let query = match asset {
            "in01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/in01.rq"),
            "in02" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/in02.rq"),
            "notin01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/notin01.rq"),
            "notin02" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/notin02.rq"),
            _ => unreachable!(),
        };
        assert_eq!(
            VkgRuntime::new(spec.clone(), NullSource)
                .unwrap()
                .query(query)
                .unwrap(),
            rtop::QueryResult::Boolean(expected),
            "{asset}.srx"
        );
    }
}

#[test]
fn applies_ontop_if_and_coalesce_function_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let spec_for = |name: &str, data: &str| {
        let facts = dir.path().join(format!("{name}.ttl"));
        std::fs::write(facts.clone(), data).unwrap();
        KnowledgeGraphSpec {
            mapping_file: mapping.clone(),
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        }
    };
    let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec_for("if", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data2.ttl")), NullSource).unwrap().query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/if01.rq")).unwrap() else { panic!("if01.rq 必须返回 bindings") };
    assert_eq!(rows.len(), 7, "if01.srx result bag 大小");
    assert_eq!(
        rows.iter()
            .filter(|row| row["integer"]
                == RdfTerm::Literal {
                    value: "true".into(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()),
                    language: None
                })
            .count(),
        1,
        "if01 仅 @ja 为 true"
    );
    assert!(
        matches!(VkgRuntime::new(spec_for("if-error", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data2.ttl")), NullSource).unwrap().query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/if02.rq")), Ok(rtop::QueryResult::Bindings(rows)) if rows == vec![std::collections::BTreeMap::new()]),
        "if02.srx 必须保留 error 的空 solution mapping"
    );
    let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec_for("coalesce", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data-coalesce.ttl")), NullSource).unwrap().query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/coalesce01.rq")).unwrap() else { panic!("coalesce01.rq 必须返回 bindings") };
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    let decimal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    };
    let mut actual = rows
        .into_iter()
        .map(|row| {
            (
                row["cx"].clone(),
                row["div"].clone(),
                row["def"].clone(),
                row.contains_key("err"),
            )
        })
        .collect::<Vec<_>>();
    let mut expected = vec![
        (integer("-1"), integer("-2"), integer("-3"), false),
        (integer("0"), integer("-2"), integer("-3"), false),
        (integer("2"), decimal("0.0"), integer("-3"), false),
        (integer("2"), decimal("2.0"), integer("-3"), false),
    ];
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "coalesce01.srx 的 fallback、division error 与 unbound err"
    );
}

#[test]
fn applies_ontop_iri_nondeterministic_function_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-empty.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data-empty.nt"),
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
    let query = |name| {
        match name {
        "iri01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/iri01.rq"),
        "now01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/now01.rq"),
        "rand01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/rand01.rq"),
        "uuid01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/uuid01.rq"),
        "struuid01" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/struuid01.rq"),
        _ => unreachable!(),
    }
    };
    let iri = VkgRuntime::new(spec.clone(), NullSource)
        .unwrap()
        .query(query("iri01"))
        .unwrap();
    assert!(
        matches!(iri, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("iri".into(), RdfTerm::Iri("http://example.org/iri".into())),
            ("uri".into(), RdfTerm::Iri("http://example.org/uri".into())),
        ])]),
        "iri01.srx"
    );
    for name in ["now01", "rand01"] {
        assert_eq!(
            VkgRuntime::new(spec.clone(), NullSource)
                .unwrap()
                .query(query(name))
                .unwrap(),
            rtop::QueryResult::Boolean(true),
            "{name}.srx"
        );
    }
    for (name, length) in [("uuid01", "45"), ("struuid01", "36")] {
        let expected = RdfTerm::Literal {
            value: length.into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
            language: None,
        };
        assert!(
            matches!(VkgRuntime::new(spec.clone(), NullSource).unwrap().query(query(name)), Ok(rtop::QueryResult::Bindings(rows)) if rows == vec![std::collections::BTreeMap::from([("length".into(), expected)])]),
            "{name}.srx"
        );
    }
}

#[test]
fn applies_ontop_plus_function_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-builtin-3.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/data-builtin-3.ttl"),
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
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    let decimal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
        language: None,
    };
    for (name, query) in [
        (
            "plus-1",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/plus-1.rq"),
        ),
        (
            "plus-2",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/functions/plus-2.rq"),
        ),
    ] {
        let rtop::QueryResult::Bindings(rows) = VkgRuntime::new(spec.clone(), NullSource)
            .unwrap()
            .query(query)
            .unwrap()
        else {
            panic!("{name}.srx 必须返回 bindings");
        };
        assert_eq!(rows.len(), 8, "{name}.srx result bag");
        let sums = rows
            .iter()
            .filter_map(|row| row.get("sum"))
            .cloned()
            .collect::<Vec<_>>();
        if name == "plus-1" {
            assert_eq!(sums, vec![integer("3"), decimal("3.0")], "{name}.srx");
        } else {
            assert!(sums.is_empty(), "{name}.srx：STR() 结果不可数值相加");
        }
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row.get("x"), Some(RdfTerm::BlankNode(_)))
                    && matches!(row.get("y"), Some(RdfTerm::Literal { value, datatype: None, language: None }) if value == "1"))
                .count(),
            1,
            "{name}.srx 保留 blank node 输入及未绑定 sum"
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row.get("x") == Some(&RdfTerm::Iri("http://example/a".into()))
                    && row.get("y")
                        == Some(&RdfTerm::Literal {
                            value: "1".into(),
                            datatype: None,
                            language: None,
                        }))
                .count(),
            1,
            "{name}.srx 保留 IRI 输入及未绑定 sum"
        );
    }
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
fn does_not_equate_datetime_literals_with_different_timezone_instants() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("facts.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, "<https://example.test/a> <https://example.test/timestamp> \"2008-04-02T00:00:00Z\"^^<http://www.w3.org/2001/XMLSchema#dateTime> .").unwrap();
    let spec = KnowledgeGraphSpec {
        mapping_file: mapping,
        facts_file: Some(facts),
        facts_format: None,
        facts_base_iri: None,
        ontology_file: None,
        xml_catalog_file: None,
    };
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime.query("SELECT ?x WHERE { ?x <https://example.test/timestamp> ?d FILTER (?d = \"2008-04-02T00:00:00-06:00\"^^<http://www.w3.org/2001/XMLSchema#dateTime>) }").unwrap();
    assert!(matches!(result, rtop::QueryResult::Bindings(rows) if rows.is_empty()));
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
    let rtop::QueryResult::Graph(graph) = runtime
        .query(
            "DESCRIBE ?person WHERE { ?person <https://example.com/type> <https://example.com/Person> }",
        )
        .unwrap()
    else {
        panic!("expected graph")
    };
    assert!(graph.iter().any(|fact| {
        matches!(&fact.subject, RdfTerm::Iri(value) if value == "https://example.com/person/8")
    }));
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
fn matches_an_explicit_xsd_string_mapping_literal_against_an_xsd_string_filter() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            _: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            Ok(vec![vec![Some("1".into()), Some("2013-03-18".into())]])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(
        &mapping,
        "[PrefixDeclaration]\nxsd: http://www.w3.org/2001/XMLSchema#\n[MappingDeclaration]\ntarget <https://example.test/date/{id}> <https://example.test/date> {value}^^xsd:string .\nsource SELECT id, value FROM dates\n",
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
    let result = runtime.query("PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\nSELECT ?date ?value { ?date <https://example.test/date> ?value FILTER (?value = \"2013-03-18\"^^xsd:string) }");
    assert!(
        matches!(result, Ok(rtop::QueryResult::Bindings(ref rows)) if rows.len() == 1),
        "{result:?}"
    );
}

#[test]
fn matches_a_timestamptz_string_mapping_against_an_equivalent_offset_filter() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            _: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            Ok(vec![vec![
                Some("1".into()),
                Some("2013-03-19T02:12:10+00:00".into()),
            ]])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(
        &mapping,
        "[PrefixDeclaration]\nxsd: http://www.w3.org/2001/XMLSchema#\n[MappingDeclaration]\ntarget <https://example.test/date/{id}> <https://example.test/date> {value}^^xsd:string .\nsource SELECT id, value FROM dates\n",
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
    let result = runtime.query("PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\nSELECT ?date ?value { ?date <https://example.test/date> ?value FILTER (?value = \"2013-03-19T03:12:10+01:00\"^^xsd:string) }");
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
fn projects_a_variable_named_graph_from_a_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("named-graph.ttl");
    std::fs::write(
        &mapping,
        r#"@prefix rr: <http://www.w3.org/ns/r2rml#> .
<map> a rr:TriplesMap;
  rr:logicalTable [ rr:tableName "source" ];
  rr:subjectMap [ rr:constant <https://example.test/s>; rr:graph <https://example.test/graph> ];
  rr:predicateObjectMap [ rr:predicate <https://example.test/p>; rr:objectMap [ rr:constant "named value" ] ] .
"#,
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
    let mut runtime = VkgRuntime::new(spec, NamedGraphMappingSource).unwrap();
    let result = runtime
        .query("SELECT ?s ?p ?o ?g WHERE { GRAPH ?g { ?s ?p ?o } }")
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(rows) if rows == vec![std::collections::BTreeMap::from([
            ("s".into(), RdfTerm::Iri("https://example.test/s".into())),
            ("p".into(), RdfTerm::Iri("https://example.test/p".into())),
            ("o".into(), RdfTerm::Literal { value: "named value".into(), datatype: None, language: None }),
            ("g".into(), RdfTerm::Iri("https://example.test/graph".into())),
        ])])
    );
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
fn accepts_disjoint_type_facts_like_the_fixed_ontop_endpoint() {
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
    let mut runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();
    let result = runtime
        .query("SELECT ?s WHERE { ?s a <http://example.org/voc#Course> }")
        .unwrap();
    assert_eq!(
        format!("{result:?}"),
        "Bindings([{\"s\": Iri(\"https://example.test/alice\")}])"
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

#[test]
fn reformulation_diagnostics_reject_non_single_bgp_select_and_non_select_queries() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
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
    let runtime = VkgRuntime::new(spec, FakeSource { sql: String::new() }).unwrap();

    assert!(
        matches!(runtime.reformulate("SELECT ?person { ?person <https://example.test/name> ?name . ?person <https://example.test/age> ?age . }"), Err(RuntimeError::NotFullyTranslatable(message)) if message.contains("BGP 改写诊断"))
    );
    assert!(
        matches!(runtime.reformulate("ASK { ?person <https://example.test/name> ?name }"), Err(RuntimeError::UnsupportedSparql(message)) if message.contains("仅支持 SELECT"))
    );
}

#[test]
fn preserves_direct_mapping_plans_while_expanding_subclass_and_subproperty_entailment() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            sql: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            let id = if sql.contains("FROM parents") {
                "parent"
            } else if sql.contains("FROM students") {
                "student"
            } else if sql.contains("FROM parent_properties") {
                "parent-property"
            } else {
                assert!(sql.contains("FROM child_properties"), "{sql}");
                "child-property"
            };
            Ok(vec![vec![Some(id.into())]])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    let ontology = dir.path().join("ontology.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\n\
         mappingId parent-class\n\
         target <https://example.test/person/{id}> a <https://example.test/Person> .\n\
         source SELECT id FROM parents\n\n\
         mappingId student-class\n\
         target <https://example.test/person/{id}> a <https://example.test/Student> .\n\
         source SELECT id FROM students\n\n\
         mappingId parent-property\n\
         target <https://example.test/person/{id}> <https://example.test/parentProperty> <https://example.test/value> .\n\
         source SELECT id FROM parent_properties\n\n\
         mappingId child-property\n\
         target <https://example.test/person/{id}> <https://example.test/childProperty> <https://example.test/value> .\n\
         source SELECT id FROM child_properties\n",
    )
    .unwrap();
    std::fs::write(
        &ontology,
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         <https://example.test/Student> rdfs:subClassOf <https://example.test/Person> .\n\
         <https://example.test/childProperty> rdfs:subPropertyOf <https://example.test/parentProperty> .\n",
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

    for query in [
        "SELECT ?person { ?person a <https://example.test/Person> }",
        "SELECT ?person { ?person <https://example.test/parentProperty> <https://example.test/value> }",
    ] {
        let result = runtime.query(query).unwrap();
        assert!(
            matches!(result, rtop::QueryResult::Bindings(ref rows) if rows.len() == 2),
            "{query}: {result:?}"
        );
    }
}

#[test]
fn infers_mapping_types_and_inverse_properties_without_direct_query_mappings() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            sql: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            assert!(sql.contains("FROM teaches"), "{sql}");
            let projection = sql.split(" FROM ").next().unwrap_or(sql);
            if projection.matches("CAST(").count() == 1 && projection.contains("CAST(course") {
                Ok(vec![vec![Some(
                    "https://example.test/course/sparql".into(),
                )]])
            } else {
                Ok(vec![vec![
                    Some("https://example.test/person/ada".into()),
                    Some("https://example.test/course/sparql".into()),
                ]])
            }
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    let ontology = dir.path().join("ontology.ttl");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\n\
         target <https://example.test/person/{id}> <https://example.test/teaches> <https://example.test/course/{course}> .\n\
         source SELECT id, course FROM teaches\n",
    )
    .unwrap();
    std::fs::write(
        &ontology,
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         <https://example.test/teaches> rdfs:domain <https://example.test/Teacher>;\n\
             rdfs:range <https://example.test/Course>;\n\
             owl:inverseOf <https://example.test/isTaughtBy> .\n",
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

    for (query, expected) in [
        (
            "SELECT ?teacher { ?teacher a <https://example.test/Teacher> }",
            "https://example.test/person/ada",
        ),
        (
            "SELECT ?course { ?course a <https://example.test/Course> }",
            "https://example.test/course/sparql",
        ),
        (
            "SELECT ?course { ?course <https://example.test/isTaughtBy> <https://example.test/person/ada> }",
            "https://example.test/course/sparql",
        ),
    ] {
        let result = runtime.query(query).unwrap();
        assert!(
            matches!(result, rtop::QueryResult::Bindings(ref rows)
                if rows.len() == 1 && rows[0].values().any(|term| matches!(term, RdfTerm::Iri(value) if value == expected))),
            "{query}: {result:?}"
        );
    }
}

#[test]
fn treats_unmapped_optional_and_minus_right_patterns_as_empty_virtual_graphs() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            sql: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            assert!(sql.contains("FROM people"), "{sql}");
            Ok(vec![vec![
                Some("https://example.test/person/ada".into()),
                Some("Ada".into()),
            ]])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\n\
         target <https://example.test/person/{id}> <https://example.test/name> {name} .\n\
         source SELECT id, name FROM people\n",
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

    let optional = runtime
        .query(
            "SELECT ?person ?name ?missing WHERE {\n\
             ?person <https://example.test/name> ?name .\n\
             OPTIONAL { ?person <https://example.test/unmapped> ?missing }\n\
             }",
        )
        .unwrap();
    assert!(
        matches!(optional, rtop::QueryResult::Bindings(ref rows)
            if rows.len() == 1 && rows[0].contains_key("person") && rows[0].contains_key("name") && !rows[0].contains_key("missing")),
        "{optional:?}"
    );

    let minus = runtime
        .query(
            "SELECT ?person WHERE {\n\
             ?person <https://example.test/name> ?name .\n\
             MINUS { ?person <https://example.test/unmapped> ?missing }\n\
             }",
        )
        .unwrap();
    assert!(
        matches!(minus, rtop::QueryResult::Bindings(ref rows)
            if rows.len() == 1 && matches!(rows[0].get("person"), Some(RdfTerm::Iri(value)) if value == "https://example.test/person/ada")),
        "{minus:?}"
    );
}

#[test]
fn joins_a_values_input_binding_with_a_virtual_mapping_bgp() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            sql: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            assert!(sql.contains("FROM people"), "{sql}");
            Ok(vec![
                vec![
                    Some("https://example.test/person/ada".into()),
                    Some("Ada".into()),
                ],
                vec![
                    Some("https://example.test/person/bert".into()),
                    Some("Bert".into()),
                ],
            ])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\n\
         target <https://example.test/person/{id}> <https://example.test/name> {name} .\n\
         source SELECT id, name FROM people\n",
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
    let result = runtime
        .query(
            "SELECT ?name WHERE {\n\
             VALUES ?person { <https://example.test/person/ada> }\n\
             ?person <https://example.test/name> ?name\n\
             }",
        )
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
        if rows == &[std::collections::BTreeMap::from([(
            "name".into(),
            RdfTerm::Literal { value: "Ada".into(), datatype: None, language: None },
        )])]),
        "{result:?}"
    );
}

#[test]
fn treats_unmapped_exists_patterns_as_empty_virtual_graphs() {
    struct Source;
    impl DataSource for Source {
        fn execute(
            &mut self,
            sql: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            assert!(sql.contains("FROM people"), "{sql}");
            Ok(vec![vec![
                Some("https://example.test/person/ada".into()),
                Some("Ada".into()),
            ]])
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\n\
         target <https://example.test/person/{id}> <https://example.test/name> {name} .\n\
         source SELECT id, name FROM people\n",
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

    let exists = runtime
        .query(
            "SELECT ?person WHERE {\n\
             ?person <https://example.test/name> ?name .\n\
             FILTER EXISTS { ?person <https://example.test/unmapped> ?value }\n\
             }",
        )
        .unwrap();
    assert!(matches!(exists, rtop::QueryResult::Bindings(ref rows) if rows.is_empty()));

    let not_exists = runtime
        .query(
            "SELECT ?person WHERE {\n\
             ?person <https://example.test/name> ?name .\n\
             FILTER NOT EXISTS { ?person <https://example.test/unmapped> ?value }\n\
             }",
        )
        .unwrap();
    assert!(
        matches!(not_exists, rtop::QueryResult::Bindings(ref rows)
            if rows.len() == 1 && matches!(rows[0].get("person"), Some(RdfTerm::Iri(value)) if value == "https://example.test/person/ada")),
        "{not_exists:?}"
    );
}

#[test]
fn applies_ontop_dawg_cast_manifest_non_ignored_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/cast/data.ttl"),
    )
    .unwrap();
    for (asset, query, expected) in [
        (
            "cast-str.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/cast/cast-str.rq"),
            &["iri", "str", "fltdbl", "decimal", "int", "dT", "bool"][..],
        ),
        (
            "cast-flt.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/cast/cast-flt.rq"),
            &["fltdbl", "decimal", "int"][..],
        ),
        (
            "cast-dbl.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/cast/cast-dbl.rq"),
            &["fltdbl", "decimal", "int"][..],
        ),
        (
            "cast-dec.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/cast/cast-dec.rq"),
            &["decimal", "int"][..],
        ),
        (
            "cast-int.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/cast/cast-int.rq"),
            &["int"][..],
        ),
        (
            "cast-bool.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/cast/cast-bool.rq"),
            &["bool"][..],
        ),
    ] {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("{asset}.srx 应返回 bindings")
        };
        assert_eq!(rows.len(), expected.len(), "{asset}.srx：{rows:?}");
        for subject in expected {
            assert!(
                rows.iter().any(|row| row.get("s")
                    == Some(&RdfTerm::Iri(format!("http://example.org/{subject}")))),
                "{asset}.srx 缺少 subject={subject}：{rows:?}"
            );
        }
    }
}

#[test]
fn applies_ontop_dawg_boolean_effective_value_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts: &str, query: &str| {
        let facts_path = dir.path().join("facts.ttl");
        std::fs::write(&facts_path, facts).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts_path),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("boolean-effective-value query 失败：{error}\n{query}"))
        else {
            panic!("boolean-effective-value action 应返回 bindings")
        };
        rows
    };
    let data_1 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/data-1.ttl");
    let data_2 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/data-2.ttl");
    let iri = |local: &str| RdfTerm::Iri(format!("http://example.org/ns#{local}"));
    let subjects = |rows: &[std::collections::BTreeMap<String, RdfTerm>], variable: &str| {
        rows.iter()
            .filter_map(|row| row.get(variable))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        subjects(
            &run(data_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/query-boolean-literal.rq")),
            "x"
        ),
        std::collections::BTreeSet::from([iri("x2")]),
        "result-boolean-literal.ttl"
    );
    for (asset, query) in [
        ("query-bev-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/query-bev-1.rq")),
        ("query-bev-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/query-bev-3.rq")),
        ("query-bev-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/query-bev-4.rq")),
    ] {
        assert_eq!(
            subjects(&run(data_1, query), "a"),
            std::collections::BTreeSet::from([iri("x1"), iri("x2"), iri("x3"), iri("x4")]),
            "{asset} result TTL"
        );
    }
    assert_eq!(
        subjects(&run(data_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/query-bev-2.rq")), "a"),
        std::collections::BTreeSet::from([iri("y1"), iri("y2"), iri("y3"), iri("y4")]),
        "result-bev-2.ttl"
    );
    assert_eq!(
        subjects(&run(data_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/query-bev-5.rq")), "a"),
        std::collections::BTreeSet::from([iri("x1")]),
        "result-bev-5.ttl"
    );
    assert_eq!(
        run(data_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/boolean-effective-value/query-bev-6.rq")),
        vec![std::collections::BTreeMap::from([("a".into(), iri("x2")), ("w".into(), RdfTerm::Literal { value: "false".into(), datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into()), language: None })])],
        "result-bev-6.ttl"
    );
}

#[test]
fn applies_ontop_dawg_expression_operator_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/data.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("expr-ops query 失败：{error}\n{query}"))
        else {
            panic!("expr-ops action 应返回 bindings")
        };
        rows
    };
    let unary_minus = run(
        "PREFIX : <http://example.org/>\nSELECT ?s ?neg WHERE { ?s :p ?o . BIND(-?o AS ?neg) }",
    );
    assert!(
        unary_minus.iter().any(|row| row.get("s")
            == Some(&RdfTerm::Iri("http://example.org/x2".into()))
            && row.get("neg")
                == Some(&RdfTerm::Literal {
                    value: "-2".into(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                    language: None
                })),
        "一元负号中间结果：{unary_minus:?}"
    );
    for (asset, query, expected) in [
        ("query-unplus-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/query-unplus-1.rq"), &["x3"][..]),
        ("query-unminus-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/query-unminus-1.rq"), &["x2"][..]),
        ("query-plus-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/query-plus-1.rq"), &["x1", "x2"][..]),
        ("query-minus-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/query-minus-1.rq"), &["x4"][..]),
        ("query-mul-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/query-mul-1.rq"), &["x1", "x2", "x4"][..]),
        ("query-ge-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/query-ge-1.rq"), &["x3", "x4"][..]),
        ("query-le-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-ops/query-le-1.rq"), &["x1", "x2"][..]),
    ] {
        let rows = run(query);
        assert_eq!(rows.len(), expected.len(), "{asset}.srx：{rows:?}");
        for subject in expected {
            assert!(
                rows.iter().any(|row| row.get("s")
                    == Some(&RdfTerm::Iri(format!("http://example.org/{subject}")))),
                "{asset}.srx 缺少 subject={subject}：{rows:?}"
            );
        }
    }
}

#[test]
fn applies_ontop_dawg_expression_equality_non_ignored_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-eq.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/data-eq.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("expr-equals query 失败：{error}\n{query}"))
        else {
            panic!("expr-equals action 应返回 bindings")
        };
        rows
    };
    for (asset, query, expected) in [
        ("query-eq-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-1.rq"), &["xd1", "xd2", "xd3", "xi1", "xi2", "xi3"][..]),
        ("query-eq-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-2.rq"), &["xd1", "xd2", "xd3", "xi1", "xi2", "xi3"][..]),
        ("query-eq-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-3.rq"), &["xp2"][..]),
        ("query-eq-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-4.rq"), &["xp1"][..]),
        ("query-eq-5.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-5.rq"), &["xu"][..]),
        ("query-eq-graph-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-graph-1.rq"), &["xi1", "xi2"][..]),
        ("query-eq-graph-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-graph-2.rq"), &["xd1"][..]),
        ("query-eq-graph-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-graph-3.rq"), &["xp2"][..]),
        ("query-eq-graph-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-graph-4.rq"), &["xp1"][..]),
        ("query-eq-graph-5.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-equals/query-eq-graph-5.rq"), &["xu"][..]),
    ] {
        let rows = run(query);
        assert_eq!(rows.len(), expected.len(), "{asset} result TTL：{rows:?}");
        for subject in expected {
            assert!(rows.iter().any(|row| row.get("x") == Some(&RdfTerm::Iri(format!("http://example.org/things#{subject}")))), "{asset} 缺少 {subject}：{rows:?}");
        }
    }
}

#[test]
fn applies_ontop_dawg_i18n_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts: &str, query: &str| {
        let path = dir.path().join("facts.ttl");
        std::fs::write(&path, facts).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(path),
                facts_format: None,
                facts_base_iri: Some(
                    "http://www.w3.org/2001/sw/DataAccess/tests/data/i18n/kanji.ttl".into(),
                ),
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("i18n query 失败：{error}\n{query}"))
        else {
            panic!("i18n action 应返回 bindings")
        };
        rows
    };
    let kanji = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/kanji.ttl");
    let first = run(kanji, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/kanji-01.rq"));
    assert_eq!(first.len(), 2, "kanji-01-results.ttl：{first:?}");
    for (name, food) in [("Alice", "納豆"), ("Bob", "海老")] {
        assert!(
            first.iter().any(|row| row.get("name")
                == Some(&RdfTerm::Literal {
                    value: name.into(),
                    datatype: None,
                    language: None
                })
                && row.get("food")
                    == Some(&RdfTerm::Iri(format!(
                        "http://www.w3.org/2001/sw/DataAccess/tests/data/i18n/kanji.ttl#{food}"
                    )))),
            "kanji-01 缺少 {name}/{food}：{first:?}"
        );
    }
    assert_eq!(run(kanji, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/kanji-02.rq")), vec![std::collections::BTreeMap::from([("name".into(), RdfTerm::Literal { value: "Bob".into(), datatype: None, language: None })])], "kanji-02-results.ttl");
    let normalized = run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/normalization-01.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/normalization-01.rq"));
    assert_eq!(
        normalized.len(),
        2,
        "normalization-01-results.ttl：{normalized:?}"
    );
    for name in ["Bob", "Eve"] {
        assert!(
            normalized.iter().any(|row| row.get("name")
                == Some(&RdfTerm::Literal {
                    value: name.into(),
                    datatype: None,
                    language: None
                })),
            "normalization-01 缺少 {name}：{normalized:?}"
        );
    }
    for (facts, query, expected) in [
        (include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/normalization-02.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/normalization-02.rq"), "s2"),
        (include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/normalization-03.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/i18n/normalization-03.rq"), "s3"),
    ] {
        assert_eq!(run(facts, query), vec![std::collections::BTreeMap::from([("S".into(), RdfTerm::Iri(format!("http://example/vocab#{expected}")))])], "normalization result TTL");
    }
}

#[test]
fn applies_ontop_dawg_regex_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("regex-data-01.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/regex/regex-data-01.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("regex query 失败：{error}\n{query}"))
        else {
            panic!("regex action 应返回 bindings")
        };
        rows
    };
    let literal = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    for (asset, query, expected) in [
        ("regex-query-001.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/regex/regex-query-001.rq"), vec![literal("ABCdefGHIjkl")]),
        ("regex-query-002.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/regex/regex-query-002.rq"), vec![literal("abcDEFghiJKL"), literal("ABCdefGHIjkl")]),
        ("regex-query-003.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/regex/regex-query-003.rq"), vec![literal("http://example.com/literal")]),
        ("regex-query-004.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/regex/regex-query-004.rq"), vec![literal("http://example.com/literal"), RdfTerm::Iri("http://example.com/uri".into())]),
    ] {
        let rows = run(query);
        assert_eq!(rows.len(), expected.len(), "{asset} result TTL：{rows:?}");
        for value in expected { assert!(rows.iter().any(|row| row.get("val") == Some(&value)), "{asset} 缺少 {value:?}：{rows:?}"); }
    }
}

#[test]
fn applies_ontop_dawg_bound_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/bound/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let result = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/bound/bound1.rq")).unwrap();
    assert_eq!(
        result,
        rtop::QueryResult::Bindings(vec![
            std::collections::BTreeMap::from([
                ("a".into(), RdfTerm::Iri("http://example.org/ns#a2".into())),
                ("c".into(), RdfTerm::Iri("http://example.org/ns#c2".into()))
            ]),
            std::collections::BTreeMap::from([
                ("a".into(), RdfTerm::Iri("http://example.org/ns#c2".into())),
                ("c".into(), RdfTerm::Iri("http://example.org/ns#f".into()))
            ]),
        ]),
        "bound1-result.ttl：{result:?}"
    );
}

#[test]
fn applies_ontop_dawg_reduced_manifest_assets_with_lax_cardinality() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts: &str, query: &str| {
        let facts_path = dir.path().join(facts_name);
        std::fs::write(&facts_path, facts).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts_path),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("REDUCED action 应返回 bindings")
        };
        rows
    };

    let star = run(
        "reduced-star.ttl",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/reduced/reduced-star.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/reduced/reduced-1.rq"),
    );
    // manifest 指定 LaxCardinality：合法 REDUCED 实现可保留或去除重复。此
    // runtime 保留 bag multiplicity；每条输出仍须来自原始 solution sequence。
    assert!(
        (2..=3).contains(&star.len()),
        "reduced-1.srx 的 lax cardinality：{star:?}"
    );
    for subject in ["x1", "x2"] {
        assert!(
            star.iter().any(|row| {
                row.get("s") == Some(&RdfTerm::Iri(format!("http://example/{subject}")))
                    && row.get("o")
                        == Some(&RdfTerm::Literal {
                            value: "abc".into(),
                            datatype: None,
                            language: None,
                        })
            }),
            "reduced-1 缺少 {subject}：{star:?}"
        );
    }

    let strings = run(
        "reduced-str.ttl",
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/reduced/reduced-str.ttl"),
        include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/reduced/reduced-2.rq"),
    );
    assert!(
        (9..=18).contains(&strings.len()),
        "reduced-2.srx 的 lax cardinality：{strings:?}"
    );
    let allowed = [
        RdfTerm::Literal {
            value: "abc".into(),
            datatype: None,
            language: None,
        },
        RdfTerm::Literal {
            value: "abc".into(),
            datatype: None,
            language: Some("en".into()),
        },
        RdfTerm::Literal {
            value: "abc".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
        RdfTerm::Literal {
            value: "ABC".into(),
            datatype: None,
            language: None,
        },
        RdfTerm::Literal {
            value: "ABC".into(),
            datatype: None,
            language: Some("en".into()),
        },
        RdfTerm::Literal {
            value: "ABC".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
        RdfTerm::Literal {
            value: "".into(),
            datatype: None,
            language: None,
        },
        RdfTerm::Literal {
            value: "".into(),
            datatype: None,
            language: Some("en".into()),
        },
        RdfTerm::Literal {
            value: "".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#string".into()),
            language: None,
        },
    ];
    for term in &allowed {
        assert!(
            strings.iter().any(|row| row.get("v") == Some(&term)),
            "reduced-2 缺少 {term:?}：{strings:?}"
        );
    }
    assert!(
        strings
            .iter()
            .all(|row| row.get("v").is_some_and(|term| allowed.contains(term))),
        "reduced-2 不得引入原始 solution set 外的 binding：{strings:?}"
    );
}

#[test]
fn applies_ontop_dawg_solution_sequence_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/data.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("solution-seq action 应返回 bindings")
        };
        rows
    };
    let term = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some(
            if value == "1.5" {
                "http://www.w3.org/2001/XMLSchema#decimal"
            } else {
                "http://www.w3.org/2001/XMLSchema#integer"
            }
            .into(),
        ),
        language: None,
    };
    for (asset, query, expected) in [
        ("slice-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-01.rq"), &["1"][..]),
        ("slice-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-02.rq"), &["1", "1", "1.5", "2", "2", "3", "3", "4"][..]),
        ("slice-03.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-03.rq"), &[][..]),
        ("slice-04.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-04.rq"), &["1", "1.5", "2", "3", "4"][..]),
        ("slice-10.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-10.rq"), &["1", "1.5", "2", "2", "3", "3", "4"][..]),
        ("slice-11.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-11.rq"), &["1", "1", "1.5", "2", "2", "3", "3", "4"][..]),
        ("slice-12.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-12.rq"), &[][..]),
        ("slice-13.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-13.rq"), &["2", "3", "4"][..]),
        ("slice-20.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-20.rq"), &["1"][..]),
        ("slice-21.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-21.rq"), &["1", "1.5"][..]),
        ("slice-22.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-22.rq"), &[][..]),
        ("slice-23.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-23.rq"), &["1.5", "2", "2", "3", "3"][..]),
        ("slice-24.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/solution-seq/slice-24.rq"), &["2", "3", "4"][..]),
    ] {
        let actual = run(query)
            .into_iter()
            .map(|row| row.get("v").cloned().expect("?v 应绑定"))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected.iter().map(|value| term(value)).collect::<Vec<_>>(), "{asset} result TTL");
    }
}

#[test]
fn applies_ontop_dawg_basic_base_prefix_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-1.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/data-1.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("basic base-prefix action 应返回 bindings")
        };
        rows
    };
    let simple = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    let iri = |value: &str| RdfTerm::Iri(value.into());
    for (asset, query, expected) in [
        ("base-prefix-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/base-prefix-1.rq"), vec![
            std::collections::BTreeMap::from([("v".into(), simple("d:x ns:p")), ("p".into(), iri("http://example.org/ns#p"))]),
            std::collections::BTreeMap::from([("v".into(), simple("x:x x:p")), ("p".into(), iri("http://example.org/x/p"))]),
        ]),
        ("base-prefix-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/base-prefix-2.rq"), vec![
            std::collections::BTreeMap::from([("v".into(), simple("z:x z:p")), ("p".into(), iri("http://example.org/x/#p"))]),
        ]),
        ("base-prefix-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/base-prefix-3.rq"), vec![
            std::collections::BTreeMap::from([("v".into(), simple("d:x ns:p"))]),
        ]),
        ("base-prefix-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/base-prefix-4.rq"), vec![
            std::collections::BTreeMap::from([("v".into(), simple("x:x x:p"))]),
        ]),
        ("base-prefix-5.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/base-prefix-5.rq"), vec![
            std::collections::BTreeMap::from([("v".into(), simple("z:x z:p"))]),
        ]),
    ] {
        assert_eq!(run(query), expected, "{asset} SRX");
    }
}

#[test]
fn applies_ontop_dawg_basic_terms_and_dollar_variables_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts: &str, query: &str| {
        let facts_path = dir.path().join(facts_name);
        std::fs::write(&facts_path, facts).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts_path),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("basic term/var query 失败：{error}\n{query}"))
        else {
            panic!("basic term/var action 应返回 bindings")
        };
        rows
    };
    let iri = |value: &str| RdfTerm::Iri(format!("http://example.org/ns#{value}"));
    for (asset, query, variable, expected) in [
        ("term-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-1.rq"), "p", "p1"),
        ("term-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-2.rq"), "p", "p2"),
        ("term-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-3.rq"), "C", "C"),
        ("term-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-4.rq"), "p", "n1"),
        ("term-5.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-5.rq"), "p", "n1"),
        ("term-6.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-6.rq"), "p", "n2"),
        ("term-7.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-7.rq"), "p", "n2"),
        ("term-8.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-8.rq"), "p", "n3"),
        ("term-9.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/term-9.rq"), "p", "n4"),
    ] {
        assert_eq!(
            run("data-4.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/data-4.ttl"), query),
            vec![std::collections::BTreeMap::from([(variable.into(), iri(expected))])],
            "{asset} SRX"
        );
    }
    let expected = [
        std::collections::BTreeMap::from([
            ("p".into(), iri("p1")),
            (
                "v".into(),
                RdfTerm::Literal {
                    value: "1".into(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                    language: None,
                },
            ),
        ]),
        std::collections::BTreeMap::from([
            ("p".into(), iri("p2")),
            (
                "v".into(),
                RdfTerm::Literal {
                    value: "2".into(),
                    datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                    language: None,
                },
            ),
        ]),
    ];
    for (asset, query) in [
        ("var-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/var-1.rq")),
        ("var-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/var-2.rq")),
    ] {
        let actual = run("data-5.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/data-5.ttl"), query);
        assert_eq!(actual.len(), expected.len(), "{asset} SRX: {actual:?}");
        for binding in expected.clone() {
            assert!(actual.contains(&binding), "{asset} 缺少 {binding:?}: {actual:?}");
        }
    }
}

#[test]
fn applies_ontop_dawg_basic_quotes_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-3.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/data-3.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("basic quotes query 失败：{error}\n{query}"))
        else {
            panic!("basic quotes action 应返回 bindings")
        };
        rows
    };
    for (asset, query, subject) in [
        ("quotes-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/quotes-1.rq"), "x1"),
        ("quotes-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/quotes-2.rq"), "x1"),
        ("quotes-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/quotes-3.rq"), "x2"),
        ("quotes-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/quotes-4.rq"), "x3"),
    ] {
        assert_eq!(
            run(query),
            vec![std::collections::BTreeMap::from([(
                "x".into(),
                RdfTerm::Iri(format!("http://example.org/ns#{subject}")),
            )])],
            "{asset} SRX"
        );
    }
}

#[test]
fn applies_ontop_dawg_basic_collection_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-2.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/data-2.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("basic list query 失败：{error}\n{query}"))
        else {
            panic!("basic list action 应返回 bindings")
        };
        rows
    };
    let iri = |value: &str| RdfTerm::Iri(format!("http://example.org/ns#{value}"));
    let integer = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    for (asset, query, expected) in [
        ("list-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/list-1.rq"), std::collections::BTreeMap::from([("p".into(), iri("list0"))])),
        ("list-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/list-2.rq"), std::collections::BTreeMap::from([("p".into(), iri("list1"))])),
        ("list-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/list-3.rq"), std::collections::BTreeMap::from([("p".into(), iri("list1")), ("v".into(), integer("1"))])),
        ("list-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/list-4.rq"), std::collections::BTreeMap::from([("p".into(), iri("list2")), ("v".into(), integer("11")), ("w".into(), integer("22"))])),
    ] {
        assert_eq!(run(query), vec![expected], "{asset} SRX");
    }
}

#[test]
fn applies_ontop_dawg_basic_bgp_edge_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts: &str, query: &str| {
        let facts_path = dir.path().join(facts_name);
        std::fs::write(&facts_path, facts).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts_path),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime
            .query(query)
            .unwrap_or_else(|error| panic!("basic BGP edge query 失败：{error}\n{query}"))
        else {
            panic!("basic BGP edge action 应返回 bindings")
        };
        rows
    };
    let data6 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/data-6.ttl");
    assert_eq!(
        run("data-6.ttl", data6, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/spoo-1.rq")),
        vec![std::collections::BTreeMap::from([("s".into(), RdfTerm::Iri("http://example.org/ns#x".into()))])],
        "spoo-1.srx"
    );
    assert_eq!(
        run("data-6.ttl", data6, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/prefix-name-1.rq")),
        vec![std::collections::BTreeMap::from([("p".into(), RdfTerm::Iri("http://example.org/ns#p1".into()))])],
        "prefix-name-1.srx"
    );
    assert!(
        run("data-7.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/data-7.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/basic/bgp-no-match.rq")).is_empty(),
        "bgp-no-match.srx"
    );
}

#[test]
fn applies_ontop_dawg_bnode_coreference_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/bnode-coreference/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime
        .query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/bnode-coreference/query.rq"))
        .unwrap()
    else {
        panic!("bnode-coreference action 应返回 bindings")
    };
    assert_eq!(rows.len(), 3, "result.ttl 共有三个 knows binding：{rows:?}");
    let pair = |left: &RdfTerm, right: &RdfTerm| {
        rows.iter()
            .any(|row| row.get("x") == Some(left) && row.get("y") == Some(right))
    };
    let alice_to_bob = rows
        .iter()
        .find_map(|row| match (row.get("x"), row.get("y")) {
            (Some(left @ RdfTerm::BlankNode(_)), Some(right @ RdfTerm::BlankNode(_)))
                if left != right && pair(right, left) =>
            {
                Some((left.clone(), right.clone()))
            }
            _ => None,
        })
        .expect("Alice/Bob 的 reciprocal blank-node references 必须保持同一 identity");
    assert!(
        pair(&alice_to_bob.1, &alice_to_bob.0),
        "result.ttl 的反向 blank-node binding 缺失：{rows:?}"
    );
    assert!(
        rows.iter().any(|row| {
            let (Some(left @ RdfTerm::BlankNode(_)), Some(right @ RdfTerm::BlankNode(_))) =
                (row.get("x"), row.get("y"))
            else {
                return false;
            };
            left != right
                && left != &alice_to_bob.0
                && left != &alice_to_bob.1
                && right != &alice_to_bob.0
                && right != &alice_to_bob.1
        }),
        "Eve/Fred 的独立 blank-node pair 缺失：{rows:?}"
    );
}

#[test]
fn applies_ontop_dawg_ask_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/ask/data.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    for (asset, query, expected) in [
        ("ask-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/ask/ask-1.rq"), true),
        ("ask-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/ask/ask-4.rq"), false),
        ("ask-7.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/ask/ask-7.rq"), true),
        ("ask-8.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/ask/ask-8.rq"), false),
    ] {
        assert_eq!(runtime.query(query).unwrap(), rtop::QueryResult::Boolean(expected), "{asset} SRX");
    }
}

#[test]
fn applies_ontop_dawg_triple_match_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts: &str, query: &str| {
        let facts_path = dir.path().join(facts_name);
        std::fs::write(&facts_path, facts).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts_path),
                facts_format: None,
                facts_base_iri: Some("http://example.org/data/".into()),
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("triple-match action 应返回 bindings")
        };
        rows
    };
    let iri = |local: &str| RdfTerm::Iri(format!("http://example.org/data/{local}"));
    let data_01 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/triple-match/data-01.ttl");
    for (asset, query, expected) in [
        ("dawg-tp-01.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/triple-match/dawg-tp-01.rq"), vec![
            std::collections::BTreeMap::from([("p".into(), iri("p")), ("q".into(), iri("v1"))]),
            std::collections::BTreeMap::from([("p".into(), iri("p")), ("q".into(), iri("v2"))]),
        ]),
        ("dawg-tp-02.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/triple-match/dawg-tp-02.rq"), vec![
            std::collections::BTreeMap::from([("x".into(), iri("x")), ("q".into(), iri("v1"))]),
            std::collections::BTreeMap::from([("x".into(), iri("x")), ("q".into(), iri("v2"))]),
        ]),
    ] {
        let actual = run("data-01.ttl", data_01, query);
        assert_eq!(actual.len(), expected.len(), "{asset} result TTL: {actual:?}");
        for binding in expected { assert!(actual.contains(&binding), "{asset} 缺少 {binding:?}: {actual:?}"); }
    }
    assert_eq!(
        run("data-02.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/triple-match/data-02.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/triple-match/dawg-tp-03.rq")),
        vec![std::collections::BTreeMap::from([("a".into(), iri("y")), ("b".into(), iri("x"))])],
        "dawg-tp-03 result TTL"
    );
    let names = run("dawg-data-01.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/triple-match/dawg-data-01.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/triple-match/dawg-tp-04.rq"));
    assert_eq!(names.len(), 3, "dawg-tp-04 result TTL: {names:?}");
    for name in ["Alice", "Bob", "Eve"] {
        assert!(
            names.iter().any(|row| row.get("name")
                == Some(&RdfTerm::Literal {
                    value: name.into(),
                    datatype: None,
                    language: None
                })),
            "dawg-tp-04 缺少 {name}: {names:?}"
        );
    }
}

#[test]
fn applies_ontop_dawg_open_world_equality_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-1.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/data-1.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("open-world action 应返回 bindings")
        };
        rows
    };
    let iri = |local: &str| RdfTerm::Iri(format!("http://example/ns#{local}"));
    let literal = |value: &str, datatype: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: Some(datatype.into()),
        language: None,
    };
    let integer = "http://www.w3.org/2001/XMLSchema#integer";
    let type1 = "http://example/t#type1";
    assert_eq!(
        run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-01.rq")),
        Vec::<std::collections::BTreeMap<String, RdfTerm>>::new(),
        "open-eq-01-result.srx"
    );
    assert_eq!(
        run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-02.rq")),
        vec![std::collections::BTreeMap::from([("x".into(), iri("x1"))])],
        "open-eq-02-result.srx"
    );
    for (asset, query, expected) in [
        (
            "open-eq-03.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-03.rq"),
            vec![
                std::collections::BTreeMap::from([("x".into(), iri("z1")), ("v".into(), literal("1", integer))]),
                std::collections::BTreeMap::from([("x".into(), iri("z2")), ("v".into(), literal("01", integer))]),
            ],
        ),
        (
            "open-eq-04.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-04.rq"),
            vec![
                std::collections::BTreeMap::from([("x".into(), iri("z3")), ("v".into(), literal("2", integer))]),
                std::collections::BTreeMap::from([("x".into(), iri("z4")), ("v".into(), literal("02", integer))]),
            ],
        ),
        (
            "open-eq-05.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-05.rq"),
            vec![std::collections::BTreeMap::from([("x".into(), iri("x1")), ("v".into(), literal("a", type1))])],
        ),
        (
            "open-eq-06.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-06.rq"),
            vec![],
        ),
    ] {
        let actual = run(query);
        assert_eq!(actual.len(), expected.len(), "{asset} result SRX: {actual:?}");
        for binding in expected {
            assert!(actual.contains(&binding), "{asset} 缺少 {binding:?}: {actual:?}");
        }
    }
}

#[test]
fn applies_ontop_dawg_open_world_pair_equality_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-2.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/data-2.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("open-world pair action 应返回 bindings")
        };
        rows
    };
    let local = |term: &RdfTerm| match term {
        RdfTerm::Iri(iri) => iri.rsplit('/').next().unwrap_or(iri).to_owned(),
        other => panic!("预期 x/y IRI，实际为 {other:?}"),
    };
    let pair_set =
        |rows: Vec<std::collections::BTreeMap<String, RdfTerm>>, left: &str, right: &str| {
            rows.into_iter()
                .map(|row| {
                    (
                        local(row.get(left).expect("result 有左侧 IRI")),
                        local(row.get(right).expect("result 有右侧 IRI")),
                    )
                })
                .collect::<std::collections::BTreeSet<_>>()
        };
    let equality = |left: &str, right: &str| {
        (matches!(
            (left, right),
            ("x1" | "x4", "x1" | "x4") | ("x2" | "x3", "x2" | "x3")
        ) || left == right)
    };
    let incomparable = |left: &str, right: &str, same_graph: bool| {
        let string = |value: &str| matches!(value, "x1" | "x4" | "y1" | "y4");
        let problematic = |value: &str| matches!(value, "x5" | "x6" | "y5" | "y6");
        (string(left) && problematic(right))
            || (problematic(left) && string(right))
            || (problematic(left) && problematic(right) && (!same_graph || left != right))
    };
    let xs = ["x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8"];
    let expected_equal = xs
        .iter()
        .flat_map(|left| {
            xs.iter().filter_map(|right| {
                equality(left, right).then(|| ((*left).to_owned(), (*right).to_owned()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        pair_set(run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-07.rq")), "x1", "x2"),
        expected_equal,
        "open-eq-07-result.srx"
    );
    let expected_not_equal = xs
        .iter()
        .flat_map(|left| {
            xs.iter().filter_map(|right| {
                (!equality(left, right) && !incomparable(left, right, true))
                    .then(|| ((*left).to_owned(), (*right).to_owned()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        pair_set(run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-08.rq")), "x1", "x2"),
        expected_not_equal,
        "open-eq-08-result.srx"
    );
    assert!(run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-09.rq")).is_empty(), "open-eq-09-result.srx");
    let ys = ["y1", "y2", "y3", "y4", "y5", "y6", "y7", "y8"];
    let expected_cross_not_equal = xs
        .iter()
        .flat_map(|left| {
            ys.iter().filter_map(|right| {
                (!incomparable(left, right, false))
                    .then(|| ((*left).to_owned(), (*right).to_owned()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();
    for asset in ["open-eq-10.rq", "open-eq-11.rq"] {
        let query = match asset {
            "open-eq-10.rq" => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-10.rq"),
            _ => include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-11.rq"),
        };
        assert_eq!(
            pair_set(run(query), "x", "y"),
            expected_cross_not_equal,
            "{asset} result SRX"
        );
    }
    let expected_optional_errors = [
        ("x1", "x5"),
        ("x1", "x6"),
        ("x4", "x5"),
        ("x4", "x6"),
        ("x5", "x1"),
        ("x5", "x4"),
        ("x5", "x6"),
        ("x6", "x1"),
        ("x6", "x4"),
        ("x6", "x5"),
    ]
    .into_iter()
    .map(|(left, right)| (left.into(), right.into()))
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        pair_set(run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-eq-12.rq")), "x", "y"),
        expected_optional_errors,
        "open-eq-12-result.srx"
    );
}

#[test]
fn applies_ontop_dawg_open_world_date_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-3.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/data-3.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("open-world date action 应返回 bindings")
        };
        rows
    };
    let local = |term: &RdfTerm| match term {
        RdfTerm::Iri(iri) => iri.rsplit('/').next().unwrap_or(iri).to_owned(),
        other => panic!("预期 date subject IRI，实际为 {other:?}"),
    };
    for (asset, query, expected) in [
        ("date-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/date-2.rq"), ["d4", "d5", "dt1"].as_slice()),
        ("date-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/date-3.rq"), ["d1", "d2", "d3"].as_slice()),
        ("date-4.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/date-4.rq"), ["d6", "d7", "d8"].as_slice()),
    ] {
        let actual = run(query).into_iter().map(|row| local(row.get("x").expect("result 有 x"))).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(actual, expected.iter().map(|value| (*value).to_owned()).collect(), "{asset} result SRX");
    }
}

#[test]
fn applies_ontop_dawg_open_world_ordering_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-4.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/data-4.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("open-world comparison action 应返回 bindings")
        };
        rows.into_iter()
            .map(|row| match row.get("x") {
                Some(RdfTerm::Iri(iri)) => iri.rsplit('/').next().unwrap_or(iri).to_owned(),
                other => panic!("预期 x IRI，实际为 {other:?}"),
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-cmp-01.rq")),
        std::collections::BTreeSet::from(["x1".into()]),
        "open-cmp-01-result.srx"
    );
    assert_eq!(
        run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/open-world/open-cmp-02.rq")),
        std::collections::BTreeSet::from(["x1".into(), "x3".into(), "x4".into()]),
        "open-cmp-02-result.srx"
    );
}

#[test]
fn applies_ontop_dawg_optional_basic_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/data.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("optional action 应返回 bindings")
        };
        rows
    };
    let iri = |value: &str| RdfTerm::Iri(format!("mailto:{value}@example.net"));
    let simple = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    let mbox = |value: &str| ("mbox".into(), iri(value));
    let name = |value: &str| ("name".into(), simple(value));
    let nick = |value: &str| ("nick".into(), simple(value));
    for (asset, query, expected) in [
        ("q-opt-1.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-1.rq"), vec![
            std::collections::BTreeMap::from([mbox("alice"), name("Alice")]),
            std::collections::BTreeMap::from([mbox("bert"), name("Bert")]),
            std::collections::BTreeMap::from([mbox("eve")]),
        ]),
        ("q-opt-2.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-2.rq"), vec![
            std::collections::BTreeMap::from([mbox("alice"), name("Alice"), nick("WhoMe?")]),
            std::collections::BTreeMap::from([mbox("bert"), name("Bert")]),
            std::collections::BTreeMap::from([mbox("eve"), nick("DuckSoup")]),
        ]),
        ("q-opt-3.rq", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-3.rq"), vec![
            std::collections::BTreeMap::from([mbox("alice")]),
            std::collections::BTreeMap::from([mbox("bert")]),
            std::collections::BTreeMap::from([mbox("eve")]),
            std::collections::BTreeMap::from([mbox("alice"), name("Alice")]),
            std::collections::BTreeMap::from([mbox("bert"), name("Bert")]),
        ]),
    ] {
        let actual = run(query);
        assert_eq!(actual.len(), expected.len(), "{asset} result TTL: {actual:?}");
        for binding in expected {
            assert!(actual.contains(&binding), "{asset} 缺少 {binding:?}: {actual:?}");
        }
    }
}

#[test]
fn applies_ontop_dawg_optional_filter_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("data-1.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional-filter/data-1.ttl")).unwrap();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("optional-filter action 应返回 bindings")
        };
        rows
    };
    let title = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    let price = RdfTerm::Literal {
        value: "10".into(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
        language: None,
    };
    let binding = |title_value: &str, price_value: Option<RdfTerm>| {
        let mut row = std::collections::BTreeMap::from([("title".into(), title(title_value))]);
        if let Some(price_value) = price_value {
            row.insert("price".into(), price_value);
        }
        row
    };
    for (asset, query, expected) in [
        (
            "expr-1.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional-filter/expr-1.rq"),
            vec![binding("TITLE 1", Some(price.clone())), binding("TITLE 2", None), binding("TITLE 3", None)],
        ),
        (
            "expr-2.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional-filter/expr-2.rq"),
            vec![binding("TITLE 1", Some(price.clone()))],
        ),
        (
            "expr-3.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional-filter/expr-3.rq"),
            vec![binding("TITLE 1", Some(price.clone())), binding("TITLE 3", None)],
        ),
        (
            "expr-4.rq",
            include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional-filter/expr-4.rq"),
            vec![binding("TITLE 1", None), binding("TITLE 2", None), binding("TITLE 3", None)],
        ),
    ] {
        let actual = run(query);
        assert_eq!(actual.len(), expected.len(), "{asset} result TTL: {actual:?}");
        for row in expected {
            assert!(actual.contains(&row), "{asset} 缺少 {row:?}: {actual:?}");
        }
    }
}

#[test]
fn applies_ontop_dawg_sort_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts_content: &str, query: &str| {
        let facts = dir.path().join(facts_name);
        std::fs::write(&facts, facts_content).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("sort action 应返回 bindings")
        };
        rows
    };
    let value = |term: &RdfTerm| match term {
        RdfTerm::Iri(iri) => iri.clone(),
        RdfTerm::Literal { value, .. } => value.clone(),
        RdfTerm::BlankNode(label) => format!("_:{label}"),
    };
    for (asset, facts_name, facts_content, query, variable, expected) in [
        ("sort-1", "data-sort-1.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-1.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-1.rq"), "name", vec!["Alice", "Bob", "Eve", "Fred"]),
        ("sort-2", "data-sort-1.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-1.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-2.rq"), "name", vec!["Fred", "Eve", "Bob", "Alice"]),
        ("sort-3", "data-sort-3.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-3.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-3.rq"), "name", vec!["Bob", "Alice", "Eve", "Fred"]),
        ("sort-4", "data-sort-4.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-4.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-4.rq"), "name", vec!["Eve", "Bob", "Fred", "Alice", "Bob"]),
        ("sort-5", "data-sort-4.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-4.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-5.rq"), "name", vec!["Alice", "Bob", "Bob", "Eve", "Fred"]),
        ("sort-6", "data-sort-6.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-6.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-6.rq"), "address", vec!["http://example.org/eve", "mailto:bob@work.example", "Fascination Street 11", "fred@work.example"]),
        ("sort-7", "data-sort-7.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-7.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-4.rq"), "name", vec!["Eve", "Bob", "Fred", "Alice"]),
        ("sort-8", "data-sort-8.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-8.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-4.rq"), "name", vec!["John", "Dirk", "Eve"]),
        ("sort-9", "data-sort-9.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-9.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-9.rq"), "name", vec!["Alice", "Bob", "Eve", "Fred"]),
        ("sort-10", "data-sort-9.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-9.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-10.rq"), "name", vec!["Fred", "Eve", "Bob", "Alice"]),
        ("sort-numbers", "data-sort-numbers.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-numbers.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-numbers.rq"), "s", vec!["http://example.org/s1", "http://example.org/s2", "http://example.org/s3"]),
        ("sort-builtin", "data-sort-builtin.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-builtin.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-builtin.rq"), "s", vec!["http://example.org/s3", "http://example.org/s1", "http://example.org/s2"]),
        ("sort-function", "data-sort-function.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/data-sort-function.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/sort/query-sort-function.rq"), "s", vec!["http://example.org/s1", "http://example.org/s3", "http://example.org/s2"]),
    ] {
        let actual = run(facts_name, facts_content, query)
            .iter()
            .map(|row| value(row.get(variable).expect("排序投影变量缺失")))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{asset} 原始 result asset 的顺序");
    }
}

#[test]
fn applies_ontop_dawg_dataset_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("dataset.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(
        &facts,
        r#"<http://example/x> <http://example/p> "1"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g1.ttl> .
<http://example/a> <http://example/p> "9"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g1.ttl> .
<http://example/x> <http://example/q> "2"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g2.ttl> .
_:g3n_x <http://example/p> "1"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3.ttl> .
_:g3n_a <http://example/p> "9"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3.ttl> .
_:g4n_x <http://example/q> "2"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g4.ttl> .
<http://example/x> <http://example/p> "1"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g1-dup.ttl> .
<http://example/a> <http://example/p> "9"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g1-dup.ttl> .
<http://example/x> <http://example/q> "2"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g2-dup.ttl> .
_:g3d_x <http://example/p> "1"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3-dup.ttl> .
_:g3d_a <http://example/p> "9"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3-dup.ttl> .
_:g4d_x <http://example/q> "2"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g4-dup.ttl> .
"#,
    )
    .unwrap();
    let rewrite_document_iris = |query: &str| {
        [
            "data-g1-dup.ttl",
            "data-g2-dup.ttl",
            "data-g3-dup.ttl",
            "data-g4-dup.ttl",
            "data-g1.ttl",
            "data-g2.ttl",
            "data-g3.ttl",
            "data-g4.ttl",
        ]
        .into_iter()
        .fold(query.to_owned(), |query, document| {
            query.replace(
                &format!("<{document}>"),
                &format!("<https://example.test/{document}>"),
            )
        })
    };
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) =
            runtime.query(&rewrite_document_iris(query)).unwrap()
        else {
            panic!("dataset action 应返回 bindings")
        };
        rows
    };
    for (asset, query, count) in [
        ("dataset-01", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-01.rq"), 2),
        ("dataset-02", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-02.rq"), 0),
        ("dataset-03", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-03.rq"), 2),
        ("dataset-04", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-04.rq"), 0),
        ("dataset-05", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-05.rq"), 2),
        ("dataset-06", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-06.rq"), 1),
        ("dataset-07", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-07.rq"), 3),
        ("dataset-08", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-08.rq"), 1),
        ("dataset-09b", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-09b.rq"), 0),
        ("dataset-10b", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-10b.rq"), 0),
        ("dataset-11", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-11.rq"), 8),
        ("dataset-12b", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-12b.rq"), 12),
    ] {
        assert_eq!(run(query).len(), count, "{asset} result TTL cardinality");
    }
    let dataset_06 = run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/dataset/dataset-06.rq"));
    assert_eq!(
        dataset_06[0].get("g"),
        Some(&RdfTerm::Iri("https://example.test/data-g2.ttl".into())),
        "FROM NAMED 只允许声明的 named graph"
    );
}

#[test]
fn applies_ontop_dawg_graph_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let g1_default = "<http://example/x> <http://example/p> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n<http://example/a> <http://example/p> \"9\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n";
    let g3_default = "_:g3d_x <http://example/p> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n_:g3d_a <http://example/p> \"9\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n";
    let named_g1 = "<http://example/x> <http://example/p> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g1.ttl> .\n<http://example/a> <http://example/p> \"9\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g1.ttl> .\n";
    let named_g2 = "<http://example/x> <http://example/q> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g2.ttl> .\n";
    let named_g3 = "_:g3n_x <http://example/p> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3.ttl> .\n_:g3n_a <http://example/p> \"9\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3.ttl> .\n";
    let named_g3_dup = "_:g3n2_x <http://example/p> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3-dup.ttl> .\n_:g3n2_a <http://example/p> \"9\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g3-dup.ttl> .\n";
    let named_g4 = "_:g4n_x <http://example/q> \"2\"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/data-g4.ttl> .\n";
    let run = |default: &str, named: &[&str], query: &str| {
        let facts = dir.path().join("graph.nq");
        std::fs::write(&facts, format!("{default}{}", named.concat())).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("graph action 应返回 bindings")
        };
        rows
    };
    for (asset, default, named, query, count) in [
        ("graph-01", g1_default, vec![], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-01.rq"), 2),
        ("graph-02", "", vec![named_g1], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-02.rq"), 0),
        ("graph-03", "", vec![named_g1], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-03.rq"), 2),
        ("graph-04", g1_default, vec![], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-04.rq"), 0),
        ("graph-05", g1_default, vec![named_g2], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-05.rq"), 2),
        ("graph-06", g1_default, vec![named_g2], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-06.rq"), 1),
        ("graph-07", g1_default, vec![named_g2], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-07.rq"), 3),
        ("graph-08", g1_default, vec![named_g2], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-08.rq"), 1),
        ("graph-09", g3_default, vec![named_g4], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-09.rq"), 0),
        ("graph-10b", g3_default, vec![named_g3_dup], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-10.rq"), 0),
        ("graph-11", g1_default, vec![named_g1, named_g2, named_g3, named_g4], include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/graph/graph-11.rq"), 8),
    ] {
        assert_eq!(run(default, &named, query).len(), count, "{asset} result TTL cardinality");
    }
}

#[test]
fn applies_ontop_dawg_algebra_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run_turtle = |facts_name: &str, facts_content: &str, query: &str| {
        let facts = dir.path().join(facts_name);
        std::fs::write(&facts, facts_content).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("algebra action 应返回 bindings")
        };
        rows
    };
    for (asset, facts_name, facts_content, query, count) in [
        ("nested-opt-1", "two-nested-opt.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/two-nested-opt.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/two-nested-opt.rq"), 1),
        ("nested-opt-2", "two-nested-opt.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/two-nested-opt.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/two-nested-opt-alt.rq"), 2),
        ("opt-filter-1", "opt-filter-1.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/opt-filter-1.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/opt-filter-1.rq"), 3),
        ("opt-filter-2", "opt-filter-2.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/opt-filter-2.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/opt-filter-2.rq"), 2),
        ("opt-filter-3", "opt-filter-3.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/opt-filter-3.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/opt-filter-3.rq"), 0),
        ("filter-place-1", "data-2.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/data-2.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/filter-placement-1.rq"), 1),
        ("filter-place-2", "data-2.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/data-2.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/filter-placement-2.rq"), 1),
        ("filter-place-3", "data-2.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/data-2.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/filter-placement-3.rq"), 1),
        ("filter-nested-1", "data-1.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/data-1.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/filter-nested-1.rq"), 1),
        ("filter-nested-2", "data-1.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/data-1.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/filter-nested-2.rq"), 0),
        ("filter-scope-1", "data-2.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/data-2.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/filter-scope-1.rq"), 12),
        ("join-scope-1", "var-scope-join-1.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/var-scope-join-1.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/var-scope-join-1.rq"), 0),
        ("join-combo-1", "join-combo-graph-2.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/join-combo-graph-2.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/join-combo-1.rq"), 2),
    ] {
        assert_eq!(run_turtle(facts_name, facts_content, query).len(), count, "{asset} result SRX cardinality");
    }
    let facts = dir.path().join("join-combo-2.nq");
    std::fs::write(&facts, r#"<http://example/b> <http://example/p> "1"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/graph-1> .
_:named_a <http://example/p> "9"^^<http://www.w3.org/2001/XMLSchema#integer> <https://example.test/graph-1> .
<http://example/x1> <http://example/p> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example/p> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Property> .
"#).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/algebra/join-combo-2.rq")).unwrap() else { panic!("join-combo-2 应返回 bindings") };
    assert_eq!(rows.len(), 1, "join-combo-2.srx");
}

#[test]
fn applies_ontop_dawg_expr_builtin_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts_content: &str, query: &str| {
        let facts = dir.path().join(facts_name);
        std::fs::write(&facts, facts_content).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: Some("http://example.org/#".into()),
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("expr-builtin action 应返回 bindings")
        };
        rows
    };
    let builtin_1 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/data-builtin-1.ttl");
    let builtin_2 = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/data-builtin-2.ttl");
    let lang_matches = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/data-langMatches.ttl");
    let lang_matches_de = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/data-langMatches-de.ttl");
    let lang_case = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/lang-case-sensitivity.ttl");
    for (asset, facts_name, facts_content, query, count) in [
        ("str-1", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-str-1.rq"), 4),
        ("str-2", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-str-2.rq"), 1),
        ("str-3", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-str-3.rq"), 2),
        ("str-4", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-str-4.rq"), 1),
        ("isBlank", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-blank-1.rq"), 1),
        ("isLiteral", "data-builtin-2.ttl", builtin_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-isliteral-1.rq"), 5),
        ("datatype-1", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-datatype-1.rq"), 3),
        ("datatype-2", "data-builtin-2.ttl", builtin_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-datatype-2.rq"), 5),
        ("datatype-3", "data-builtin-2.ttl", builtin_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-datatype-3.rq"), 2),
        ("lang-1", "data-builtin-2.ttl", builtin_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-lang-1.rq"), 5),
        ("lang-2", "data-builtin-2.ttl", builtin_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-lang-2.rq"), 4),
        ("lang-3", "data-builtin-2.ttl", builtin_2, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-lang-3.rq"), 1),
        ("isURI", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-uri-1.rq"), 1),
        ("isIRI", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-iri-1.rq"), 1),
        ("langMatches-1", "data-langMatches.ttl", lang_matches, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-langMatches-1.rq"), 1),
        ("langMatches-2", "data-langMatches.ttl", lang_matches, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-langMatches-2.rq"), 2),
        ("langMatches-3", "data-langMatches.ttl", lang_matches, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-langMatches-3.rq"), 3),
        ("langMatches-4", "data-langMatches.ttl", lang_matches, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-langMatches-4.rq"), 1),
        ("langMatches-basic", "data-langMatches-de.ttl", lang_matches_de, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/q-langMatches-de-de.rq"), 1),
        ("lang-case-eq", "lang-case.ttl", lang_case, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/lang-case-sensitivity-eq.rq"), 4),
        ("lang-case-ne", "lang-case.ttl", lang_case, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/lang-case-sensitivity-ne.rq"), 0),
        ("sameTerm", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/sameTerm.rq"), 14),
        ("sameTerm-eq", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/sameTerm-eq.rq"), 14),
        ("sameTerm-not-eq", "data-builtin-1.ttl", builtin_1, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/expr-builtin/sameTerm-not-eq.rq"), 28),
    ] {
        let actual = run(facts_name, facts_content, query);
        assert_eq!(actual.len(), count, "{asset} result asset cardinality: {actual:?}");
    }
}

#[test]
fn applies_ontop_dawg_optional_complex_1_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("complex-data-1.ttl");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/complex-data-1.ttl")).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-complex-1.rq")).unwrap() else {
        panic!("complex optional 1 应返回 bindings")
    };
    let iri = |value: &str| RdfTerm::Iri(value.into());
    let simple = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    let expected = vec![
        std::collections::BTreeMap::from([
            ("person".into(), iri("tag:alice@example:foafUri")),
            ("nick".into(), simple("WhoMe?")),
            ("name".into(), simple("Alice")),
            ("img".into(), iri("http://example.com/alice.png")),
        ]),
        std::collections::BTreeMap::from([
            ("person".into(), iri("tag:john@example:foafUri")),
            ("nick".into(), simple("jDoe")),
            ("page".into(), iri("http://example.com/people/johnDoe")),
        ]),
    ];
    assert_eq!(
        rows.len(),
        expected.len(),
        "result-opt-complex-1.ttl: {rows:?}"
    );
    for binding in expected {
        assert!(rows.contains(&binding), "缺少 {binding:?}: {rows:?}");
    }
}

#[test]
fn applies_ontop_dawg_optional_complex_2_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("complex.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, r#"_:eve <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:eve <http://xmlns.com/foaf/0.1/name> "Eve" .
_:eve <http://example.org/things#empId> "9"^^<http://www.w3.org/2001/XMLSchema#integer> .
_:alice <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:alice <http://xmlns.com/foaf/0.1/name> "Alice" .
_:alice <http://example.org/things#empId> "29"^^<http://www.w3.org/2001/XMLSchema#integer> .
_:fred <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:fred <http://xmlns.com/foaf/0.1/name> "Fred" .
_:bert <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:bert <http://xmlns.com/foaf/0.1/name> "Bert" .
_:bert <http://example.org/things#ssn> "000000000" .
_:named_alice <http://xmlns.com/foaf/0.1/name> "Alice" <https://example.test/complex-data-1> .
_:named_alice <http://xmlns.com/foaf/0.1/nick> "WhoMe?" <https://example.test/complex-data-1> .
_:named_bert <http://xmlns.com/foaf/0.1/name> "Bert" <https://example.test/complex-data-1> .
_:named_bert <http://xmlns.com/foaf/0.1/nick> "BigB" <https://example.test/complex-data-1> .
"#).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-complex-2.rq")).unwrap() else {
        panic!("complex optional 2 应返回 bindings")
    };
    assert_eq!(rows.len(), 2, "result-opt-complex-2.ttl: {rows:?}");
    assert!(
        rows.contains(&std::collections::BTreeMap::from([(
            "id".into(),
            RdfTerm::Literal {
                value: "29".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into()),
                language: None,
            }
        )])),
        "缺少 Alice id result：{rows:?}"
    );
    assert!(
        rows.contains(&std::collections::BTreeMap::from([(
            "ssn".into(),
            RdfTerm::Literal {
                value: "000000000".into(),
                datatype: None,
                language: None,
            }
        )])),
        "缺少 Bert ssn result：{rows:?}"
    );
}

#[test]
fn applies_ontop_dawg_optional_complex_3_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("complex.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, r#"_:alice <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:alice <http://xmlns.com/foaf/0.1/name> "Alice" .
_:alice <http://example.org/things#healthplan> <http://example.org/things#HealthPlanD> .
_:bert <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:bert <http://xmlns.com/foaf/0.1/name> "Bert" .
_:bert <http://example.org/things#healthplan> <http://example.org/things#HealthPlanA> .
_:bert <http://example.org/things#department> "DeptA" .
_:named_alice <http://xmlns.com/foaf/0.1/name> "Alice" <https://example.test/complex-data-1> .
_:named_alice <http://xmlns.com/foaf/0.1/nick> "WhoMe?" <https://example.test/complex-data-1> .
_:named_bert <http://xmlns.com/foaf/0.1/name> "Bert" <https://example.test/complex-data-1> .
_:named_bert <http://xmlns.com/foaf/0.1/nick> "BigB" <https://example.test/complex-data-1> .
"#).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-complex-3.rq")).unwrap() else {
        panic!("complex optional 3 应返回 bindings")
    };
    let simple = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    let iri = |value: &str| RdfTerm::Iri(format!("http://example.org/things#{value}"));
    let expected = vec![
        std::collections::BTreeMap::from([
            ("name".into(), simple("Alice")),
            ("nick".into(), simple("WhoMe?")),
            ("plan".into(), iri("HealthPlanD")),
        ]),
        std::collections::BTreeMap::from([
            ("name".into(), simple("Bert")),
            ("nick".into(), simple("BigB")),
            ("plan".into(), iri("HealthPlanA")),
            ("dept".into(), simple("DeptA")),
        ]),
    ];
    assert_eq!(
        rows.len(),
        expected.len(),
        "result-opt-complex-3.ttl: {rows:?}"
    );
    for binding in expected {
        assert!(rows.contains(&binding), "缺少 {binding:?}: {rows:?}");
    }
}

#[test]
fn applies_ontop_dawg_optional_complex_4_manifest_asset() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    let facts = dir.path().join("complex.nq");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    std::fs::write(&facts, r#"_:alice <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:alice <http://xmlns.com/foaf/0.1/name> "Alice" .
_:alice <http://example.org/things#healthplan> <http://example.org/things#HealthPlanD> .
_:bob_c <http://xmlns.com/foaf/0.1/name> "Bob" .
_:bob_c <http://example.org/things#healthplan> <http://example.org/things#HealthPlanC> .
_:bob_b <http://xmlns.com/foaf/0.1/name> "Bob" .
_:bob_b <http://example.org/things#healthplan> <http://example.org/things#HealthPlanB> .
_:bert <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://xmlns.com/foaf/0.1/Person> .
_:bert <http://xmlns.com/foaf/0.1/name> "Bert" .
_:bert <http://example.org/things#healthplan> <http://example.org/things#HealthPlanA> .
_:bert <http://example.org/things#department> "DeptA" .
_:named_alice <http://xmlns.com/foaf/0.1/name> "Alice" <https://example.test/complex-data-1> .
_:named_alice <http://xmlns.com/foaf/0.1/depiction> <http://example.com/alice.png> <https://example.test/complex-data-1> .
"#).unwrap();
    let mut runtime = VkgRuntime::new(
        KnowledgeGraphSpec {
            mapping_file: mapping,
            facts_file: Some(facts),
            facts_format: None,
            facts_base_iri: None,
            ontology_file: None,
            xml_catalog_file: None,
        },
        NullSource,
    )
    .unwrap();
    let rtop::QueryResult::Bindings(rows) = runtime.query(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/optional/q-opt-complex-4.rq")).unwrap() else {
        panic!("complex optional 4 应返回 bindings")
    };
    let simple = |value: &str| RdfTerm::Literal {
        value: value.into(),
        datatype: None,
        language: None,
    };
    let iri = |value: &str| RdfTerm::Iri(format!("http://example.org/things#{value}"));
    let expected = vec![
        std::collections::BTreeMap::from([
            ("name".into(), simple("Alice")),
            ("plan".into(), iri("HealthPlanD")),
            (
                "img".into(),
                RdfTerm::Iri("http://example.com/alice.png".into()),
            ),
        ]),
        std::collections::BTreeMap::from([
            ("name".into(), simple("Bob")),
            ("plan".into(), iri("HealthPlanC")),
        ]),
        std::collections::BTreeMap::from([
            ("name".into(), simple("Bob")),
            ("plan".into(), iri("HealthPlanB")),
        ]),
        std::collections::BTreeMap::from([
            ("name".into(), simple("Bert")),
            ("plan".into(), iri("HealthPlanA")),
        ]),
        std::collections::BTreeMap::from([
            ("name".into(), simple("Bert")),
            ("dept".into(), simple("DeptA")),
        ]),
    ];
    assert_eq!(
        rows.len(),
        expected.len(),
        "result-opt-complex-4.ttl: {rows:?}"
    );
    for binding in expected {
        assert!(rows.contains(&binding), "缺少 {binding:?}: {rows:?}");
    }
}

#[test]
fn applies_ontop_dawg_distinct_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts_content: &str, query: &str| {
        let facts = dir.path().join(facts_name);
        std::fs::write(&facts, facts_content).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Bindings(rows) = runtime.query(query).unwrap() else {
            panic!("distinct action 应返回 bindings")
        };
        rows
    };
    let no_distinct = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/no-distinct-1.rq");
    let distinct = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/distinct-1.rq");
    for (facts_name, facts_content, plain_count, distinct_count) in [
        ("data-num.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/data-num.ttl"), 22, 9),
        ("data-str.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/data-str.ttl"), 18, 9),
        ("data-node.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/data-node.ttl"), 4, 2),
        ("data-all.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/data-all.ttl"), 44, 20),
    ] {
        let plain = run(facts_name, facts_content, no_distinct);
        let deduplicated = run(facts_name, facts_content, distinct);
        assert_eq!(plain.len(), plain_count, "{facts_name} no-distinct SRX");
        assert_eq!(deduplicated.len(), distinct_count, "{facts_name} distinct SRX");
        assert_eq!(plain.iter().collect::<std::collections::BTreeSet<_>>().len(), distinct_count, "{facts_name} RDF term identity distinctness");
    }
    let data_opt = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/data-opt.ttl");
    let opt_plain = run("data-opt.ttl", data_opt, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/no-distinct-2.rq"));
    let opt_distinct = run("data-opt.ttl", data_opt, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/distinct-2.rq"));
    assert_eq!(opt_plain.len(), 6, "no-distinct-opt.srx");
    assert_eq!(opt_distinct.len(), 3, "distinct-opt.srx");
    assert!(
        opt_distinct
            .iter()
            .any(std::collections::BTreeMap::is_empty),
        "distinct-opt.srx 应保留未绑定 ?v mapping"
    );
    let star = run("data-star.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/data-star.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/distinct/distinct-star-1.rq"));
    assert_eq!(star.len(), 2, "distinct-star-1.srx");
    assert!(
        star.iter()
            .all(|row| row.contains_key("s") && row.contains_key("o")),
        "SELECT DISTINCT * 应投影完整 binding：{star:?}"
    );
}

#[test]
fn applies_ontop_dawg_construct_identity_subgraph_optional_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let run = |facts_name: &str, facts_content: &str, query: &str| {
        let facts = dir.path().join(facts_name);
        std::fs::write(&facts, facts_content).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Graph(graph) = runtime.query(query).unwrap() else {
            panic!("CONSTRUCT action 应返回 RDF graph")
        };
        graph
    };
    let data_ident = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/data-ident.ttl");
    let identity = run("data-ident.ttl", data_ident, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/query-ident.rq"));
    assert_eq!(
        identity.len(),
        9,
        "result-ident.ttl triple count: {identity:?}"
    );
    let knows = "http://xmlns.com/foaf/0.1/knows";
    let alice_bob = identity
        .iter()
        .find_map(|fact| match (&fact.subject, &fact.object) {
            (left @ RdfTerm::BlankNode(_), right @ RdfTerm::BlankNode(_))
                if fact.predicate == knows && left != right =>
            {
                Some((left.clone(), right.clone()))
            }
            _ => None,
        })
        .expect("identity construct 应保留 Alice/Bob blank-node 关系");
    assert!(
        identity.iter().any(|fact| fact.subject == alice_bob.1
            && fact.predicate == knows
            && fact.object == alice_bob.0),
        "result-ident.ttl 应保留 reciprocal knows"
    );
    let subgraph = run("data-ident.ttl", data_ident, include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/query-subgraph.rq"));
    assert_eq!(subgraph.len(), 2, "result-subgraph.ttl: {subgraph:?}");
    assert!(
        subgraph
            .iter()
            .all(|fact| fact.predicate == "http://xmlns.com/foaf/0.1/name"),
        "subgraph construct 只能产生 foaf:name: {subgraph:?}"
    );
    let optional = run("data-opt.ttl", include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/data-opt.ttl"), include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/query-construct-optional.rq"));
    assert_eq!(
        optional.len(),
        1,
        "result-construct-optional.ttl: {optional:?}"
    );
    assert!(matches!(&optional[0], rtop::RdfFact {
        subject: RdfTerm::Iri(subject), predicate, object: RdfTerm::Literal { value, datatype: Some(datatype), language: None }, graph: None
    } if subject == "http://example/x" && predicate == "http://example/p2" && value == "2" && datatype == "http://www.w3.org/2001/XMLSchema#integer"));
}

#[test]
fn applies_ontop_sparql11_construct_where_manifest_assets() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/construct",
    );
    let temp = tempfile::tempdir().unwrap();
    let mapping = temp.path().join("empty.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/unused/{id}> <https://example.test/type> <https://example.test/Unused> .\nsource SELECT id FROM unused\n",
    )
    .unwrap();
    let facts = temp.path().join("data.ttl");
    std::fs::write(
        &facts,
        std::fs::read_to_string(directory.join("data.ttl")).unwrap(),
    )
    .unwrap();

    let graph_terms = |facts: Vec<rtop::RdfFact>| {
        facts
            .into_iter()
            .map(|fact| (fact.subject, fact.predicate, fact.object, fact.graph))
            .collect::<std::collections::BTreeSet<_>>()
    };
    for asset in [
        "constructwhere01",
        "constructwhere02",
        "constructwhere03",
        "constructwhere04",
    ] {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Graph(actual) = runtime
            .query(&std::fs::read_to_string(directory.join(format!("{asset}.rq"))).unwrap())
            .unwrap_or_else(|error| panic!("{asset} 应执行：{error}"))
        else {
            panic!("{asset} 应返回 RDF graph")
        };
        let expected = rtop::parse_turtle(
            std::fs::read(directory.join(format!("{asset}result.ttl")))
                .unwrap()
                .as_slice(),
            None,
        )
        .unwrap();
        assert_eq!(
            graph_terms(actual),
            graph_terms(expected),
            "{asset} 应匹配原始 result TTL"
        );
    }

    for asset in ["constructwhere05.rq", "constructwhere06.rq"] {
        let input = std::fs::read_to_string(directory.join(asset)).unwrap();
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        assert!(runtime.query(&input).is_err(), "{asset} 必须被拒绝");
    }
}

#[test]
fn applies_ontop_dawg_construct_reification_manifest_assets() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("x.obda");
    std::fs::write(&mapping, "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n").unwrap();
    let data = include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/data-reif.ttl");
    let facts = dir.path().join("data-reif.ttl");
    std::fs::write(&facts, data).unwrap();
    let expected = rtop::parse_turtle(data, None)
        .unwrap()
        .into_iter()
        .map(|fact| (fact.subject, RdfTerm::Iri(fact.predicate), fact.object))
        .collect::<std::collections::BTreeSet<_>>();
    let run = |query: &str| {
        let mut runtime = VkgRuntime::new(
            KnowledgeGraphSpec {
                mapping_file: mapping.clone(),
                facts_file: Some(facts.clone()),
                facts_format: None,
                facts_base_iri: None,
                ontology_file: None,
                xml_catalog_file: None,
            },
            NullSource,
        )
        .unwrap();
        let rtop::QueryResult::Graph(graph) = runtime.query(query).unwrap() else {
            panic!("reification CONSTRUCT action 应返回 RDF graph")
        };
        graph
    };
    let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let reification_predicates = [
        format!("{rdf}subject"),
        format!("{rdf}predicate"),
        format!("{rdf}object"),
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    let normalize = |graph: Vec<rtop::RdfFact>| {
        assert_eq!(
            graph.len(),
            24,
            "result-reif.ttl 应有 24 triples: {graph:?}"
        );
        let mut groups = std::collections::BTreeMap::<RdfTerm, Vec<rtop::RdfFact>>::new();
        for fact in graph {
            assert_eq!(fact.graph, None, "reification result 应位于 default graph");
            assert!(
                matches!(fact.subject, RdfTerm::BlankNode(_)),
                "每个 reification resource 必须是 blank node: {fact:?}"
            );
            groups.entry(fact.subject.clone()).or_default().push(fact);
        }
        assert_eq!(
            groups.len(),
            8,
            "每个 solution mapping 都须使用新鲜 blank node"
        );
        let mut sources = std::collections::BTreeSet::new();
        for facts in groups.into_values() {
            assert_eq!(
                facts.len(),
                3,
                "每个 reification blank node 必须恰有三条属性"
            );
            let predicates = facts
                .iter()
                .map(|fact| fact.predicate.clone())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(predicates, reification_predicates);
            let subject = facts
                .iter()
                .find(|fact| fact.predicate == format!("{rdf}subject"))
                .unwrap()
                .object
                .clone();
            let predicate = facts
                .iter()
                .find(|fact| fact.predicate == format!("{rdf}predicate"))
                .unwrap()
                .object
                .clone();
            let object = facts
                .iter()
                .find(|fact| fact.predicate == format!("{rdf}object"))
                .unwrap()
                .object
                .clone();
            sources.insert((subject, predicate, object));
        }
        assert_eq!(
            sources, expected,
            "reification graph 应覆盖全部输入 triples"
        );
        sources
    };
    let property_list = normalize(run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/query-reif-1.rq")));
    let labeled_blank_node = normalize(run(include_str!("../../ontop/test/sparql-compliance/src/test/resources/testcases-dawg/data-r2/construct/query-reif-2.rq")));
    assert_eq!(property_list, labeled_blank_node);
}

#[test]
fn routes_buffered_geosparql_intersection_through_the_postgres_adapter_port() {
    struct GeospatialSource;
    impl DataSource for GeospatialSource {
        fn execute(
            &mut self,
            _: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            Ok(Vec::new())
        }

        fn geospatial(
            &mut self,
            name: &str,
            arguments: &[String],
        ) -> Result<Option<rtop::GeospatialValue>, RuntimeError> {
            assert!(name.to_ascii_uppercase().contains("BUFFER"), "{name}");
            assert_eq!(
                arguments,
                [
                    "POINT(0 0)",
                    "20",
                    "http://www.opengis.net/def/uom/OGC/1.0/metre"
                ]
            );
            Ok(Some(rtop::GeospatialValue::Wkt("BUFFERED".into())))
        }

        fn geospatial_intersection_with_buffer(
            &mut self,
            buffered_wkt: &str,
            distance: &str,
            other_wkt: &str,
        ) -> Result<Option<rtop::GeospatialValue>, RuntimeError> {
            assert_eq!(buffered_wkt, "POINT(0 0)");
            assert_eq!(distance, "20");
            assert_eq!(other_wkt, "POINT(1 1)");
            Ok(Some(rtop::GeospatialValue::Wkt("INTERSECTION".into())))
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
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
    let mut runtime = VkgRuntime::new(spec, GeospatialSource).unwrap();
    let result = runtime
        .query(
            "PREFIX geof: <http://www.opengis.net/def/function/geosparql/>\n\
             PREFIX uom: <http://www.opengis.net/def/uom/OGC/1.0/>\n\
             SELECT ?result WHERE {\n\
             VALUES (?x ?y) {\n\
                 (\"POINT(0 0)\"^^<http://www.opengis.net/ont/geosparql#wktLiteral>\n\
                  \"POINT(1 1)\"^^<http://www.opengis.net/ont/geosparql#wktLiteral>)\n\
             }\n\
             BIND(geof:buffer(?x, 20, uom:metre) AS ?buffer)\n\
             BIND(geof:intersection(?buffer, ?y) AS ?result)\n\
             }",
        )
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
            if matches!(rows[0].get("result"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value == "INTERSECTION" && datatype == "http://www.opengis.net/ont/geosparql#wktLiteral")),
        "{result:?}"
    );
}

#[test]
fn routes_unbuffered_geosparql_operations_through_the_generic_adapter_port() {
    struct GeospatialSource;
    impl DataSource for GeospatialSource {
        fn execute(
            &mut self,
            _: &str,
            _: &[String],
        ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
            Ok(Vec::new())
        }

        fn geospatial(
            &mut self,
            name: &str,
            arguments: &[String],
        ) -> Result<Option<rtop::GeospatialValue>, RuntimeError> {
            let name = name.to_ascii_uppercase();
            assert_eq!(arguments, ["POINT(0 0)", "POINT(1 1)"]);
            if name.contains("INTERSECTION") {
                return Ok(Some(rtop::GeospatialValue::Wkt("GENERIC".into())));
            }
            assert!(name.contains("SFINTERSECTS"), "{name}");
            Ok(Some(rtop::GeospatialValue::Boolean(true)))
        }

        fn geospatial_intersection_with_buffer(
            &mut self,
            _: &str,
            _: &str,
            _: &str,
        ) -> Result<Option<rtop::GeospatialValue>, RuntimeError> {
            panic!("没有 preceding buffer 时不得调用专用 intersection port")
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
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
    let mut runtime = VkgRuntime::new(spec, GeospatialSource).unwrap();
    let result = runtime
        .query(
            "PREFIX geof: <http://www.opengis.net/def/function/geosparql/>\n\
             SELECT ?intersection ?touches WHERE {\n\
             VALUES (?x ?y) {\n\
                 (\"POINT(0 0)\"^^<http://www.opengis.net/ont/geosparql#wktLiteral>\n\
                  \"POINT(1 1)\"^^<http://www.opengis.net/ont/geosparql#wktLiteral>)\n\
             }\n\
             BIND(geof:intersection(?x, ?y) AS ?intersection)\n\
             BIND(geof:sfIntersects(?x, ?y) AS ?touches)\n\
             }",
        )
        .unwrap();
    assert!(
        matches!(result, rtop::QueryResult::Bindings(ref rows)
            if matches!(rows[0].get("intersection"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value == "GENERIC" && datatype == "http://www.opengis.net/ont/geosparql#wktLiteral")
            && matches!(rows[0].get("touches"), Some(RdfTerm::Literal { value, datatype: Some(datatype), language: None }) if value == "true" && datatype == "http://www.w3.org/2001/XMLSchema#boolean")),
        "{result:?}"
    );
}

#[test]
fn rejects_invalid_sparql_terms_before_runtime_planning() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("empty.obda");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/row/{id}> <https://example.test/value> {value} .\nsource SELECT id, value FROM rows\n",
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

    for query in [
        "SELECT * WHERE { ?s [] ?o }",
        "SELECT * WHERE { GRAPH _:graph { ?s ?p ?o } }",
        "SELECT * WHERE { ?s ?p ?o FILTER(_:term = ?o) }",
        "SELECT * WHERE { BIND(\"not-a-boolean\"^^<http://www.w3.org/2001/XMLSchema#boolean> AS ?value) }",
        "SELECT * WHERE { BIND(\"not-a-date\"^^<http://www.w3.org/2001/XMLSchema#dateTime> AS ?value) }",
    ] {
        assert!(
            matches!(runtime.query(query), Err(RuntimeError::MalformedSparql(_)) | Err(RuntimeError::Type(_))),
            "应在 parser boundary 拒绝：{query}"
        );
    }
}
