use rtop::{
    DataSource, DataValue, GeospatialValue, PostgresConnectionConfig, PostgresDataSource,
    RelationColumn, RelationConstraints, RelationForeignKey, RelationMetadata, RuntimeError,
    StreamControl,
};
use std::time::Duration;

fn postgres_config() -> Option<PostgresConnectionConfig> {
    Some(PostgresConnectionConfig {
        host: std::env::var("RTOP_POSTGRES_HOST").ok()?,
        port: std::env::var("RTOP_POSTGRES_PORT").ok()?.parse().ok()?,
        database: std::env::var("RTOP_POSTGRES_DATABASE").ok()?,
        user: std::env::var("RTOP_POSTGRES_USER").ok()?,
        password: std::env::var("RTOP_POSTGRES_PASSWORD").ok()?,
        timestamp_timezone: None,
    })
}

#[test]
fn postgres_stream_stops_early_and_connection_remains_usable() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    let mut rows = 0;
    source
        .execute_typed_stream("SELECT generate_series(1, 1000000)", &[], &mut |_| {
            rows += 1;
            Ok(StreamControl::Stop)
        })
        .expect("early stop must release the result stream");
    assert_eq!(rows, 1);
    assert_eq!(
        source
            .execute("SELECT 42", &[])
            .expect("connection is reusable"),
        vec![vec![Some("42".into())]]
    );
}

#[test]
fn postgres_binds_parameters_without_interpolating_sql() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    assert!(source.supports_postgres_bgp_pushdown());
    assert_eq!(
        source
            .execute("SELECT $1::text", &["Ada'; SELECT 1; --".into()])
            .expect("bound parameter query"),
        vec![vec![Some("Ada'; SELECT 1; --".into())]]
    );
}

#[test]
fn postgres_cancellation_has_stable_diagnostic_and_connection_remains_usable() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    let cancellation = source.cancellation();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        cancellation.cancel().expect("send cancellation request");
    });
    assert_eq!(
        source.execute("SELECT pg_sleep(5)", &[]),
        Err(RuntimeError::DataSource("query-cancelled".into()))
    );
    canceller.join().expect("cancellation thread completes");
    assert_eq!(
        source
            .execute("SELECT 7", &[])
            .expect("connection is reusable"),
        vec![vec![Some("7".into())]]
    );
    source
        .cancel()
        .expect("public cancellation port accepts an idle connection");
}

#[test]
fn postgres_executes_geosparql_postgis_calls_through_the_public_adapter_port() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    source
        .execute("CREATE EXTENSION IF NOT EXISTS postgis", &[])
        .expect("enable fixed PostGIS extension");
    assert_eq!(
        source
            .geospatial(
                "GEOF:SFINTERSECTS",
                &["POINT(0 0)".into(), "POINT(0 0)".into()],
            )
            .expect("evaluate sfIntersects"),
        Some(GeospatialValue::Boolean(true))
    );
    assert_eq!(
        source
            .geospatial(
                "<http://www.opengis.net/def/function/geosparql/intersection>",
                &["POINT(0 0)".into(), "POINT(0 0)".into()],
            )
            .expect("evaluate intersection"),
        Some(GeospatialValue::Wkt("POINT(0 0)".into()))
    );
    assert!(matches!(
        source
            .geospatial(
                "GEOF:BUFFER",
                &[
                    "POINT(2 2)".into(),
                    "20".into(),
                    "uom:metre".into(),
                ],
            )
            .expect("evaluate metre buffer"),
        Some(GeospatialValue::Wkt(value)) if value.starts_with("POLYGON(")
    ));
    assert!(matches!(
        source
            .geospatial_intersection_with_buffer(
                "POLYGON((2 2,7 2,7 5,2 5,2 2))",
                "20",
                "POLYGON((1 1,8 1,8 7,1 7,1 1))",
            )
            .expect("evaluate nested buffer intersection"),
        Some(GeospatialValue::Wkt(value)) if value.starts_with("POLYGON(")
    ));
    assert_eq!(
        source
            .geospatial("GEOF:BUFFER", &["POINT(0 0)".into()])
            .expect("unsupported arity is not a datasource failure"),
        None
    );
}

