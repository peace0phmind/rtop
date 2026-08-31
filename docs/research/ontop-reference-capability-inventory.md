# Ontop 参考能力清单

**参考版本。** `/home/mind/top/ontop` 的 `version5` 提交
`5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。本清单只以该提交中受 Git
跟踪的文件为依据；工作树内的未跟踪测试输出不属于可复现基线。

**迁移口径。** `rtop` 不携带 JVM、JDBC 或 Java 进程内 API。跨语言文件格式、
SPARQL 协议和虚拟知识图谱语义属于迁移目标；数据库访问可使用受许可的 ODBC
adapter。缺少非 JVM 路线的数据源 adapter 暂缓开发。

## 核心虚拟知识图谱能力

| 参考模块/源码区域 | 可观察能力 | `rtop` 分类 | 后续票据 |
| --- | --- | --- | --- |
| `core/model`、`core/obda` | RDF 术语、类型、函数符号、OBDA 规范与内部查询模型 | Rust 语义迁移 | 识别 Ontop 核心引擎的迁移边界 |
| `core/optimization` | 中间查询优化、约束与变量处理 | Rust 语义迁移 | 识别 Ontop 核心引擎的迁移边界 |
| `core/kg-query` | SPARQL 查询表示、翻译入口与结果集语义；当前实现直接依赖 RDF4J AST | Rust 原生替代；不能沿用 RDF4J 类型 | 识别 Ontop 核心引擎的迁移边界 |
| `engine/reformulation/*` | 本体/映射驱动的 SPARQL 到中间查询及 SQL 改写 | Rust 语义迁移 | 审计 SPARQL 到 SQL 的可观察能力与方言边界 |
| `engine/system/*` | 连接、语句执行、结果流、物化 | Rust 语义迁移；数据源接口重建 | 审计 SPARQL 到 SQL 的可观察能力与方言边界 |
| `db/rdb` | 目录与约束元数据、SQL 代数/生成、各数据库函数与标识符方言 | Rust 语义迁移；传输层按 adapter 分类 | 审计 SPARQL 到 SQL 的可观察能力与方言边界 |

## 输入、映射与本体

| 参考区域 | 输入与行为 | `rtop` 分类 |
| --- | --- | --- |
| `mapping/core`、`mapping/sql/native` | Ontop 原生 `.obda` 映射、目标三元组模板、SQL 映射规则 | 迁移文件语义；解析实现用 Rust 重写 |
| `mapping/sql/r2rml` | R2RML RDF/Turtle 读取、校验、转换与序列化 | 迁移 R2RML 语义；替换 RDF4J/R2RML Java 实现 |
| `mapping/owlapi`、`ontology/owlapi` | OWL 文档加载、imports、OWL 2 QL 转换与分类 | 迁移 OWL 文件与 OWL 2 QL 语义；替换 OWLAPI |
| `mapping/owlapi` 的 facts 加载路径 | 可选 RDF facts 文件及格式推断 | 迁移实际支持的 RDF facts 语义；替换 RDF4J Rio |
| SQL 映射配置 builders | 映射、本体、facts、数据库元数据、lenses 与规则的组合配置 | 新建 `rtop` 配置；不接受 JDBC URL、驱动类或 Java DataSource |

参考测试资产中可见 `.obda`、`.owl`、`.ttl`、`.r2rml` 与 `.properties` 文件；
其中 `.properties` 的 JDBC 字段不能作为 `rtop` 运行时契约，只能成为后续转换工具的输入。

## CLI 与 HTTP 外部契约

| 参考入口 | 可观察能力 | `rtop` 分类 |
| --- | --- | --- |
| `client/cli` | `bootstrap`、`compile`、`endpoint`、`extract-db-metadata`、`materialize`、`pretty-r2rml`、`query`、`to-obda`、`to-r2rml`、`v1-to-v3`、`validate`、版本与 shell completion | 命令语义与输入/输出待逐项审计；Airline Java 框架不迁移 |
| `client/endpoint` | `/sparql` 查询、`/ontop/reformulate`、`/ontology`、预定义查询及 portal/restart 配套端点 | 保留跨语言 HTTP/SPARQL 契约；Spring Boot、Tomcat、Servlet 与 portal Java 实现不迁移 |
| `build/distribution`、`client/docker` | CLI/endpoint 的发布与容器材料 | 交付效果待审计；Maven/JAR/Assembly 机制不迁移 |

## Java 专属、明确排除的能力

| 参考区域 | 排除原因 |
| --- | --- |
| `binding/rdf4j` | RDF4J `Repository`、查询和结果对象是 Java 进程内 API；SPARQL/RDF 文件语义另行迁移 |
| `binding/owlapi` | OWLAPI engine、连接和结果对象是 Java 进程内 API；OWL 2 QL 语义另行迁移 |
| `protege/*` | Protégé、Swing、OSGi 与插件发行是 Java 桌面宿主能力 |
| JDBC/JNDI/DataSource、HikariCP、Tomcat JDBC | JVM 数据访问契约；由 `rtop` 数据源 interface 及 ODBC/原生 adapter 替代 |
| H2 内存数据库测试与 H2 嵌入式运行 | JVM 嵌入式能力；不作为 `rtop` 数据源或端到端验证基线 |

## 数据源与方言证据

`db/rdb` 含 PostgreSQL、MySQL、Oracle、DB2、SQL Server、Trino、Presto、Athena、
BigQuery、Dremio、Denodo、DuckDB、Spark SQL、TDEngine、Teiid 等的元数据或 SQL
方言实现。其含义是 **SQL 生成语义存在**，不意味着 `rtop` 已获得非 JVM 传输层。

- PostgreSQL、MySQL/MariaDB 等有服务端协议路线，后续以 Rust adapter 验证。
- 可获准的 ODBC 路线可以实现为 `rtop` 数据源 adapter，许可与再分发规则见
  [ODBC 供应链核对](./odbc-licensing-and-redistribution.md)。
- H2、Teiid 与没有可确认非 JVM 路线的来源不阻塞核心迁移，暂缓 adapter 开发。
- 当前 JDBC 驱动和 ODBC 驱动的许可、获取与再分发证据分别见
  [JDBC 核对](./jdbc-driver-licenses.md) 与
  [ODBC 可用性](./odbc-driver-availability.md)。

## 验证资产与执行基线

| 资产 | 作用 | `rtop` 使用方式 |
| --- | --- | --- |
| `test/sparql-compliance` | SPARQL 查询语义与标准用例 | 转为 Rust 端到端/集成验收契约 |
| `test/rdb2rdf-compliance` | R2RML 规范用例 | 转为映射解析与物化验收契约 |
| `test/lightweight-tests` | 数据库/方言分组测试；含多服务端 Docker 镜像 | 只挑选有非 JVM adapter 的服务端数据库 |
| `test/docker-tests` | 多数据库容器、映射、推理和回归场景 | 作为逐方言验收语料，不把 Java H2 fixture 当 Rust 基线 |
| `test/semantic-index` | 语义索引/物化方向 | 非默认虚拟模式；需在核心路线明确后再评估 |

参考测试树目前含 323 个 Java 测试类和 6,274 个资源文件；全仓库 `src/test` 目录还
包括各生产模块的测试。参考 CI 使用 Maven 并在 JDK 11/17 上执行，且为多种数据源
启动 Docker 服务。复现 Ontop 参考行为时，应从固定提交导出的快照在 Docker JDK 11/17
容器中执行；不得依赖主机 JDK 8 或未跟踪文件。

## 结论

`rtop` 的可迁移核心是“本体 + 映射 + 关系数据源 → SPARQL 改写与结果”的语义链路，
以及其跨语言文件、CLI 和 HTTP 形式。Java binding、JDBC API、Protégé 与 Maven
发行不是迁移目标。下一张票据应据此确定核心模块的 seam，尤其要把数据源 adapter 与
查询改写核心隔离开。
