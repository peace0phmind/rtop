//! `rtop` 的稳定内核边界。此模块不公开 PostgreSQL 驱动类型。
mod config;
mod datasource;
mod mapping;
mod model;
mod sparql;

pub use config::{load_configuration, KnowledgeGraphSpec, LoadedConfiguration};
pub use datasource::{DataSource, PostgresConnectionConfig, PostgresDataSource};
pub use model::{Binding, QueryResult, RdfTerm, RuntimeError};

use mapping::Mapping;
use sparql::SelectQuery;

/// VKG 的唯一高层 seam：加载配置并执行查询。
pub struct VkgRuntime<D> {
    spec: KnowledgeGraphSpec,
    source: D,
    mapping: Mapping,
}

impl<D: DataSource> VkgRuntime<D> {
    pub fn new(spec: KnowledgeGraphSpec, source: D) -> Result<Self, RuntimeError> {
        let mapping = Mapping::parse(
            &std::fs::read_to_string(&spec.mapping_file)
                .map_err(|e| RuntimeError::Config(format!("无法读取 mapping：{e}")))?,
        )?;
        Ok(Self {
            spec,
            source,
            mapping,
        })
    }

    /// 目前交付 SELECT 基本图模式；其余语法会得到稳定的 `unsupported-sparql`。
    pub fn query(&mut self, sparql: &str) -> Result<QueryResult, RuntimeError> {
        let query = SelectQuery::parse(sparql)?;
        let plan = self.mapping.reformulate(&query)?;
        let rows = self.source.execute(&plan.sql, &plan.parameters)?;
        let bindings = rows
            .into_iter()
            .map(|row| {
                plan.variables
                    .iter()
                    .enumerate()
                    .map(|(index, name)| (name.clone(), RdfTerm::Iri(row[index].clone())))
                    .collect::<Binding>()
            })
            .collect();
        Ok(QueryResult::Bindings(bindings))
    }

    pub fn spec(&self) -> &KnowledgeGraphSpec {
        &self.spec
    }
}