#[test]
fn postgres_exposes_manifest_scalar_datatypes() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    let rows = source
        .execute_typed(
            "SELECT true::boolean, 7::smallint, 8::integer, 9::bigint, 1.25::numeric, 2.5::real, 3.5::double precision, DATE '2013-03-18', TIME '10:12:10', TIMETZ '10:12:10+01', TIMESTAMP '2013-03-18 10:12:10', TIMESTAMPTZ '2013-03-19 03:12:10+01', decode('0a0b', 'hex')",
            &[],
        )
        .expect("manifest scalar types must be readable");
    assert_eq!(
        rows,
        vec![vec![
            Some(DataValue {
                value: "true".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".into())
            }),
            Some(DataValue {
                value: "7".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into())
            }),
            Some(DataValue {
                value: "8".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into())
            }),
            Some(DataValue {
                value: "9".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#integer".into())
            }),
            Some(DataValue {
                value: "1.25".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into())
            }),
            Some(DataValue {
                value: "2.5E0".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#double".into())
            }),
            Some(DataValue {
                value: "3.5E0".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#double".into())
            }),
            Some(DataValue {
                value: "2013-03-18".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#date".into())
            }),
            Some(DataValue {
                value: "10:12:10".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#time".into())
            }),
            Some(DataValue {
                value: "10:12:10+01:00".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#time".into())
            }),
            Some(DataValue {
                value: "2013-03-18T10:12:10".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#dateTime".into())
            }),
            Some(DataValue {
                value: "2013-03-19T02:12:10.000000Z".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#dateTime".into())
            }),
            Some(DataValue {
                value: "0A0B".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#hexBinary".into())
            }),
        ]]
    );
}

#[test]
fn postgres_canonicalizes_special_numeric_and_temporal_lexicals() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    let rows = source
        .execute_typed(
            "SELECT 1::real, 'NaN'::double precision, 'Infinity'::double precision, '-Infinity'::double precision, 'NaN'::numeric, TIMETZ '10:12:10.125+00', INTERVAL '1 mon 1 day -00:01:40.125'",
            &[],
        )
        .expect("special PostgreSQL scalar values must be readable");
    assert_eq!(
        rows,
        vec![vec![
            Some(DataValue {
                value: "1.0E0".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#double".into()),
            }),
            Some(DataValue {
                value: "NaN".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#double".into()),
            }),
            Some(DataValue {
                value: "INF".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#double".into()),
            }),
            Some(DataValue {
                value: "-INF".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#double".into()),
            }),
            Some(DataValue {
                value: "NaN".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#decimal".into()),
            }),
            Some(DataValue {
                value: "10:12:10.125Z".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#time".into()),
            }),
            Some(DataValue {
                value: "1 mon 1 day -00:01:40.125".into(),
                datatype: None,
            }),
        ]]
    );
    assert_eq!(
        source
            .execute("SELECT NULL::text, 'plain text'::text", &[])
            .expect("untyped null and text values must be readable"),
        vec![vec![None, Some("plain text".into())]]
    );
}

#[test]
fn postgres_exposes_interval_for_explicit_string_mappings() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    assert_eq!(
        source
            .execute_typed("SELECT INTERVAL '100 seconds'", &[])
            .expect("interval must be readable"),
        vec![vec![Some(DataValue {
            value: "00:01:40".into(),
            datatype: None,
        })]]
    );
}

#[test]
fn postgres_applies_or_rejects_timestamp_timezone_configuration() {
    let Some(mut config) = postgres_config() else {
        return;
    };
    config.timestamp_timezone = Some("Asia/Shanghai".into());
    let mut source = PostgresDataSource::connect(&config).expect("valid timezone connects");
    assert_eq!(
        source
            .execute_typed("SELECT TIMESTAMP '2013-03-18 10:12:10'", &[])
            .expect("timestamp query"),
        vec![vec![Some(DataValue {
            value: "2013-03-18T18:12:10+08:00".into(),
            datatype: Some("http://www.w3.org/2001/XMLSchema#dateTime".into()),
        })]]
    );

    config.timestamp_timezone = Some("not-a-timezone".into());
    assert!(matches!(
        PostgresDataSource::connect(&config),
        Err(RuntimeError::Config(message)) if message == "无效 datasource.timestamp_timezone"
    ));
}

