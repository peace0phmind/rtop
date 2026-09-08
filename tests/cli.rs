use std::process::Command;

#[test]
fn validate_rejects_an_unreadable_mapping() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rtop.toml");
    std::fs::write(&config, "mapping = 'missing.obda'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args(["validate", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid-mapping"));
}

#[test]
fn validate_reports_completion_and_compile_is_silent_without_connecting_postgres() {
    let dir = tempfile::tempdir().unwrap();
    let mapping = dir.path().join("mapping.obda");
    let config = dir.path().join("rtop.toml");
    std::fs::write(
        &mapping,
        "[MappingDeclaration]\ntarget <https://example.test/person/{id}> <https://example.test/name> {name} .\nsource SELECT id, name FROM people\n",
    )
    .unwrap();
    std::fs::write(
        &config,
        "mapping = 'mapping.obda'\n[datasource]\nkind = 'postgres'\nhost = '127.0.0.1'\nport = 1\ndatabase = 'unused'\nuser = 'unused'\npassword = 'unused'\n",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_rtop");

    let validate = Command::new(binary)
        .args(["validate", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(validate.status.success(), "{validate:?}");
    assert_eq!(
        String::from_utf8_lossy(&validate.stdout).trim(),
        "Validation completed"
    );
    assert!(validate.stderr.is_empty());

    let compile = Command::new(binary)
        .args(["compile", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(compile.status.success(), "{compile:?}");
    assert!(compile.stdout.is_empty());
    assert!(compile.stderr.is_empty());
}

#[test]
fn validate_and_compile_accept_direct_mapping_configuration_without_connecting_postgres() {
    // Direct Mapping 的 validate/compile 只能验证显式 relation allow-list 与 base IRI；
    // catalog 在 query/endpoint 执行期才连接 PostgreSQL。使用不可连接端口确保 CLI
    // 没有在这个诊断入口意外建立数据库连接。
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("direct.toml");
    std::fs::write(
        &config,
        "[direct_mapping]\nbase_iri = 'https://example.test/base/'\nrelations = ['people']\n\n[datasource]\nkind = 'postgres'\nhost = '127.0.0.1'\nport = 1\ndatabase = 'unused'\nuser = 'unused'\npassword = 'unused'\n",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_rtop");

    let validate = Command::new(binary)
        .args(["validate", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(validate.status.success(), "{validate:?}");
    assert_eq!(
        String::from_utf8_lossy(&validate.stdout).trim(),
        "Validation completed"
    );
    assert!(validate.stderr.is_empty());

    let compile = Command::new(binary)
        .args(["compile", config.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(compile.status.success(), "{compile:?}");
    assert!(compile.stdout.is_empty());
    assert!(compile.stderr.is_empty());
}

#[test]
fn query_rejects_an_unreadable_query_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args(["query", "missing-config.toml", "missing-query.rq"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid-config: 无法读取查询文件"));
}

#[test]
fn to_r2rml_rejects_duplicate_native_mapping_ids() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("duplicate.obda");
    let output = dir.path().join("converted.ttl");
    std::fs::write(
        &input,
        "[MappingDeclaration] @collection [[\nmappingId duplicate\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n\nmappingId duplicate\ntarget <https://example.test/person/{id}> <https://example.test/label> \"person\" .\nsource SELECT id FROM people\n]]\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args([
            "mapping",
            "to-r2rml",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("Duplicate mapping IDs found in obda file"));
    assert!(!output.exists());
}

#[test]
fn mapping_conversion_commands_write_ontop_compatible_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("input.obda");
    let r2rml = dir.path().join("converted.ttl");
    let pretty = dir.path().join("pretty.ttl");
    std::fs::write(
        &native,
        "[MappingDeclaration] @collection [[\nmappingId person\ntarget <https://example.test/person/{id}> <https://example.test/label> {name}@en .\nsource SELECT id, name FROM people\n]]\n",
    )
    .unwrap();

    let missing_force = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args([
            "mapping",
            "to-r2rml",
            native.to_str().unwrap(),
            r2rml.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(missing_force.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_force.stderr).contains("需要 PostgreSQL metadata"));

    let converted = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args([
            "mapping",
            "to-r2rml",
            native.to_str().unwrap(),
            r2rml.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert!(converted.status.success(), "{converted:?}");
    let serialized = std::fs::read_to_string(&r2rml).unwrap();
    assert!(serialized.contains("rr:column \"name\""));
    assert!(serialized.contains("rr:language \"en\""));

    let prettified = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args([
            "mapping",
            "pretty-r2rml",
            r2rml.to_str().unwrap(),
            pretty.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(prettified.status.success(), "{prettified:?}");
    assert!(std::fs::read_to_string(pretty)
        .unwrap()
        .contains("http://www.w3.org/ns/r2rml#"));
}

#[test]
fn mapping_conversion_commands_classify_invalid_input_at_the_cli_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let invalid_r2rml = dir.path().join("invalid.ttl");
    let output = dir.path().join("converted.obda");
    std::fs::write(
        &invalid_r2rml,
        "@prefix rr: <http://www.w3.org/ns/r2rml#> .\n<map> rr:",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_rtop");

    let to_obda = Command::new(binary)
        .args([
            "mapping",
            "to-obda",
            invalid_r2rml.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(to_obda.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&to_obda.stderr).contains("R2RML RDF 语法错误"));
    assert!(!output.exists());

    let pretty = Command::new(binary)
        .args([
            "mapping",
            "pretty-r2rml",
            invalid_r2rml.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(pretty.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&pretty.stderr).contains("R2RML RDF 语法错误"));

    let missing_native = Command::new(binary)
        .args([
            "mapping",
            "to-r2rml",
            dir.path().join("missing.obda").to_str().unwrap(),
            output.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert_eq!(missing_native.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_native.stderr).contains("无法读取 native OBDA"));
}

#[test]
fn to_obda_uses_ontop_default_base_for_relative_iri_templates_only() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("relative.ttl");
    let output = dir.path().join("converted.obda");
    std::fs::write(
        &input,
        "@prefix rr: <http://www.w3.org/ns/r2rml#> .\n@base <https://example.test/base/> .\n<map> a rr:TriplesMap; rr:logicalTable [ rr:sqlQuery \"SELECT id FROM people\" ]; rr:subjectMap [ rr:template \"person/{id}\" ]; rr:predicateObjectMap [ rr:predicate <kind>; rr:objectMap [ rr:constant <Person> ] ] .\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args([
            "mapping",
            "to-obda",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    let native = std::fs::read_to_string(output).unwrap();
    assert!(native.contains("<http://example.com/base/person/{id}>"));
    assert!(native.contains("<https://example.test/base/kind>"));
    assert!(native.contains("<https://example.test/base/Person>"));
}

#[test]
fn v1_to_v3_rejects_legacy_source_uri_like_ontop() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("legacy.obda");
    let output = dir.path().join("converted.obda");
    std::fs::write(
        &input,
        "[SourceDeclaration]\nsourceUri legacy-source\nconnectionUrl jdbc:postgresql://example.test/db\n\n[MappingDeclaration] @collection [[\nmappingId m\ntarget <https://example.test/person/{id}> <https://example.test/type> <https://example.test/Person> .\nsource SELECT id FROM people\n]]\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rtop"))
        .args([
            "mapping",
            "v1-to-v3",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("Unknown parameter name \"sourceUri\"")
    );
    assert!(!output.exists());
}

#[test]
fn v1_to_v3_converts_native_properties_simplified_sql_and_r2rml_inputs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures = root.join("tests/compat/postgres-query-kinds");
    let dir = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_rtop");

    let native_output = dir.path().join("native.obda");
    let native = Command::new(binary)
        .args([
            "mapping",
            "v1-to-v3",
            fixtures
                .join("v1-to-v3-duplicate-alias.obda")
                .to_str()
                .unwrap(),
            native_output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(native.status.success(), "{native:?}");
    assert!(std::fs::read_to_string(&native_output)
        .unwrap()
        .contains("left_person.id AS id1, right_person.id AS id2"));
    assert!(
        std::fs::read_to_string(native_output.with_extension("properties"))
            .unwrap()
            .contains("jdbc.url=jdbc:postgresql://legacy.invalid/rtop_test")
    );

    let simplified_output = dir.path().join("simplified.obda");
    let simplified = Command::new(binary)
        .args([
            "mapping",
            "v1-to-v3",
            fixtures
                .join("v1-to-v3-simplify-projection.obda")
                .to_str()
                .unwrap(),
            simplified_output.to_str().unwrap(),
            "--simplify-projection",
        ])
        .output()
        .unwrap();
    assert!(simplified.status.success(), "{simplified:?}");
    assert!(std::fs::read_to_string(&simplified_output)
        .unwrap()
        .contains("source\t\tSELECT * FROM query_people"));

    let r2rml_output = dir.path().join("converted.ttl");
    let r2rml = Command::new(binary)
        .args([
            "mapping",
            "v1-to-v3",
            fixtures.join("v1-to-v3-r2rml.ttl").to_str().unwrap(),
            r2rml_output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(r2rml.status.success(), "{r2rml:?}");
    assert!(std::fs::read_to_string(r2rml_output)
        .unwrap()
        .contains("rr:predicate <https://example.test/related>"));
}

#[test]
fn cli_reports_stable_usage_for_missing_or_unknown_command_arguments() {
    let binary = env!("CARGO_BIN_EXE_rtop");

    let no_command = Command::new(binary).output().unwrap();
    assert_eq!(no_command.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&no_command.stderr).contains("用法：rtop <query|validate"));

    let unknown = Command::new(binary)
        .args(["unknown", "config.toml"])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(64));
    assert_eq!(
        String::from_utf8_lossy(&unknown.stderr).trim(),
        "未知命令：unknown"
    );

    let missing_mapping_files = Command::new(binary)
        .args(["mapping", "to-obda"])
        .output()
        .unwrap();
    assert_eq!(missing_mapping_files.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&missing_mapping_files.stderr)
        .contains("用法：rtop mapping <pretty-r2rml|v1-to-v3>"));

    let unknown_mapping = Command::new(binary)
        .args(["mapping", "unknown", "input", "output"])
        .output()
        .unwrap();
    assert_eq!(unknown_mapping.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&unknown_mapping.stderr)
        .contains("用法：rtop mapping <pretty-r2rml|to-obda|v1-to-v3>"));

    let missing_bootstrap = Command::new(binary)
        .args(["bootstrap", "config.toml"])
        .output()
        .unwrap();
    assert_eq!(missing_bootstrap.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&missing_bootstrap.stderr).contains("用法：rtop bootstrap"));
}

#[test]
fn cli_converts_and_migrates_fixed_ontop_mapping_assets() {
    let directory = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_rtop");
    let d001 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ontop/test/rdb2rdf-compliance/src/test/resources/D001/r2rmla.ttl");
    let pretty = directory.path().join("pretty.ttl");
    let native = directory.path().join("mapping.obda");
    let reloaded = directory.path().join("reloaded.ttl");

    for (mode, input, output) in [
        ("pretty-r2rml", d001.as_path(), pretty.as_path()),
        ("to-obda", d001.as_path(), native.as_path()),
    ] {
        let mut command = Command::new(binary);
        command.args([
            "mapping",
            mode,
            input.to_str().unwrap(),
            output.to_str().unwrap(),
        ]);
        let result = command.output().unwrap();
        assert!(result.status.success(), "{mode}: {result:?}");
    }
    assert!(std::fs::read_to_string(&pretty)
        .unwrap()
        .contains("http://www.w3.org/ns/r2rml#TriplesMap"));
    assert!(std::fs::read_to_string(&native)
        .unwrap()
        .contains("[MappingDeclaration]"));

    let reloaded_result = Command::new(binary)
        .args([
            "mapping",
            "to-r2rml",
            native.to_str().unwrap(),
            reloaded.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert!(reloaded_result.status.success(), "{reloaded_result:?}");
    assert!(std::fs::read_to_string(&reloaded)
        .unwrap()
        .contains("rr:TriplesMap"));

    let legacy = directory.path().join("legacy.obda");
    std::fs::write(
        &legacy,
        "[SourceDeclaration]\nconnectionUrl jdbc:postgresql://db\nusername ada\npassword secret\n\n[MappingDeclaration] @collection [[\nmappingId people\ntarget <https://example.test/person/{person.id}> a <https://example.test/Person> .\nsource SELECT person.id FROM person\n]]\n",
    )
    .unwrap();
    let migrated = directory.path().join("migrated.obda");
    let migration = Command::new(binary)
        .args([
            "mapping",
            "v1-to-v3",
            legacy.to_str().unwrap(),
            migrated.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(migration.status.success(), "{migration:?}");
    assert!(std::fs::read_to_string(&migrated).unwrap().contains("{id}"));
    assert_eq!(
        std::fs::read_to_string(migrated.with_extension("properties")).unwrap(),
        "jdbc.url=jdbc:postgresql://db\njdbc.user=ada\njdbc.password=secret\n"
    );
}
