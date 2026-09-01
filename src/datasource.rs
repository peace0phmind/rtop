use crate::RuntimeError;

/// 数据源端口只接收已方言化的 SQL 和值；不泄漏驱动连接/行类型。
pub trait DataSource { fn execute(&mut self, sql: &str, parameters: &[String]) -> Result<Vec<Vec<String>>, RuntimeError>; }

#[derive(Debug, Clone)]
pub struct PostgresConnectionConfig { pub host: String, pub port: u16, pub database: String, pub user: String, pub password: String }
impl PostgresConnectionConfig {
    fn native_config(&self) -> String { format!("host={} port={} dbname={} user={} password={}", self.host, self.port, self.database, self.user, self.password) }
}

pub struct PostgresDataSource { client: postgres::Client }
impl PostgresDataSource {
    pub fn connect(config: &PostgresConnectionConfig) -> Result<Self, RuntimeError> {
        let client = postgres::Client::connect(&config.native_config(), postgres::NoTls).map_err(|e| RuntimeError::DataSource(e.to_string()))?;
        Ok(Self { client })
    }
}
impl DataSource for PostgresDataSource {
    fn execute(&mut self, sql: &str, parameters: &[String]) -> Result<Vec<Vec<String>>, RuntimeError> {
        let values: Vec<&(dyn postgres::types::ToSql + Sync)> = parameters.iter().map(|p| p as &(dyn postgres::types::ToSql + Sync)).collect();
        let rows = self.client.query(sql, &values).map_err(|e| RuntimeError::DataSource(e.to_string()))?;
        rows.into_iter().map(|row| (0..row.len()).map(|i| row.try_get::<_, Option<String>>(i)
            .map_err(|e| RuntimeError::DataSource(e.to_string()))
            .map(|value| value.unwrap_or_default())).collect()).collect()
    }
}
