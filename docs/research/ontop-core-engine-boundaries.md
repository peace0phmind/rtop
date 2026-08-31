# Ontop 核心引擎迁移边界

**参考版本。** Ontop `version5` 的
`5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。本文件回答路线图 #3；它描述
Rust 模块间的契约，不把 Java 类型、依赖注入或 JDBC 对象带入 `rtop`。

## 结论：一个内核、三个端口

`rtop` 的内核是确定性的查询语义：接收解析后的 SPARQL、本体、映射和关系模式，
产出可执行的 SQL 计划及 RDF 结果构造规则。内核不得依赖网络、文件系统、ODBC、
HTTP、RDF/OWL 解析器对象或任何 Java 类型。

围绕内核保留三个方向明确的端口：

1. **输入端口**：解析 adapter 将 SPARQL、OBDA、R2RML、RDF facts 和 OWL 文档转换
   为 `rtop` 自有的 RDF 术语、本体和映射模型；配置 adapter 组装一次可查询的
   `KnowledgeGraphSpec`。
2. **数据源端口**：方言/元数据选择产生 SQL，`DataSource` 执行带参数的 SQL 并流式
   返回带类型的行。PostgreSQL 等原生协议和许可合规的 ODBC 都是此端口的实现；ODBC
   句柄、SQLSTATE、驱动元数据和连接池对象不能跨入改写器。
3. **交付端口**：CLI 与 SPARQL HTTP adapter 分别把请求变成输入端口的对象，并把
   tuple、boolean、graph 或错误序列化。它们不调用 RDF4J `Repository` 或 Java
   `OntopConnection`。

```
files / HTTP / CLI ──> input adapters ──> KnowledgeGraphSpec
                                              │
                                      reformulation kernel
                                              │
                         SQL dialect <── executable plan ──> result construction
                                              │
                                      DataSource adapter
                                              │
                                      database service
