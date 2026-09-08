#[test]
fn rejects_jdbc_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rtop.toml");
    std::fs::write(
        &config,
        "mapping = 'm.obda'\njdbc.url = 'jdbc:postgresql://example/db'\n",
    )
    .unwrap();
    assert!(
        format!("{}", rtop::load_configuration(&config).unwrap_err())
            .starts_with("invalid-config:")
    );
}

#[test]
fn rejects_jdbc_namespace_without_a_jdbc_url() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rtop.toml");
    std::fs::write(&config, "mapping = 'm.obda'\njdbc.fetch_size = 10\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n").unwrap();
    assert!(format!("{}", rtop::load_configuration(&config).unwrap_err()).contains("JDBC"));
}

#[test]
fn accepts_the_native_datasource_section() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rtop.toml");
    std::fs::write(&config, "mapping = 'm.obda'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n").unwrap();
    assert!(rtop::load_configuration(&config).is_ok());
}

#[test]
fn accepts_mapping_datatype_inference_override() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rtop.toml");
    std::fs::write(
        &config,
        "mapping = 'm.obda'\nmapping_infer_default_datatype = false\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n",
    )
    .unwrap();

    assert!(
        !rtop::load_configuration(&config)
            .unwrap()
            .mapping_infer_default_datatype
    );
}

#[test]
fn rejects_postgres_only_and_mutually_exclusive_configuration_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rtop.toml");
    let write = |body: &str| std::fs::write(&config, body).unwrap();
    let assert_error = |expected: &str| {
        assert!(
            format!("{}", rtop::load_configuration(&config).unwrap_err()).contains(expected),
            "expected {expected}"
        );
    };

    write("mapping = 'm.obda'\n[datasource]\nkind = 'mysql'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n");
    assert_error("datasource.kind 必须为 postgres");

    write("mapping = 'm.obda'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\npassword_file = 'secret'\n");
    assert_error("password 与 password_file 只能指定一个");

    write("mapping = 'm.obda'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\n");
    assert_error("必须指定 password 或 password_file");

    write("mapping = 'm.obda'\n[endpoint]\npredefined_config = 'predefined.json'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n");
    assert_error("必须同时指定");

    write("mapping = 'm.obda'\n[direct_mapping]\nbase_iri = 'https://example.test/'\nrelations = ['people']\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n");
    assert_error("必须且只能指定 mapping 或 direct_mapping");

    write("[direct_mapping]\nbase_iri = 'https://example.test/'\nrelations = []\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n");
    assert_error("direct_mapping.relations 不能为空");

    write("[direct_mapping]\nbase_iri = 'not absolute'\nrelations = ['people']\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n");
    assert_error("无效 direct_mapping.base_iri");

    write("mapping = 'm.obda'\nfacts_format = 'json'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n");
    assert_error("不支持的 facts_format");
}

#[test]
fn resolves_password_file_direct_mapping_and_endpoint_options() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rtop.toml");
    std::fs::write(dir.path().join("secret"), "from-file\n").unwrap();
    std::fs::write(
        &config,
        "facts = 'facts.nq'\nfacts_format = 'nquads'\nfacts_base_iri = 'https://example.test/facts/'\n[direct_mapping]\nbase_iri = 'https://example.test/base/'\nrelations = ['people']\npreserve_physical_rows = false\n[endpoint]\nenable_download_ontology = true\npredefined_config = 'predefined.json'\npredefined_queries = 'predefined.toml'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\nport = 5433\ndatabase = 'x'\nuser = 'x'\npassword_file = 'secret'\ntimestamp_timezone = 'UTC'\n",
    )
    .unwrap();

    let loaded = rtop::load_configuration(&config).unwrap();
    assert_eq!(loaded.postgres.password, "from-file");
    assert_eq!(loaded.postgres.port, 5433);
    assert_eq!(loaded.postgres.timestamp_timezone.as_deref(), Some("UTC"));
    assert_eq!(loaded.spec.facts_format.as_deref(), Some("nquads"));
    assert_eq!(
        loaded.spec.facts_base_iri.as_deref(),
        Some("https://example.test/facts/")
    );
    assert_eq!(loaded.direct_mapping.unwrap().preserve_physical_rows, false);
    assert!(loaded.endpoint.enable_download_ontology);
    assert!(loaded
        .endpoint
        .predefined_config
        .unwrap()
        .ends_with("predefined.json"));
    assert!(loaded
        .endpoint
        .predefined_queries
        .unwrap()
        .ends_with("predefined.toml"));
}

#[test]
fn classifies_file_and_toml_loading_failures_before_runtime_startup() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.toml");
    assert!(
        format!("{}", rtop::load_configuration(&missing).unwrap_err()).contains("无法读取配置")
    );

    let config = dir.path().join("rtop.toml");
    std::fs::write(&config, "not valid toml = [").unwrap();
    assert!(
        format!("{}", rtop::load_configuration(&config).unwrap_err())
            .starts_with("invalid-config:")
    );

    std::fs::write(
        &config,
        "mapping = 'm.obda'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword_file = 'missing-secret'\n",
    )
    .unwrap();
    assert!(
        format!("{}", rtop::load_configuration(&config).unwrap_err())
            .contains("无法读取 password_file")
    );

    std::fs::write(
        &config,
        "mapping = 'm.obda'\nontology = 'missing.ttl'\n[datasource]\nkind = 'postgres'\nhost = 'localhost'\ndatabase = 'x'\nuser = 'x'\npassword = 'x'\n",
    )
    .unwrap();
    assert!(rtop::load_configuration(&config)
        .unwrap()
        .spec
        .ontology_file
        .unwrap()
        .ends_with("missing.ttl"));
}
