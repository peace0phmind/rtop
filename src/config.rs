use crate::datasource::PostgresConnectionConfig;
use crate::RuntimeError;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct FileConfig {
    mapping: Option<String>,
    direct_mapping: Option<FileDirectMapping>,
    facts: Option<String>,
    facts_format: Option<String>,
    facts_base_iri: Option<String>,
    ontology: Option<String>,
    xml_catalog: Option<String>,
    endpoint: Option<FileEndpoint>,
    datasource: FileDataSource,
}
#[derive(Debug, Deserialize)]
struct FileEndpoint {
    enable_download_ontology: Option<bool>,
    predefined_config: Option<String>,
    predefined_queries: Option<String>,
}
#[derive(Debug, Deserialize)]
struct FileDirectMapping {
    base_iri: String,
    relations: Vec<String>,
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
    timestamp_timezone: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeGraphSpec {
    pub mapping_file: PathBuf,
    pub facts_file: Option<PathBuf>,
    pub facts_format: Option<String>,
    pub facts_base_iri: Option<String>,
    pub ontology_file: Option<PathBuf>,
    /// OWL imports 的本地 XML Catalog；仅用于把 IRI 映射到本地文档。
    pub xml_catalog_file: Option<PathBuf>,
}

/// 配置 adapter 的产物；连接信息只交给 PostgreSQL adapter，不进入 `VkgRuntime`。
#[derive(Debug, Clone)]
pub struct LoadedConfiguration {
    pub spec: KnowledgeGraphSpec,
    pub postgres: PostgresConnectionConfig,
    pub direct_mapping: Option<DirectMappingConfiguration>,
    pub endpoint: EndpointConfiguration,
}

/// endpoint adapter 的可选交付配置。两个预定义查询文件必须成对提供。
#[derive(Debug, Clone, Default)]
pub struct EndpointConfiguration {
    pub enable_download_ontology: bool,
    pub predefined_config: Option<PathBuf>,
    pub predefined_queries: Option<PathBuf>,
}

/// 配置文件中 Direct Mapping 的显式 relation allow-list。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectMappingConfiguration {
    pub base_iri: String,
    pub relations: Vec<String>,
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
    let endpoint = config.endpoint.unwrap_or(FileEndpoint {
        enable_download_ontology: None,
        predefined_config: None,
        predefined_queries: None,
    });
    if endpoint.predefined_config.is_some() != endpoint.predefined_queries.is_some() {
        return Err(RuntimeError::Config(
            "endpoint.predefined_config 与 endpoint.predefined_queries 必须同时指定".into(),
        ));
    }
    let direct_mapping = config
        .direct_mapping
        .map(|direct| DirectMappingConfiguration {
            base_iri: direct.base_iri,
            relations: direct.relations,
        });
    if config.mapping.is_some() == direct_mapping.is_some() {
        return Err(RuntimeError::Config(
            "必须且只能指定 mapping 或 direct_mapping".into(),
        ));
    }
    if let Some(direct) = &direct_mapping {
        if direct.relations.is_empty() {
            return Err(RuntimeError::Config(
                "direct_mapping.relations 不能为空".into(),
            ));
        }
        let _iri = oxiri::Iri::parse(direct.base_iri.clone())
            .map_err(|e| RuntimeError::Config(format!("无效 direct_mapping.base_iri：{e}")))?;
        if !direct.base_iri.contains(':') {
            return Err(RuntimeError::Config(
                "direct_mapping.base_iri 必须是绝对 IRI".into(),
            ));
        }
    }
    let xml_catalog_file = config.xml_catalog.map(|file| base.join(file));
    let ontology_file = config
        .ontology
        .map(|file| crate::ontology::resolve_input(base, &file, xml_catalog_file.as_deref()))
        .transpose()?;
    let mapping_file = config
        .mapping
        .map(|mapping| base.join(mapping))
        .unwrap_or_default();
    let facts_file = config.facts.map(|file| base.join(file));
    if let Some(format) = &config.facts_format {
        if !matches!(
            format.as_str(),
            "turtle" | "ttl" | "nquads" | "nq" | "rdf" | "xml" | "rdfxml"
        ) {
            return Err(RuntimeError::Config(format!(
                "不支持的 facts_format：{format}"
            )));
        }
    }
    // 集成 gate 可让 Docker 自动分配宿主端口，避免多个 PostgreSQL 情景竞争 55432。
    // 显式环境变量只覆盖端口；配置文件仍是 host、database、凭据等连接语义的唯一来源。
    let port = std::env::var("RTOP_POSTGRES_PORT")
        .ok()
        .map(|value| {
            value.parse::<u16>().map_err(|_| {
                RuntimeError::Config("RTOP_POSTGRES_PORT 必须是 1 到 65535 的端口号".into())
            })
        })
        .transpose()?
        .unwrap_or_else(|| config.datasource.port.unwrap_or(5432));
    let postgres = PostgresConnectionConfig {
        host: config.datasource.host,
        port,
        database: config.datasource.database,
        user: config.datasource.user,
        password,
        timestamp_timezone: config.datasource.timestamp_timezone,
    };
    Ok(LoadedConfiguration {
        spec: KnowledgeGraphSpec {
            mapping_file,
            facts_file,
            facts_format: config.facts_format,
            facts_base_iri: config.facts_base_iri,
            ontology_file,
            xml_catalog_file,
        },
        postgres,
        direct_mapping,
        endpoint: EndpointConfiguration {
            enable_download_ontology: endpoint.enable_download_ontology.unwrap_or(false),
            predefined_config: endpoint.predefined_config.map(|file| base.join(file)),
            predefined_queries: endpoint.predefined_queries.map(|file| base.join(file)),
        },
    })
}
