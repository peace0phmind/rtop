#[test]
fn rejects_jdbc_configuration() {
    let dir = tempfile::tempdir().unwrap(); let config = dir.path().join("rtop.toml");
    std::fs::write(&config, "mapping = 'm.obda'\njdbc.url = 'jdbc:postgresql://example/db'\n").unwrap();
    assert!(format!("{}", rtop::load_configuration(config).unwrap_err()).starts_with("invalid-config:"));
}

#[test]
fn rejects_jdbc_namespace_without_a_jdbc_url() {
    let dir = tempfile::tempdir().unwrap(); let config = dir.path().join("rtop.toml");
    std::fs::write(&config, "mapping = 'm.obda'\njdbc.fetch_size = 10\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n").unwrap();
    assert!(format!("{}", rtop::load_configuration(config).unwrap_err()).contains("JDBC"));
}
