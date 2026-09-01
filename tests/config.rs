#[test]
fn rejects_jdbc_configuration() {
    let dir = tempfile::tempdir().unwrap(); let config = dir.path().join("rtop.toml");
    std::fs::write(&config, "mapping = 'm.obda'\njdbc.url = 'jdbc:postgresql://example/db'\n").unwrap();
    assert!(format!("{}", rtop::load_spec(config).unwrap_err()).starts_with("invalid-config:"));
}
