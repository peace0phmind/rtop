use crate::RuntimeError;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct FileConfig { mapping: String, datasource: FileDataSource }
#[derive(Debug, Deserialize)]
struct FileDataSource { kind: String, host: String, port: Option<u16>, database: String, user: String, password: Option<String>, password_file: Option<String> }

#[derive(Debug, Clone)]
pub struct KnowledgeGraphSpec { pub mapping_file: PathBuf, pub database_url: String }

pub fn load_spec(path: impl AsRef<Path>) -> Result<KnowledgeGraphSpec, RuntimeError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| RuntimeError::Config(format!("cannot read config: {e}")))?;
    if text.contains("jdbc:") || text.contains("driver") || text.contains("DataSource") {
        return Err(RuntimeError::Config("JDBC URL, driver class and Java DataSource are not supported".into()));
    }
    let config: FileConfig = toml::from_str(&text).map_err(|e| RuntimeError::Config(e.to_string()))?;
    if config.datasource.kind != "postgres" { return Err(RuntimeError::Config("datasource.kind must be postgres".into())); }
    let password = match (config.datasource.password, config.datasource.password_file) {
        (Some(_), Some(_)) => return Err(RuntimeError::Config("use password or password_file, not both".into())),
        (Some(value), None) => value,
        (None, Some(file)) => std::fs::read_to_string(path.parent().unwrap_or(Path::new(".")).join(file))
            .map_err(|e| RuntimeError::Config(format!("cannot read password_file: {e}")))?.trim_end().to_owned(),
        (None, None) => return Err(RuntimeError::Config("datasource password or password_file is required".into())),
    };
    let base = path.parent().unwrap_or(Path::new("."));
    let mapping_file = base.join(config.mapping);
    let port = config.datasource.port.unwrap_or(5432);
    let database_url = format!("host={} port={} dbname={} user={} password={}", config.datasource.host, port, config.datasource.database, config.datasource.user, password);
    Ok(KnowledgeGraphSpec { mapping_file, database_url })
}
