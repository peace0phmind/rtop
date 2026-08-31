# Rust 数据访问栈：JDBC 替代的覆盖边界

**问题。** `rtop` 不携带 JVM/JDBC 或 Java bridge 时，是否存在一个像 JDBC 一样、可统一接入 DuckDB、Athena、BigQuery、Dremio、Denodo、Presto、Trino、Spark、TDEngine、Teiid、Oracle 与 DB2 的 Rust 协议栈？

**结论（2026-08-31）。** 没有。现有技术可以提供统一的 *Rust 调用接口*，或覆盖一部分共享协议，却没有一个以 Rust 实现底层协议、覆盖上述全部产品的 JDBC 等价物。不能把“能够从 Rust 调用 C/ODBC/ADBC 动态库”误写成“Rust 原生协议支持”。

## 三种容易混淆的“通用”层

| 技术 | 能统一什么 | 为什么不是本项目的完全答案 |
| --- | --- | --- |
| [SQLx](https://docs.rs/sqlx/latest/sqlx/) / [Diesel](https://docs.rs/diesel/latest/diesel/) | Rust API 和常见关系库驱动；SQLx 文档列出的内建数据库模块为 PostgreSQL、MySQL、SQLite | 不为本问题列出的云查询引擎与厂商系统提供可插拔通用驱动模型。 |
| [ODBC + `odbc-api`](https://docs.rs/odbc-api/latest/odbc_api/) | 统一调用 ODBC C API；理论上可接任何安装了厂商 ODBC 驱动的数据源 | 底层仍是厂商 ODBC 驱动和 C ABI，不是 Rust 协议实现；应在“纯 Rust 协议栈”约束下排除。 |
| [Apache Arrow ADBC](https://arrow.apache.org/adbc/current/) | Arrow 原生的数据库 API/驱动 ABI；[Rust API](https://arrow.apache.org/adbc/current/rust/) 有 `adbc_core` 与 driver manager | ADBC 是 API/驱动 ABI，不是一个覆盖所有数据库的 wire protocol。官方 [驱动目录](https://arrow.apache.org/adbc/current/driver/index.html) 明说驱动通常是可由任意语言编写、动态加载的 `.so/.dll/.dylib`。上游 Rust workspace 目前仅有 `dummy` driver（[workspace 清单](https://github.com/apache/arrow-adbc/tree/main/rust/driver)），故不能据此宣称所有驱动是 Rust 原生。 |

ADBC 值得保留为 `rtop` 的**可选结果/驱动边界**：它能让 Rust 核心以 Arrow 数据批次表示结果，并在日后允许加载合规驱动；但不得成为“本期各源已得到 Rust 支持”的验收依据。

另一个有价值但范围较小的共同协议是 [Arrow Flight SQL](https://arrow.apache.org/docs/format/FlightSql.html)。它是基于 Arrow Flight/gRPC 的 SQL 协议，客户端可与实现了相应端点的服务通信；Rust 的 [`arrow-flight`](https://docs.rs/arrow-flight/latest/arrow_flight/) 提供 Flight SQL 模块。它仅覆盖服务端实际提供 Flight SQL 的产品，不是 JDBC 替代品。

## 与 Ontop 现有 JDBC 数据源的对照

“可行”只表示不依赖 JVM/JDBC；不表示已与 Ontop 行为等价。`rtop` 仍要逐源验证认证、参数、类型、NULL、时区、目录元数据、取消与流式结果。

| 数据源 | 不依赖 JVM 的接入事实 | 严格 Rust 协议栈判断 | 迁移建议 |
| --- | --- | --- | --- |
| DuckDB | [`duckdb-rs`](https://docs.rs/duckdb/latest/duckdb/) 是 Rust 对 DuckDB 的封装，并公开 `duckdb_sys` FFI。ADBC 目录也列出 DuckDB 驱动。 | **非 Java，但非纯 Rust**：常规 Rust 接入依赖嵌入式 DuckDB 原生库；它不是网络协议客户端。 | 若允许原生库 FFI，可单列为嵌入式后端；若“纯 Rust”连 C/C++ FFI 也禁止，则排除。 |
| Athena | AWS 提供 [Rust `aws-sdk-athena`](https://docs.rs/aws-sdk-athena/latest/aws_sdk_athena/)；其 [StartQueryExecution API](https://docs.aws.amazon.com/athena/latest/APIReference/API_StartQueryExecution.html) 是提交 SQL 后轮询/取结果的服务 API。 | **可 Rust 原生 API 接入，但不是通用连接协议**。 | 做独立 Athena HTTP/AWS SDK 适配器，不能塞进 JDBC 式同步连接假设。 |
| BigQuery | [BigQuery REST API](https://cloud.google.com/bigquery/docs/reference/rest) 可由 Rust HTTP/OAuth 客户端实现；ADBC 驱动目录列 BigQuery，但为共享库驱动生态。 | **可协议原生实现**，但需单独实现 Google 认证、作业与分页语义；ADBC 不能证明 Rust-only。 | 独立 REST 适配器；或接受非 Rust ADBC 二进制时才使用 ADBC。 |
| Dremio | ADBC 的 Flight SQL 驱动说明列 Dremio 为兼容服务；Flight SQL 是公开协议。 | **可行的共享 Rust 协议路径**，前提是部署启用并兼容 Flight SQL。 | 优先 `arrow-flight`/Flight SQL 适配器，并以目标 Dremio 版本做互操作测试。 |
| Trino | [客户端协议](https://trino.io/docs/current/develop/client-protocol.html) 定义 `POST /v1/statement`、JSON 结果与 `nextUri` 轮询；ADBC 目录也列 Trino 共享库驱动。 | **可用 Rust HTTP 直接实现**，但不是与 Presto 保证同一的通用驱动。 | 建立 Trino HTTP 适配器；单测协议轮询、取消、认证与类型 JSON。 |
| Presto | 与 Trino 同源的 HTTP 查询模式可供实现，但版本/产品分叉不应假定完全兼容。 | **可做 HTTP 适配器，非通用保证。** | Presto 另列适配器或经过协议兼容测试后共享实现。 |
| Spark | ADBC 目录列 Apache Spark 驱动，但它是共享库发行；Spark 的常见 SQL 服务入口仍常是 JDBC/ODBC/Thrift 服务。 | **没有由上述资料证明的通用 pure-Rust JDBC 替代。** | 仅在目标 Spark 提供 Flight SQL 或明确选定的非 JVM 协议时纳入；否则暂缓。 |
| Denodo | Ontop 当前经 JDBC 接入并不意味着存在 Rust wire client。 | **未发现可作为基线承诺的 Rust 原生通用协议。** | 暂不迁移；如业务要求，先调研其已启用的 HTTP/OData/ODBC/专有接口。 |
| TDEngine | 不能从 JDBC 依赖推出 Rust 驱动的协议与行为等价。 | **需要单源验证。** | 作为独立协议调研/原型票据，未验证前不纳入首期。 |
| Teiid | 同上；JDBC 适配不等于可复用的 Rust 服务协议。 | **需要单源验证。** | 暂不承诺；若只开放 JDBC/ODBC，则与严格约束冲突。 |
| Oracle | [`oracle`](https://docs.rs/oracle/latest/oracle/) 明确基于 ODPI-C，并要求 Oracle Client。 | **非 Java，但非 pure-Rust**。 | 只有允许 Oracle Client/FFI 时才可作为可选后端；否则排除。 |
| DB2 | 可走 ODBC，但这需要 IBM ODBC 驱动；它不构成 Rust 原生协议实现。 | **无已确认的 pure-Rust 通用路径。** | 暂缓，除非单独找到并验证目标 DB2 版本的 Rust wire/HTTP 客户端。 |

## 对 `rtop` issue 分解的影响

不要建立“实现 Rust JDBC”这一 issue。应建立一个阻塞性决策票据：**“定义 `rtop` 可接受的非 Java 数据源适配等级”**，并明确以下二选一边界：

1. **严格 Rust 协议级。** 适配器必须用 Rust 直接实现/调用公开服务协议；允许的共同层仅为 Flight SQL、HTTP API、数据库原生 wire protocol。排除 ODBC、JDBC、Java sidecar、以及依赖厂商 C client 的 DuckDB/Oracle。首批可验证候选为 PostgreSQL（另行验证）、Athena、BigQuery、Trino，及部署启用 Flight SQL 的 Dremio。
2. **非 Java 原生运行时。** 禁止 JVM，但接受 C/C++ 共享库/ADBC/ODBC。这样能扩大 DuckDB、Oracle、DB2 与 ADBC 驱动覆盖，不过验收必须记录二进制、许可证、平台和驱动版本；它不再是“纯 Rust 协议栈”。

无论选择哪条，核心应只依赖一个由 `rtop` 定义的窄 `DataSource` trait（连接、执行、取消、流式 `RecordBatch`、诊断）。SQL 方言生成、认证、目录发现和物理传输保持在每个适配器之后；不要让任一第三方驱动的 AST、连接对象或错误类型渗入改写层。

## 局限与后续验证

本报告只判定接入路线，不判定 Ontop SQL 方言、类型映射或结果语义已经可迁移。ADBC 驱动目录会变化，且许多驱动由非 ASF 维护；在认领某数据源实现 issue 前，应固定目标服务版本，并用 Docker/服务端实例完成一次 Rust 客户端连接、参数查询、NULL/日期类型、取消和 Arrow/行结果的冒烟验证。
