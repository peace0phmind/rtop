use rtop::{
    DataSource, DataValue, PostgresConnectionConfig, PostgresDataSource, RelationColumn,
    RelationConstraints, RelationForeignKey, RelationMetadata, RuntimeError, StreamControl,
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
                value: "2.5".into(),
                datatype: Some("http://www.w3.org/2001/XMLSchema#double".into())
            }),
            Some(DataValue {
                value: "3.5".into(),
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