```

## 从参考模块到 Rust 边界

| Ontop 模块 | 参考中的职责与耦合 | `rtop` 的位置 |
| --- | --- | --- |
| `core/model`, `core/obda`, `core/optimization` | RDF 术语、IQ/函数符号、规范对象和优化 | 纯核心模型与优化；Rust 自有类型 |
| `core/kg-query` | 查询种类及 SPARQL 解析工厂；POM 直接依赖 RDF4J model/query/parser | 解析 adapter + Rust 查询 AST；解析结果才进入内核 |
| `mapping/core`, `mapping/sql/core`, `mapping/sql/native` | 映射模型、规范化、映射优化、SQL 映射 | 映射核心；文件读取放输入 adapter |
| `db/rdb` | 数据库模式、约束、SQL 代数和方言 | 核心的 schema/dialect 子模块；不含连接实现 |
| `engine/reformulation/core`, `engine/reformulation/sql` | 将 KG 查询改写为 IQ/SQL | 改写核心；输入为 Rust 查询 AST，输出为 `ExecutablePlan` |
| `engine/system/core` | 引擎、语句、结果集及执行编排 | 查询服务层；以 `DataSource` port 执行计划 |
| `engine/system/sql/core` | SQL 执行、JDBC pool、Tomcat JDBC/HikariCP | `DataSource` adapter 层；Rust 重做，JDBC pool 排除 |
| `ontology/owlapi`, `mapping/owlapi` | OWLAPI 文档/本体转换和映射装配 | OWL 输入 adapter；只输出内核本体模型 |
| `mapping/sql/r2rml` | R2RML RDF 读写与校验 | R2RML 输入/输出 adapter；只输出/消费映射模型 |
| `binding/rdf4j`, `binding/owlapi` | Java 进程内 Repository/OWLAPI 连接和结果对象 | 明确排除；不可作为 Rust facade |
| `client/*`, `protege/*` | CLI、HTTP、Spring/Tomcat、Protégé/Swing/OSGi | CLI/HTTP 是交付 adapter；后者全部排除 |

## 参考接口给出的可观察 seam

参考代码已暴露出适合迁移的责任分割，但其 Java 类型不是目标接口：

| 参考接口 | 当前输入/输出 | Rust 契约 |
| --- | --- | --- |
| `KGQueryFactory` | SPARQL 字符串 → Select/Ask/Construct/Describe 查询；实现依赖 RDF4J | `SparqlParser: &str → QueryAst`，语法错误与不支持特性分离报告 |
| `QueryReformulator` | KGQuery + `QueryContext` → IQ，另可渲染改写 | `Reformulator: QueryAst + KnowledgeGraphSpec + QueryContext → ExecutablePlan`；调试渲染为可选观测输出 |
| `DBConnector` | 生命周期 + 连接池连接 | `DataSource`：创建会话/执行请求/取消；资源生命周期由 Rust 所有权表达 |
| `OntopStatement` | 可执行 IQ → tuple/graph/boolean 结果；还暴露 HTTP headers、UUID、Guava multimap | `QueryService`：计划 + 参数 → `QueryResultStream`；HTTP headers/请求 ID 仅在 HTTP adapter 映射为 `QueryContext` |
| `OntopQueryEngine` | connect/getConnection/getReformulator | `Runtime` 组合根；不把连接或改写器作为外部 Java API 暴露 |

`QueryContext` 是唯一可从交付层进入改写/执行的请求级输入。应只包含有明确语义的
选项（例如超时、取消令牌、已认证主体或适用的查询选项）；不得把 HTTP header map、
ODBC/JDBC connection 或框架请求对象原样传入。

## 必须守住的依赖方向

```
domain model  <- mapping / ontology normalization <- reformulation / optimization
     ^                                                     |
     |                                                     v
schema + dialect <-------------------------------- executable SQL plan
                                                           |
                                                         DataSource
                                                           ^
                         protocol/file/CLI adapters --------|-------- ODBC/native adapters
```

- 内核可依赖数据模型、映射、本体、schema 和 dialect；不可依赖 parser、HTTP 或
  传输驱动。
- `DataSource` 接收的是已经由 dialect 生成的参数化 SQL；它不决定 SPARQL 语义、
  映射饱和或 SQL 方言。
- RDF/OWL/R2RML 解析器可以替换，前提是它们经验证后产生相同的内部模型或同等的
  明确错误。解析库的 AST 不得成为持久核心类型。
- CLI 与 HTTP 必须对同一个 `QueryService` 调用，避免两套查询语义。

## 已发现的 Java 耦合点及处理

| 耦合点 | 证据 | 处理 |
| --- | --- | --- |
| RDF4J 查询 AST/解析器 | `core/kg-query/pom.xml` 依赖 `rdf4j-query*` 和 `rdf4j-queryparser-sparql` | Rust parser adapter；#5 定义可观察 SPARQL 边界 |
| RDF4J/Commons RDF 结果模型 | `engine/system/core/pom.xml` 依赖 RDF4J 与 `commons-rdf-rdf4j` | Rust RDF term/result stream；HTTP 根据标准格式序列化 |
| OWLAPI 与 Protégé XML catalog | `ontology/owlapi`、`mapping/owlapi` POM | 输入 adapter；#4 核验 OWL 2 QL、imports 与错误行为 |
| Guice/assisted inject 与 Java 生命周期 | reformulation/system POM | Rust 显式组合根与 trait 注入；不迁移 DI framework |
| JDBC/Tomcat JDBC/HikariCP | `engine/system/sql/core/pom.xml` | `DataSource` 实现；原生协议/ODBC 单独选择并验证 |
| RDF4J Repository / OWLAPI binding | `binding/rdf4j`、`binding/owlapi` | 不迁移；#6 仅审计可替代的 HTTP/CLI 外部行为 |

## 首个纵切的验收场景

用一个 Docker 中的 PostgreSQL（Rust 原生协议）场景验证边界：读取最小 OWL 2 QL
本体和 `.obda` 映射，解析一个 `SELECT`，改写为参数化 PostgreSQL SQL，经
`DataSource` 取得行并序列化 SPARQL JSON。替换为 ODBC adapter 时，只允许
`DataSource` 集成测试变化；同一 `ExecutablePlan`、结果 RDF 术语和 HTTP 载荷必须
不变。这一场景能发现“驱动对象泄漏入改写器”或“不同入口有不同查询语义”的问题。

## 对后续票据的约束

- #4 的解析与校验工作只能产生上述输入端口的内部模型。
- #5 的 SPARQL/SQL 审计须把 dialect 语义与 `DataSource` 传输语义分开记录。
- #6 只能保留 CLI/HTTP 的跨语言可观察契约，不能把 Java binding 当作兼容目标。
- #7 的验收必须涵盖该纵切，且 Rust 端用容器化服务端数据库而不是嵌入式 H2。
