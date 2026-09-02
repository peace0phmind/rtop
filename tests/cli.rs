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
