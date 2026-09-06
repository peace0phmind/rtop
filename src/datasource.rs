use crate::RuntimeError;
use fallible_iterator::FallibleIterator;
use postgres::types::{FromSql, Type};
use serde::Serialize;

/// 数据源返回的 Rust 自有单元值；RDF datatype 是 adapter 可选提供的服务器类型信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataValue {
    pub value: String,
    pub datatype: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeospatialValue {
    Boolean(bool),
    Wkt(String),
}

/// 一个 PostgreSQL relation 的基线可观察约束摘要。
///
/// `unique_or_primary_key_count` 合并 PostgreSQL `p` 与 `u` 约束类型，恰好对应
/// Ontop `RelationDefinition.getUniqueConstraints()` 的计数语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationConstraints {
    pub unique_or_primary_key_count: usize,
    pub foreign_key_count: usize,
}

/// Direct Mapping 所需的 PostgreSQL relation 元数据；保持为 Rust 自有值对象，
/// 不泄漏 JDBC `DatabaseMetaData` 或 driver row 类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationMetadata {
    pub columns: Vec<RelationColumn>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<RelationForeignKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationColumn {
    pub name: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
}

/// 可写入 JSON 的 PostgreSQL catalog 快照，字段形状兼容 Ontop metadata 文件的核心部分。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatabaseMetadata {
    pub relations: Vec<DatabaseMetadataRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatabaseMetadataRelation {
    #[serde(rename = "uniqueConstraints")]
    pub unique_constraints: Vec<DatabaseUniqueConstraint>,
    #[serde(rename = "foreignKeys")]
    pub foreign_keys: Vec<DatabaseForeignKey>,
    pub columns: Vec<DatabaseMetadataColumn>,
    pub name: Vec<String>,
    #[serde(rename = "otherNames")]
    pub other_names: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatabaseUniqueConstraint {
    pub name: String,
    pub determinants: Vec<String>,
    #[serde(rename = "isPrimaryKey")]
    pub is_primary_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatabaseForeignKey {
    pub name: String,
    pub from: DatabaseRelationColumns,
    pub to: DatabaseRelationColumns,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatabaseRelationColumns {
    pub relation: Vec<String>,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatabaseMetadataColumn {
    pub name: String,
    #[serde(rename = "isNullable")]
    pub is_nullable: bool,
    pub datatype: String,
}

/// 流式消费的控制信号。`Stop` 会立即停止读取，并释放当前查询的结果流。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamControl {
    Continue,
    Stop,
}

/// 可从另一执行单元发起取消的 Rust 自有句柄；不暴露 PostgreSQL driver 类型。
#[derive(Clone)]
pub struct QueryCancellation {
    token: postgres::CancelToken,
}

impl QueryCancellation {
    /// 请求取消正在执行的查询。服务器端取消具有竞态性；成功发送请求并不表示查询尚未结束。
    pub fn cancel(&self) -> Result<(), RuntimeError> {
        self.token
            .cancel_query(postgres::NoTls)
            .map_err(datasource_error)
    }
}

/// 数据源端口只接收已方言化的 SQL 和值；不泄漏驱动连接/行类型。
pub trait DataSource {
    /// 是否可以执行由多个 mapping plan 组合出的 PostgreSQL BGP JOIN。默认关闭，
    /// 以免内存/测试 adapter 把单条 SQL fixture 误当作数据库查询计划。
    fn supports_postgres_bgp_pushdown(&self) -> bool {
        false
    }

    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError>;

    fn execute_typed(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<DataValue>>>, RuntimeError> {
        self.execute(sql, parameters).map(|rows| {
            rows.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|value| {
                            value.map(|value| DataValue {
                                value,
                                datatype: None,
                            })
                        })
                        .collect()
                })
                .collect()
        })
    }

    /// 逐行消费类型化结果。默认实现保留给内存/测试 adapter；生产 adapter 可避免物化全集。
    fn execute_typed_stream(
        &mut self,
        sql: &str,
        parameters: &[String],
        consume: &mut dyn FnMut(Vec<Option<DataValue>>) -> Result<StreamControl, RuntimeError>,
    ) -> Result<(), RuntimeError> {
        for row in self.execute_typed(sql, parameters)? {
            if consume(row)? == StreamControl::Stop {
                break;
            }
        }
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// 由数据库方言执行 GeoSPARQL 函数；默认 adapter 不提供空间能力。
    fn geospatial(
        &mut self,
        _function: &str,
        _arguments: &[String],
    ) -> Result<Option<GeospatialValue>, RuntimeError> {
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct PostgresConnectionConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    /// 将 PostgreSQL `timestamp without time zone` 视为 UTC instant 后的展示时区。
    pub timestamp_timezone: Option<String>,
}
impl PostgresConnectionConfig {
    fn native_config(&self) -> String {
        format!(
            "host={} port={} dbname={} user={} password={}",
            self.host, self.port, self.database, self.user, self.password
        )
    }
}

pub struct PostgresDataSource {
    client: postgres::Client,
    timestamp_timezone: Option<chrono_tz::Tz>,
}
impl PostgresDataSource {
    pub fn connect(config: &PostgresConnectionConfig) -> Result<Self, RuntimeError> {
        let client = postgres::Client::connect(&config.native_config(), postgres::NoTls)
            .map_err(|e| RuntimeError::DataSource(e.to_string()))?;
        let timestamp_timezone = config
            .timestamp_timezone
            .as_deref()
            .map(str::parse)
            .transpose()
            .map_err(|_| RuntimeError::Config("无效 datasource.timestamp_timezone".into()))?;
        Ok(Self {
            client,
            timestamp_timezone,
        })
    }

    /// 取得可跨线程持有的取消句柄，用于取消此连接上的长查询。
    pub fn cancellation(&self) -> QueryCancellation {
        QueryCancellation {
            token: self.client.cancel_token(),
        }
    }

    /// 查询当前 schema 中一个 relation 的 unique/primary-key 和 foreign-key 数量。
    /// 表名作为绑定参数传入，不参与 SQL 拼接。
    pub fn relation_constraints(
        &mut self,
        table: &str,
    ) -> Result<RelationConstraints, RuntimeError> {
        let rows = self
            .client
            .query(
                "SELECT con.contype::text, count(*)::bigint \
                 FROM pg_constraint AS con \
                 JOIN pg_class AS rel ON rel.oid = con.conrelid \
                 JOIN pg_namespace AS ns ON ns.oid = rel.relnamespace \
                 WHERE ns.nspname = current_schema() \
                   AND rel.relname = $1 \
                   AND con.contype IN ('p', 'u', 'f') \
                 GROUP BY con.contype",
                &[&table],
            )
            .map_err(datasource_error)?;
        let mut constraints = RelationConstraints {
            unique_or_primary_key_count: 0,
            foreign_key_count: 0,
        };
        for row in rows {
            let kind: String = row.try_get(0).map_err(datasource_error)?;
            let count: i64 = row.try_get(1).map_err(datasource_error)?;
            let count = usize::try_from(count)
                .map_err(|_| RuntimeError::DataSource("无效 PostgreSQL constraint count".into()))?;
            match kind.as_str() {
                "p" | "u" => constraints.unique_or_primary_key_count += count,
                "f" => constraints.foreign_key_count += count,
                _ => {}
            }
        }
        Ok(constraints)
    }

    /// 读取 Direct Mapping 所需的 relation column、primary key 与 foreign key 顺序。
    /// relation 名以绑定参数传递；列名来自服务器 catalog，后续生成器必须负责引用。
    pub fn relation_metadata(&mut self, table: &str) -> Result<RelationMetadata, RuntimeError> {
        let columns = self
            .client
            .query(
                "SELECT column_name, is_nullable = 'YES' FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1 ORDER BY ordinal_position",
                &[&table],
            )
            .map_err(datasource_error)?
            .into_iter()
            .map(|row| {
                Ok(RelationColumn {
                    name: row.try_get(0).map_err(datasource_error)?,
                    nullable: row.try_get(1).map_err(datasource_error)?,
                })
            })
            .collect::<Result<Vec<_>, RuntimeError>>()?;
        let primary_key = self
            .client
            .query(
                "SELECT att.attname FROM pg_constraint con \
             JOIN pg_class rel ON rel.oid = con.conrelid \
             JOIN pg_namespace ns ON ns.oid = rel.relnamespace \
             JOIN unnest(con.conkey) WITH ORDINALITY key(attnum, position) ON true \
             JOIN pg_attribute att ON att.attrelid = rel.oid AND att.attnum = key.attnum \
             WHERE ns.nspname = current_schema() AND rel.relname = $1 AND con.contype = 'p' \
             ORDER BY key.position",
                &[&table],
            )
            .map_err(datasource_error)?
            .into_iter()
            .map(|row| row.try_get(0).map_err(datasource_error))
            .collect::<Result<Vec<_>, RuntimeError>>()?;
        let foreign_key_rows = self.client.query(
            "SELECT con.conname, child_att.attname, parent_rel.relname, parent_att.attname \
             FROM pg_constraint con \
             JOIN pg_class child_rel ON child_rel.oid = con.conrelid \
             JOIN pg_namespace ns ON ns.oid = child_rel.relnamespace \
             JOIN pg_class parent_rel ON parent_rel.oid = con.confrelid \
             JOIN unnest(con.conkey, con.confkey) WITH ORDINALITY key(child_attnum, parent_attnum, position) ON true \
             JOIN pg_attribute child_att ON child_att.attrelid = child_rel.oid AND child_att.attnum = key.child_attnum \
             JOIN pg_attribute parent_att ON parent_att.attrelid = parent_rel.oid AND parent_att.attnum = key.parent_attnum \
             WHERE ns.nspname = current_schema() AND child_rel.relname = $1 AND con.contype = 'f' \
             ORDER BY con.conname, key.position",
            &[&table],
        ).map_err(datasource_error)?;
        let mut foreign_keys = Vec::<RelationForeignKey>::new();
        for row in foreign_key_rows {
            let name: String = row.try_get(0).map_err(datasource_error)?;
            let column: String = row.try_get(1).map_err(datasource_error)?;
            let referenced_table: String = row.try_get(2).map_err(datasource_error)?;
            let referenced_column: String = row.try_get(3).map_err(datasource_error)?;
            if let Some(existing) = foreign_keys.iter_mut().find(|key| key.name == name) {
                existing.columns.push(column);
                existing.referenced_columns.push(referenced_column);
            } else {
                foreign_keys.push(RelationForeignKey {
                    name,
                    columns: vec![column],
                    referenced_table,
                    referenced_columns: vec![referenced_column],
                });
            }
        }
        Ok(RelationMetadata {
            columns,
            primary_key,
            foreign_keys,
        })
    }

    /// 提取当前 schema 的 relation/column/unique/foreign-key catalog 快照。
    pub fn database_metadata(&mut self) -> Result<DatabaseMetadata, RuntimeError> {
        let schema: String = self
            .client
            .query_one("SELECT current_schema()", &[])
            .map_err(datasource_error)?
            .try_get(0)
            .map_err(datasource_error)?;
        let tables = self
            .client
            .query(
                "SELECT rel.relname FROM pg_class rel JOIN pg_namespace ns ON ns.oid = rel.relnamespace \
                 WHERE ns.nspname = current_schema() AND rel.relkind IN ('r', 'p') ORDER BY rel.relname",
                &[],
            )
            .map_err(datasource_error)?;
        let mut relations = Vec::new();
        for row in tables {
            let table: String = row.try_get(0).map_err(datasource_error)?;
            let columns = self.client.query(
                "SELECT att.attname, NOT att.attnotnull, upper(format_type(att.atttypid, att.atttypmod)) \
                 FROM pg_attribute att JOIN pg_class rel ON rel.oid = att.attrelid \
                 JOIN pg_namespace ns ON ns.oid = rel.relnamespace \
                 WHERE ns.nspname = current_schema() AND rel.relname = $1 AND att.attnum > 0 AND NOT att.attisdropped \
                 ORDER BY att.attnum", &[&table]).map_err(datasource_error)?
                .into_iter().map(|column| Ok(DatabaseMetadataColumn {
                    name: quoted_identifier(column.try_get(0).map_err(datasource_error)?),
                    is_nullable: column.try_get(1).map_err(datasource_error)?,
                    datatype: column.try_get(2).map_err(datasource_error)?,
                })).collect::<Result<Vec<_>, RuntimeError>>()?;
            let constraint_rows = self.client.query(
                "SELECT con.conname, con.contype::text, att.attname FROM pg_constraint con \
                 JOIN pg_class rel ON rel.oid = con.conrelid JOIN pg_namespace ns ON ns.oid = rel.relnamespace \
                 JOIN unnest(con.conkey) WITH ORDINALITY key(attnum, position) ON true \
                 JOIN pg_attribute att ON att.attrelid = rel.oid AND att.attnum = key.attnum \
                 WHERE ns.nspname = current_schema() AND rel.relname = $1 AND con.contype IN ('p', 'u') \
                 ORDER BY con.conname, key.position", &[&table]).map_err(datasource_error)?;
            let mut unique_constraints = Vec::<DatabaseUniqueConstraint>::new();
            for constraint in constraint_rows {
                let name: String = constraint.try_get(0).map_err(datasource_error)?;
                let kind: String = constraint.try_get(1).map_err(datasource_error)?;
                let column: String = constraint.try_get(2).map_err(datasource_error)?;
                if let Some(existing) = unique_constraints.iter_mut().find(|item| item.name == name)
                {
                    existing.determinants.push(quoted_identifier(column));
                } else {
                    unique_constraints.push(DatabaseUniqueConstraint {
                        name,
                        determinants: vec![quoted_identifier(column)],
                        is_primary_key: kind == "p",
                    });
                }
            }
            let foreign_key_rows = self.client.query(
                "SELECT con.conname, child_att.attname, parent_rel.relname, parent_att.attname FROM pg_constraint con \
                 JOIN pg_class child_rel ON child_rel.oid = con.conrelid JOIN pg_namespace ns ON ns.oid = child_rel.relnamespace \
                 JOIN pg_class parent_rel ON parent_rel.oid = con.confrelid \
                 JOIN unnest(con.conkey, con.confkey) WITH ORDINALITY key(child_attnum, parent_attnum, position) ON true \
                 JOIN pg_attribute child_att ON child_att.attrelid = child_rel.oid AND child_att.attnum = key.child_attnum \
                 JOIN pg_attribute parent_att ON parent_att.attrelid = parent_rel.oid AND parent_att.attnum = key.parent_attnum \
                 WHERE ns.nspname = current_schema() AND child_rel.relname = $1 AND con.contype = 'f' ORDER BY con.conname, key.position", &[&table]).map_err(datasource_error)?;
            let mut foreign_keys = Vec::<DatabaseForeignKey>::new();
            for foreign_key in foreign_key_rows {
                let name: String = foreign_key.try_get(0).map_err(datasource_error)?;
                let column: String = foreign_key.try_get(1).map_err(datasource_error)?;
                let parent: String = foreign_key.try_get(2).map_err(datasource_error)?;
                let parent_column: String = foreign_key.try_get(3).map_err(datasource_error)?;
                if let Some(existing) = foreign_keys.iter_mut().find(|item| item.name == name) {
                    existing.from.columns.push(quoted_identifier(column));
                    existing.to.columns.push(quoted_identifier(parent_column));
                } else {
                    foreign_keys.push(DatabaseForeignKey {
                        name,
                        from: DatabaseRelationColumns {
                            relation: vec![quoted_identifier(table.clone())],
                            columns: vec![quoted_identifier(column)],
                        },
                        to: DatabaseRelationColumns {
                            relation: vec![quoted_identifier(parent)],
                            columns: vec![quoted_identifier(parent_column)],
                        },
                    });
                }
            }
            relations.push(DatabaseMetadataRelation {
                unique_constraints,
                foreign_keys,
                columns,
                name: vec![quoted_identifier(table.clone())],
                other_names: vec![vec![
                    quoted_identifier(schema.clone()),
                    quoted_identifier(table),
                ]],
            });
        }
        Ok(DatabaseMetadata { relations })
    }
}

fn quoted_identifier(value: String) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
impl DataSource for PostgresDataSource {
    fn supports_postgres_bgp_pushdown(&self) -> bool {
        true
    }

    fn execute(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<String>>>, RuntimeError> {
        let values: Vec<&(dyn postgres::types::ToSql + Sync)> = parameters
            .iter()
            .map(|p| p as &(dyn postgres::types::ToSql + Sync))
            .collect();
        let rows = self.client.query(sql, &values).map_err(datasource_error)?;
        rows.into_iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| value(&row, i, self.timestamp_timezone))
                    .collect()
            })
            .collect()
    }

    fn execute_typed(
        &mut self,
        sql: &str,
        parameters: &[String],
    ) -> Result<Vec<Vec<Option<DataValue>>>, RuntimeError> {
        let values: Vec<&(dyn postgres::types::ToSql + Sync)> = parameters
            .iter()
            .map(|p| p as &(dyn postgres::types::ToSql + Sync))
            .collect();
        self.client
            .query(sql, &values)
            .map_err(datasource_error)?
            .into_iter()
            .map(|row| {
                (0..row.len())
                    .map(|index| {
                        let datatype = rdf_datatype(row.columns()[index].type_());
                        value(&row, index, self.timestamp_timezone).map(|value| {
                            value.map(|value| DataValue {
                                value,
                                datatype: datatype.map(str::to_owned),
                            })
                        })
                    })
                    .collect()
            })
            .collect()
    }

    fn execute_typed_stream(
        &mut self,
        sql: &str,
        parameters: &[String],
        consume: &mut dyn FnMut(Vec<Option<DataValue>>) -> Result<StreamControl, RuntimeError>,
    ) -> Result<(), RuntimeError> {
        let values: Vec<&(dyn postgres::types::ToSql + Sync)> = parameters
            .iter()
            .map(|p| p as &(dyn postgres::types::ToSql + Sync))
            .collect();
        let mut rows = self
            .client
            .query_raw(sql, values)
            .map_err(datasource_error)?;
        while let Some(row) = rows.next().map_err(datasource_error)? {
            let values = (0..row.len())
                .map(|index| {
                    let datatype = rdf_datatype(row.columns()[index].type_());
                    value(&row, index, self.timestamp_timezone).map(|value| {
                        value.map(|value| DataValue {
                            value,
                            datatype: datatype.map(str::to_owned),
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if consume(values)? == StreamControl::Stop {
                break;
            }
        }
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), RuntimeError> {
        self.cancellation().cancel()
    }

    fn geospatial(
        &mut self,
        function: &str,
        arguments: &[String],
    ) -> Result<Option<GeospatialValue>, RuntimeError> {
        let Some(call) = PostgisCall::parse(function, arguments) else {
            return Ok(None);
        };
        let parameters = call.parameters();
        let values = parameters
            .iter()
            .map(|value| value as &(dyn postgres::types::ToSql + Sync))
            .collect::<Vec<_>>();
        let row = self
            .client
            .query_one(call.sql(), &values)
            .map_err(datasource_error)?;
        match call.result() {
            GeospatialResult::Boolean => row
                .try_get::<_, Option<bool>>(0)
                .map(|value| value.map(GeospatialValue::Boolean))
                .map_err(datasource_error),
            GeospatialResult::Wkt => row
                .try_get::<_, Option<String>>(0)
                .map(|value| value.map(GeospatialValue::Wkt))
                .map_err(datasource_error),
        }
    }
}

enum GeospatialResult {
    Boolean,
    Wkt,
}

/// PostGIS adapter 的内部调用模型：将 GeoSPARQL 名称、arity、单位与绑定参数收敛在
/// 一个私有模块中，调用方不必了解 PostGIS SQL 或可支持的单位集合。
enum PostgisCall<'a> {
    SfIntersects { left: &'a str, right: &'a str },
    BufferMetre { wkt: &'a str, distance: &'a str },
    Intersection { left: &'a str, right: &'a str },
}

impl<'a> PostgisCall<'a> {
    fn parse(function: &str, arguments: &'a [String]) -> Option<Self> {
        let function = function.trim().to_ascii_uppercase();
        match (function.as_str(), arguments) {
            (
                "<HTTP://WWW.OPENGIS.NET/DEF/FUNCTION/GEOSPARQL/SFINTERSECTS>"
                | "GEOF:SFINTERSECTS",
                [left, right],
            ) => Some(Self::SfIntersects { left, right }),
            (
                "<HTTP://WWW.OPENGIS.NET/DEF/FUNCTION/GEOSPARQL/BUFFER>" | "GEOF:BUFFER",
                [wkt, distance, unit],
            ) if is_metre_unit(unit) => Some(Self::BufferMetre { wkt, distance }),
            (
                "<HTTP://WWW.OPENGIS.NET/DEF/FUNCTION/GEOSPARQL/INTERSECTION>"
                | "GEOF:INTERSECTION",
                [left, right],
            ) => Some(Self::Intersection { left, right }),
            _ => None,
        }
    }

    fn sql(&self) -> &'static str {
        match self {
            Self::SfIntersects { .. } => {
                "SELECT ST_Intersects(ST_GeomFromText($1::text), ST_GeomFromText($2::text))"
            }
            Self::BufferMetre { .. } => {
                "SELECT ST_AsText(ST_Buffer(ST_GeogFromText($1::text), $2::text::double precision)::geometry)"
            }
            Self::Intersection { .. } => {
                "SELECT ST_AsText(ST_Intersection(ST_GeomFromText($1::text), ST_GeomFromText($2::text)))"
            }
        }
    }

    fn parameters(&self) -> [&'a str; 2] {
        match self {
            Self::SfIntersects { left, right } | Self::Intersection { left, right } => {
                [left, right]
            }
            Self::BufferMetre { wkt, distance } => [wkt, distance],
        }
    }

    fn result(&self) -> GeospatialResult {
        match self {
            Self::SfIntersects { .. } => GeospatialResult::Boolean,
            Self::BufferMetre { .. } | Self::Intersection { .. } => GeospatialResult::Wkt,
        }
    }
}

fn is_metre_unit(value: &str) -> bool {
    matches!(
        value.trim(),
        "http://www.opengis.net/def/uom/OGC/1.0/metre"
            | "<http://www.opengis.net/def/uom/OGC/1.0/metre>"
            | "uom:metre"
            | "UOM:METRE"
    )
}

#[cfg(test)]
mod tests {
    use super::PostgisCall;

    #[test]
    fn accepts_only_the_supported_geosparql_buffer_unit() {
        let arguments = vec![
            "POINT(2 2)".to_owned(),
            "20".to_owned(),
            "http://www.opengis.net/def/uom/OGC/1.0/metre".to_owned(),
        ];
        assert!(matches!(
            PostgisCall::parse("GEOF:BUFFER", &arguments),
            Some(PostgisCall::BufferMetre { .. })
        ));

        let unsupported_unit = vec![
            "POINT(2 2)".to_owned(),
            "20".to_owned(),
            "http://www.opengis.net/def/uom/OGC/1.0/degree".to_owned(),
        ];
        assert!(PostgisCall::parse("GEOF:BUFFER", &unsupported_unit).is_none());
    }
}

fn value(
    row: &postgres::Row,
    index: usize,
    timestamp_timezone: Option<chrono_tz::Tz>,
) -> Result<Option<String>, RuntimeError> {
    use postgres::types::Type;
    let ty = row.columns()[index].type_();
    match *ty {
        Type::BOOL => row
            .try_get::<_, Option<bool>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::INT2 => row
            .try_get::<_, Option<i16>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::NUMERIC => row
            .try_get::<_, Option<NumericText>>(index)
            .map(|value| value.map(|value| value.0))
            .map_err(datasource_error),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::FLOAT8 => row
            .try_get::<_, Option<f64>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::DATE => row
            .try_get::<_, Option<chrono::NaiveDate>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::TIME => row
            .try_get::<_, Option<chrono::NaiveTime>>(index)
            .map(|value| value.map(|value| value.to_string()))
            .map_err(datasource_error),
        Type::TIMETZ => row
            .try_get::<_, Option<TimeTzText>>(index)
            .map(|value| value.map(|value| value.0))
            .map_err(datasource_error),
        Type::INTERVAL => row
            .try_get::<_, Option<IntervalText>>(index)
            .map(|value| value.map(|value| value.0))
            .map_err(datasource_error),
        Type::TIMESTAMP => row
            .try_get::<_, Option<chrono::NaiveDateTime>>(index)
            .map(|value| {
                value.map(|value| match timestamp_timezone {
                    Some(timezone) => value
                        .and_utc()
                        .with_timezone(&timezone)
                        .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, false),
                    None => value
                        .and_utc()
                        .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
                        .strip_suffix('Z')
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .map_err(datasource_error),
        Type::TIMESTAMPTZ => row
            .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(index)
            .map(|value| {
                value.map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Micros, true))
            })
            .map_err(datasource_error),
        Type::BYTEA => row
            .try_get::<_, Option<Vec<u8>>>(index)
            .map(|value| {
                value.map(|bytes| {
                    bytes
                        .iter()
                        .map(|byte| format!("{byte:02X}"))
                        .collect::<String>()
                })
            })
            .map_err(datasource_error),
        _ => row
            .try_get::<_, Option<String>>(index)
            .map_err(datasource_error),
    }
}

fn datasource_error(error: postgres::Error) -> RuntimeError {
    if error.code().is_some_and(|code| code.code() == "57014") {
        return RuntimeError::DataSource("query-cancelled".into());
    }
    RuntimeError::DataSource(error.to_string())
}

fn rdf_datatype(ty: &postgres::types::Type) -> Option<&'static str> {
    use postgres::types::Type;
    match *ty {
        Type::BOOL => Some("http://www.w3.org/2001/XMLSchema#boolean"),
        Type::INT2 | Type::INT4 | Type::INT8 => Some("http://www.w3.org/2001/XMLSchema#integer"),
        Type::NUMERIC => Some("http://www.w3.org/2001/XMLSchema#decimal"),
        // RDB2RDF Direct Mapping baseline 对 SQL REAL 与 FLOAT 都使用 xsd:double；
        // PostgreSQL wire type FLOAT4 在此 adapter 边界同样采用该公开 RDF 契约。
        Type::FLOAT4 => Some("http://www.w3.org/2001/XMLSchema#double"),
        Type::FLOAT8 => Some("http://www.w3.org/2001/XMLSchema#double"),
        Type::DATE => Some("http://www.w3.org/2001/XMLSchema#date"),
        Type::TIME | Type::TIMETZ => Some("http://www.w3.org/2001/XMLSchema#time"),
        Type::TIMESTAMP | Type::TIMESTAMPTZ => Some("http://www.w3.org/2001/XMLSchema#dateTime"),
        Type::BYTEA => Some("http://www.w3.org/2001/XMLSchema#hexBinary"),
        _ => None,
    }
}

/// PostgreSQL NUMERIC 的二进制 wire 表示。driver 未提供十进制文本实现，因此在 adapter
/// 边界解码为 PostgreSQL 已规范化的十进制词法值，而不将数据库数值降级为无类型文本。
struct NumericText(String);

impl<'a> FromSql<'a> for NumericText {
    fn from_sql(_: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() < 8 || (raw.len() - 8) % 2 != 0 {
            return Err("无效 PostgreSQL NUMERIC 二进制值".into());
        }
        let ndigits = i16::from_be_bytes([raw[0], raw[1]]);
        let weight = i16::from_be_bytes([raw[2], raw[3]]);
        let sign = u16::from_be_bytes([raw[4], raw[5]]);
        let scale = u16::from_be_bytes([raw[6], raw[7]]) as usize;
        if ndigits < 0 || raw.len() != 8 + ndigits as usize * 2 {
            return Err("无效 PostgreSQL NUMERIC digit count".into());
        }
        if sign == 0xC000 {
            return Ok(Self("NaN".into()));
        }
        if sign != 0 && sign != 0x4000 {
            return Err("无效 PostgreSQL NUMERIC sign".into());
        }
        let digits = raw[8..]
            .chunks_exact(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        if digits.iter().any(|digit| *digit >= 10_000) {
            return Err("无效 PostgreSQL NUMERIC digit".into());
        }
        let digit_at = |exponent: i16| {
            let index = weight - exponent;
            (index >= 0)
                .then(|| digits.get(index as usize).copied())
                .flatten()
                .unwrap_or(0)
        };
        let mut value: String = if weight < 0 {
            "0".into()
        } else {
            (0..=weight)
                .rev()
                .map(|exponent| {
                    let digit = digit_at(exponent);
                    if exponent == weight {
                        digit.to_string()
                    } else {
                        format!("{digit:04}")
                    }
                })
                .collect()
        };
        if scale > 0 {
            let fraction_groups = ((scale + 3) / 4) as i16;
            let fraction = (1..=fraction_groups)
                .map(|offset| format!("{:04}", digit_at(-offset)))
                .collect::<String>();
            value.push('.');
            value.push_str(&fraction[..scale]);
        }
        if sign == 0x4000
            && value
                .chars()
                .any(|character| character.is_ascii_digit() && character != '0')
        {
            value.insert(0, '-');
        }
        Ok(Self(value))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::NUMERIC
    }
}

/// PostgreSQL TIMETZ 在 wire 上是“午夜后微秒数 + UTC 西侧秒偏移”。将其转换为 xsd:time
/// 规范可打印形式，保留原始墙上时间和偏移，而非错误地按本地时区重新解释。
struct TimeTzText(String);

impl<'a> FromSql<'a> for TimeTzText {
    fn from_sql(_: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() != 12 {
            return Err("无效 PostgreSQL TIMETZ 二进制值".into());
        }
        let micros = i64::from_be_bytes(raw[..8].try_into().expect("length checked"));
        if !(0..=86_400_000_000).contains(&micros) {
            return Err("无效 PostgreSQL TIMETZ 时间".into());
        }
        let west_seconds = i32::from_be_bytes(raw[8..].try_into().expect("length checked"));
        let offset_seconds = west_seconds
            .checked_neg()
            .ok_or("无效 PostgreSQL TIMETZ 偏移")?;
        let absolute_offset = offset_seconds.unsigned_abs();
        if absolute_offset > 15 * 60 * 60 || absolute_offset % 60 != 0 {
            return Err("无效 PostgreSQL TIMETZ 偏移".into());
        }
        let hours = micros / 3_600_000_000;
        let minutes = (micros / 60_000_000) % 60;
        let seconds = (micros / 1_000_000) % 60;
        let fraction = micros % 1_000_000;
        let mut value = format!("{hours:02}:{minutes:02}:{seconds:02}");
        if fraction != 0 {
            value.push('.');
            value.push_str(format!("{fraction:06}").trim_end_matches('0'));
        }
        if offset_seconds == 0 {
            value.push('Z');
        } else {
            value.push(if offset_seconds > 0 { '+' } else { '-' });
            value.push_str(&format!(
                "{:02}:{:02}",
                absolute_offset / 3600,
                (absolute_offset / 60) % 60
            ));
        }
        Ok(Self(value))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::TIMETZ
    }
}

/// PostgreSQL INTERVAL 的 wire 格式为微秒、天、月三元组。rtop 保持 PostgreSQL 的稳定
/// 人类可读词法形式，供显式 `^^xsd:string` mapping 使用；不把月近似为固定天数。
struct IntervalText(String);

impl<'a> FromSql<'a> for IntervalText {
    fn from_sql(_: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() != 16 {
            return Err("无效 PostgreSQL INTERVAL 二进制值".into());
        }
        let micros = i64::from_be_bytes(raw[..8].try_into().expect("length checked"));
        let days = i32::from_be_bytes(raw[8..12].try_into().expect("length checked"));
        let months = i32::from_be_bytes(raw[12..].try_into().expect("length checked"));
        let mut parts = Vec::new();
        if months != 0 {
            parts.push(format!(
                "{months} {}",
                if months.abs() == 1 { "mon" } else { "mons" }
            ));
        }
        if days != 0 {
            parts.push(format!(
                "{days} {}",
                if days.abs() == 1 { "day" } else { "days" }
            ));
        }
        let sign = if micros < 0 { "-" } else { "" };
        let absolute = micros.unsigned_abs();
        let hours = absolute / 3_600_000_000;
        let minutes = (absolute / 60_000_000) % 60;
        let seconds = (absolute / 1_000_000) % 60;
        let fraction = absolute % 1_000_000;
        let mut time = format!("{sign}{hours:02}:{minutes:02}:{seconds:02}");
        if fraction != 0 {
            time.push('.');
            time.push_str(format!("{fraction:06}").trim_end_matches('0'));
        }
        if micros != 0 || parts.is_empty() {
            parts.push(time);
        }
        Ok(Self(parts.join(" ")))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::INTERVAL
    }
}
