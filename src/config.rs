use crate::datasource::PostgresConnectionConfig;
use crate::RuntimeError;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct FileConfig {
    mapping: String,
    facts: Option<String>,
    facts_format: Option<String>,
    facts_base_iri: Option<String>,
    ontology: Option<String>,
    datasource: FileDataSource,
}
#[derive(Debug, Deserialize)]
struct FileDataSource {
    kind: String,
    host: String,
    port: Option<u16>,
    database: String,
    user: String,
    password: Option<String>,
    password_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeGraphSpec {
    pub mapping_file: PathBuf,
    pub facts_file: Option<PathBuf>,
    pub facts_format: Option<String>,
    pub facts_base_iri: Option<String>,
    pub ontology_file: Option<PathBuf>,
}

/// 配置 adapter 的产物；连接信息只交给 PostgreSQL adapter，不进入 `VkgRuntime`。
#[derive(Debug, Clone)]
pub struct LoadedConfiguration {
    pub spec: KnowledgeGraphSpec,
    pub postgres: PostgresConnectionConfig,
}

pub fn load_configuration(path: impl AsRef<Path>) -> Result<LoadedConfiguration, RuntimeError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| RuntimeError::Config(format!("无法读取配置：{e}")))?;
    let lower = text.to_ascii_lowercase();
    if lower.contains("jdbc:")
        || lower.contains("jdbc.")
        || lower.contains("driver_class")
        || lower.contains("java datasource")
    {
        return Err(RuntimeError::Config(
            "不支持 JDBC URL、驱动类或 Java DataSource".into(),
        ));
    }
    let config: FileConfig =
        toml::from_str(&text).map_err(|e| RuntimeError::Config(e.to_string()))?;
    if config.datasource.kind != "postgres" {
        return Err(RuntimeError::Config(
            "datasource.kind 必须为 postgres".into(),
        ));
    }
    let password = match (config.datasource.password, config.datasource.password_file) {
        (Some(_), Some(_)) => {
            return Err(RuntimeError::Config(
                "password 与 password_file 只能指定一个".into(),
            ))
        }
        (Some(value), None) => value,
        (None, Some(file)) => {
            std::fs::read_to_string(path.parent().unwrap_or(Path::new(".")).join(file))
                .map_err(|e| RuntimeError::Config(format!("无法读取 password_file：{e}")))?
                .trim_end()
                .to_owned()
        }
        (None, None) => {
            return Err(RuntimeError::Config(
                "datasource 必须指定 password 或 password_file".into(),
            ))
        }
    };
    let base = path.parent().unwrap_or(Path::new("."));
    let mapping_file = base.join(config.mapping);
    let facts_file = config.facts.map(|file| base.join(file));
    if let Some(format) = &config.facts_format {
        if !matches!(format.as_str(), "turtle" | "ttl" | "nquads" | "nq") {
            return Err(RuntimeError::Config(format!(
                "不支持的 facts_format：{format}"
            )));
        }
    }
    let port = config.datasource.port.unwrap_or(5432);
    let postgres = PostgresConnectionConfig {
        host: config.datasource.host,
        port,
        database: config.datasource.database,
        user: config.datasource.user,
        password,
    };
    Ok(LoadedConfiguration {
        spec: KnowledgeGraphSpec {
            mapping_file,
            facts_file,
            facts_format: config.facts_format,
            facts_base_iri: config.facts_base_iri,
            ontology_file: config.ontology.map(|file| base.join(file)),
        },
        postgres,
    })
}