#[test]
fn postgres_inspects_primary_unique_and_foreign_key_constraints() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    assert_eq!(
        source.relation_constraints("Book").expect("inspect Book"),
        RelationConstraints {
            unique_or_primary_key_count: 1,
            foreign_key_count: 0,
        }
    );
    assert_eq!(
        source
            .relation_constraints("BookWriter")
            .expect("inspect BookWriter"),
        RelationConstraints {
            unique_or_primary_key_count: 0,
            foreign_key_count: 2,
        }
    );
    assert_eq!(
        source
            .relation_constraints("Edition")
            .expect("inspect Edition"),
        RelationConstraints {
            unique_or_primary_key_count: 1,
            foreign_key_count: 1,
        }
    );
    assert_eq!(
        source
            .relation_constraints("Writer")
            .expect("inspect Writer"),
        RelationConstraints {
            unique_or_primary_key_count: 1,
            foreign_key_count: 0,
        }
    );
}

#[test]
fn postgres_exports_quoted_catalog_metadata_for_fixture_constraints() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    let metadata = source
        .database_metadata()
        .expect("export database metadata");

    let book_writer = metadata
        .relations
        .iter()
        .find(|relation| relation.name == ["\"BookWriter\""])
        .expect("BookWriter fixture relation is present");
    assert_eq!(
        book_writer
            .columns
            .iter()
            .map(|column| (
                column.name.as_str(),
                column.is_nullable,
                column.datatype.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("\"bk_code\"", true, "INTEGER"),
            ("\"wr_code\"", true, "INTEGER"),
        ]
    );
    assert!(book_writer.unique_constraints.is_empty());
    assert_eq!(book_writer.foreign_keys.len(), 2);
    assert_eq!(
        book_writer.foreign_keys[0].from.relation,
        vec!["\"BookWriter\""],
    );
    assert_eq!(
        book_writer.foreign_keys[0].from.columns,
        vec!["\"bk_code\""],
    );
    assert_eq!(book_writer.foreign_keys[0].to.relation, vec!["\"Book\""],);
    assert_eq!(book_writer.foreign_keys[0].to.columns, vec!["\"bk_code\""],);
    assert_eq!(
        book_writer.foreign_keys[1].from.columns,
        vec!["\"wr_code\""],
    );
    assert_eq!(book_writer.foreign_keys[1].to.relation, vec!["\"Writer\""],);
    assert_eq!(book_writer.foreign_keys[1].to.columns, vec!["\"wr_code\""],);

    let edition = metadata
        .relations
        .iter()
        .find(|relation| relation.name == ["\"Edition\""])
        .expect("Edition fixture relation is present");
    assert_eq!(edition.unique_constraints.len(), 1);
    assert!(edition.unique_constraints[0].is_primary_key);
    assert_eq!(
        edition.unique_constraints[0].determinants,
        vec!["\"ed_code\""]
    );
    assert_eq!(edition.other_names.len(), 1);
    assert_eq!(edition.other_names[0][1], "\"Edition\"");
}

