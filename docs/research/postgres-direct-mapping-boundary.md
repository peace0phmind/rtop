# PostgreSQL Direct Mapping 边界

#51 的 Direct Mapping 从 `PostgresDataSource::relation_metadata` 取得 relation 的有序
column、primary key 和 foreign key。该 adapter port 返回 Rust 自有值对象，不暴露
JDBC `DatabaseMetaData`、PostgreSQL driver row 或连接类型。

Direct Mapping planner 应把 metadata 转为既有 `Mapping` 规则/SQL plan；
`VkgRuntime` 继续只执行 Mapping 与 SPARQL，SPARQL parser 和本体层不读取 PostgreSQL
catalog。这样保证从 D001/D004/D017 开始的表/键/引用标识符语义仍由同一查询 seam
观察，而不是新建旁路 RDF 存储。
