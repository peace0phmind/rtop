use crate::RuntimeError;
use crate::datasource::PostgresConnectionConfig;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct FileConfig { mapping: String, datasource: FileDataSource }
#[derive(Debug, Deserialize)]
struct FileDataSource { kind: String, host: String, port: Option<u16>, database: String, user: String, password: Option<String>, password_file: Option<String> }

#[derive(Debug, Clone)]
pub struct KnowledgeGraphSpec { pub mapping_file: PathBuf }

/// 配置 adapter 的产物；连接信息只交给 PostgreSQL adapter，不进入 `VkgRuntime`。
#[derive(Debug, Clone)]
pub struct LoadedConfiguration { pub spec: KnowledgeGraphSpec, pub postgres: PostgresConnectionConfig }

pub fn load_configuration(path: impl AsRef<Path>) -> Result<LoadedConfiguration, RuntimeError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| RuntimeError::Config(format!("cannot read config: {e}")))?;
    let lower = text.to_ascii_lowercase();
    if lower.contains("jdbc:") || lower.contains("jdbc.") || lower.contains("driver") || lower.contains("datasource") {
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
    let postgres = PostgresConnectionConfig { host: config.datasource.host, port, database: config.datasource.database, user: config.datasource.user, password };
    Ok(LoadedConfiguration { spec: KnowledgeGraphSpec { mapping_file }, postgres })
}