#[test]
fn postgres_groups_composite_catalog_constraints_in_metadata_exports() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    source
        .execute(
            "CREATE TABLE \"rtop_catalog_composite_parent\" (\
             \"part_a\" integer NOT NULL, \"part_b\" integer NOT NULL, \
             CONSTRAINT \"rtop_catalog_composite_parent_pkey\" PRIMARY KEY (\"part_a\", \"part_b\"))",
            &[],
        )
        .expect("create composite metadata parent");
    source
        .execute(
            "CREATE TABLE \"rtop_catalog_composite_child\" (\
             \"child_a\" integer NOT NULL, \"child_b\" integer NOT NULL, \
             \"alternate_a\" integer NOT NULL, \"alternate_b\" integer NOT NULL, \
             CONSTRAINT \"rtop_catalog_composite_child_pkey\" PRIMARY KEY (\"child_a\", \"child_b\"), \
             CONSTRAINT \"rtop_catalog_composite_child_unique\" UNIQUE (\"alternate_a\", \"alternate_b\"), \
             CONSTRAINT \"rtop_catalog_composite_child_parent_fkey\" FOREIGN KEY (\"child_a\", \"child_b\") \
                 REFERENCES \"rtop_catalog_composite_parent\" (\"part_a\", \"part_b\"))",
            &[],
        )
        .expect("create composite metadata child");

    let relation = source
        .relation_metadata("rtop_catalog_composite_child")
        .expect("inspect composite relation metadata");
    assert_eq!(relation.primary_key, vec!["child_a", "child_b"]);
    assert_eq!(relation.foreign_keys.len(), 1);
    assert_eq!(relation.foreign_keys[0].columns, vec!["child_a", "child_b"]);
    assert_eq!(
        relation.foreign_keys[0].referenced_columns,
        vec!["part_a", "part_b"]
    );

    let metadata = source
        .database_metadata()
        .expect("export composite metadata");
    let relation = metadata
        .relations
        .iter()
        .find(|relation| relation.name == ["\"rtop_catalog_composite_child\""])
        .expect("composite child is exported");
    assert_eq!(relation.unique_constraints.len(), 2);
    assert_eq!(
        relation.unique_constraints[0].determinants,
        vec!["\"child_a\"", "\"child_b\""]
    );
    assert!(relation.unique_constraints[0].is_primary_key);
    assert_eq!(
        relation.unique_constraints[1].determinants,
        vec!["\"alternate_a\"", "\"alternate_b\""]
    );
    assert!(!relation.unique_constraints[1].is_primary_key);
    assert_eq!(relation.foreign_keys.len(), 1);
    assert_eq!(
        relation.foreign_keys[0].from.columns,
        vec!["\"child_a\"", "\"child_b\""]
    );
    assert_eq!(
        relation.foreign_keys[0].to.relation,
        vec!["\"rtop_catalog_composite_parent\""]
    );
    assert_eq!(
        relation.foreign_keys[0].to.columns,
        vec!["\"part_a\"", "\"part_b\""]
    );

    source
        .execute("DROP TABLE \"rtop_catalog_composite_child\"", &[])
        .expect("clean up composite metadata child");
    source
        .execute("DROP TABLE \"rtop_catalog_composite_parent\"", &[])
        .expect("clean up composite metadata parent");
}

#[test]
fn postgres_exposes_relation_metadata_for_direct_mapping() {
    let Some(config) = postgres_config() else {
        return;
    };
    let mut source = PostgresDataSource::connect(&config).expect("connect PostgreSQL");
    source
        .execute(
            "CREATE TABLE \"rtop_direct_metadata_parent\" (\"id\" integer PRIMARY KEY)",
            &[],
        )
        .expect("create isolated Direct Mapping metadata parent");
    source
        .execute(
            "CREATE TABLE \"rtop_direct_metadata_child\" (\"id\" integer NOT NULL, \"parent_id\" integer, \
             CONSTRAINT \"rtop_direct_metadata_child_pkey\" PRIMARY KEY (\"id\"), \
             CONSTRAINT \"rtop_direct_metadata_child_parent_fkey\" FOREIGN KEY (\"parent_id\") REFERENCES \"rtop_direct_metadata_parent\"(\"id\"))",
            &[],
        )
        .expect("create isolated Direct Mapping metadata child");
    assert_eq!(
        source
            .relation_metadata("rtop_direct_metadata_child")
            .expect("inspect isolated child relation"),
        RelationMetadata {
            columns: vec![
                RelationColumn {
                    name: "id".into(),
                    nullable: false
                },
                RelationColumn {
                    name: "parent_id".into(),
                    nullable: true
                },
            ],
            primary_key: vec!["id".into()],
            foreign_keys: vec![RelationForeignKey {
                name: "rtop_direct_metadata_child_parent_fkey".into(),
                columns: vec!["parent_id".into()],
                referenced_table: "rtop_direct_metadata_parent".into(),
                referenced_columns: vec!["id".into()],
            }],
        }
    );
    source
        .execute("DROP TABLE \"rtop_direct_metadata_child\"", &[])
        .expect("clean up isolated Direct Mapping metadata child");
    source
        .execute("DROP TABLE \"rtop_direct_metadata_parent\"", &[])
        .expect("clean up isolated Direct Mapping metadata parent");
}
