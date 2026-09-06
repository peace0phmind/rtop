# Ontop 相对 rtop 的功能差异

**结论。** Ontop 的代码量主要多在三类能力：多数据库/JDBC 方言、Java 宿主集成
（RDF4J、OWLAPI、Protégé）和成熟 HTTP 服务包装。它不意味着 rtop 尚未具备
VKG 的基本闭环：当前 rtop 已有 PostgreSQL 上的 OBDA/R2RML/Direct Mapping、RDF
facts、受限 OWL 2 QL 改写、SPARQL 查询、物化、CLI 和 `/sparql` endpoint。实际缺口
应按下表理解，不能把刻意不迁移的 Java 进程内 API 算作 Rust 产品的“缺功能”。

## 对照范围与方法

- Ontop：只读 `../ontop` 的提交 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。
- rtop：本工作树当前受跟踪源码。`docs/adr/0001-postgresql-limited-observable-equivalence.md:3`
  已明确产品边界是 PostgreSQL 限定的跨语言可观察行为，而不是复制
  Java/JDBC/RDF4J/OWLAPI/Protégé 宿主类型。
- 下表的“已支持”是可由 rtop 源码直接验证的能力，不把测试资产数量当作功能本身。

## 主要功能差异

| 领域 | Ontop 有而 rtop 尚缺/范围更窄 | rtop 已支持 | 一手依据 |
| --- | --- | --- | --- |
| 数据库和 SQL 方言 | 通过 JDBC 覆盖多种数据库及其元数据、类型、函数和 SQL 生成方言；源码含 MySQL、Oracle、SQL Server、DB2、BigQuery、DuckDB、Trino、Spark、Snowflake 等类型工厂。rtop 运行时只接受 `datasource.kind = "postgres"`，且实现只有原生 PostgreSQL adapter。 | PostgreSQL（含 PostGIS 路径）的连接、元数据、取消、流式结果和 GeoSPARQL 下推。 | Ontop `pom.xml:48-58` 列出 `db` 等模块；`db/rdb/src/main/java/it/unibz/inf/ontop/model/type/impl/{MySQL,Oracle,SQLServer,BigQuery,DuckDB,Trino}*DBTypeFactory.java`；rtop `src/config.rs:78-92`、`src/datasource.rs:121-177,198-216`。 |
| OWL 本体处理与推理 | Ontop 的 OWLAPI 集成提供 `OntopOWLEngine`、连接、语句和结果集对象；其完整 OWL 2 QL 装载/分类路径比 rtop 的最小 TBox 更广。rtop 不支持非文件 `owl:imports`，且注释明确匿名 class expression 不在当前范围。 | 文件 Turtle/RDF/XML 本体、imports 闭包、类/属性层级、domain/range、inverse、部分等价类/交集规范化、互斥类检查。 | Ontop `binding/owlapi/src/main/java/it/unibz/inf/ontop/owlapi/OntopOWLEngine.java:13-16` 及同模块的 `connection/`、`resultset/`；rtop `src/ontology.rs:25-35,179-289`，尤其 `246-249`、`281-286`。 |
| RDF4J 嵌入式 Java API | Ontop 可作为 RDF4J `Repository` 被 Java 应用嵌入，带 connection、查询及结果对象。rtop 没有等价的 Java API（这是既定非目标）。 | Rust 自有 `VkgRuntime`、CLI 与标准 HTTP SPARQL 查询接口；RDF facts 和 R2RML 的可观察语义已迁移。 | Ontop `binding/rdf4j/src/main/java/it/unibz/inf/ontop/rdf4j/repository/OntopRepository.java:8-20`；rtop `src/lib.rs:1`，`compatibility-report.json:100,114`。 |
| Protégé 图形化插件 | Ontop 提供 Protégé/OSGi/Swing 桌面集成：映射编辑、数据源与 JDBC 驱动设置、SPARQL 查询面板、R2RML 导入导出及物化动作。rtop 没有桌面插件（既定非目标）。 | 可用 CLI 完成映射转换、验证、查询、物化和 bootstrap。 | Ontop `protege/plugin/src/main/java/it/unibz/inf/ontop/protege/{mapping,query,connection,action}/`；rtop `src/main.rs:30-62,64-120`。 |
| HTTP 服务运维/UI | Ontop endpoint 有 Spring Boot/Tomcat 层、可配置 CORS、异步 restart、portal 配置/页面等；rtop 的最小 TCP HTTP adapter 未实现 CORS、portal 或 restart。 | `/sparql`（GET/POST）、`/ontology`、`/predefined/*`、开发态 `/ontop/reformulate` 和 `/healthz`。 | Ontop `client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/OntopEndpointApplication.java:14-53` 及 `controllers/{AutoRestart,Portal,PortalConfig}Controller.java`；rtop `src/server.rs:11-24,60-107`。 |
| CLI 易用性与发布配套 | Ontop CLI 另有 `version`、`help` 与 shell completion，且有 Maven/JAR/assembly 分发体系。rtop 当前主命令没有这些子命令/补全。 | 两边的核心操作均可从 CLI 触达：query、validate、compile、endpoint、materialize、bootstrap、metadata 提取和映射转换；rtop 还提供 OCI 镜像默认 endpoint。 | Ontop `client/cli/src/main/java/it/unibz/inf/ontop/cli/Ontop.java:35-65`；rtop `src/main.rs:30-62,64-120`，`README.md:3-14`。 |

## 不是当前主要缺口的部分

以下能力已经进入 rtop 的 PostgreSQL 目标范围，因此不应因 Ontop 代码库更大而重复
计为缺口：

- **映射与 RDF 输入：** rtop 配置接受 mapping、facts、ontology 和 Direct Mapping
  （`src/config.rs:7-46,124-163`）；兼容性报告明确记录 R2RML、facts 与命名图语义。
- **查询/物化/元数据/bootstrap：** rtop 已实现对应 CLI 分支
  （`src/main.rs:64-120,176-229`），而非只有 demo endpoint。
- **基本本体查询改写：** rtop 会针对 subclass、subproperty、domain/range、inverse
  维护查询所需的关系（`src/ontology.rs:79-140`）。这与“完整 OWLAPI 生态”不同，
  但不是“完全没有 OWL”。
- **地理空间：** 数据源端口专门定义 GeoSPARQL 执行接口
  （`src/datasource.rs:169-176`），且 `compatibility-report.json:2251-2257` 记录了
  PostGIS 17/3.5.2 的实测情景；不应把 GeoSPARQL 列为当前缺口。

## 优先级判断

若目标是扩大 rtop 的**跨语言产品能力**，最大且最有价值的增量是新增独立的
数据源 adapter/方言验收（先选一个实际需要的服务端数据库）。若目标是让既有 Java
用户无改动迁移，则 RDF4J/OWLAPI/Protégé 和完整 endpoint portal 才是缺口，但它们与
ADR 已采纳的 Rust 原生边界相冲突，须先重新作出产品决策。

