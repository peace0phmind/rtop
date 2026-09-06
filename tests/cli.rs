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
