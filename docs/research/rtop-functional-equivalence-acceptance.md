# rtop 功能等价的分层验收契约

**参考实现：** Ontop `version5`，提交 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。

## 可判定的定义

“功能等价”不是 Rust 重现 Java 类、SQL 字符串或构建产物；它指在一个冻结的、
可运行的验收情景中，`rtop` 对所有**迁移范围内**输入产生与 Ontop 相同的可观察
结果，或产生已登记的同类失败。一个情景由以下内容共同标识：

- 固定 Ontop commit、JDK（11 与 17）、Maven/Docker image digest；
- ontology、mapping、facts、SPARQL、配置、数据库初始化 SQL 和数据源服务镜像 digest；
- 指定的 SQL dialect、adapter、其版本和许可证/再分发记录；
- 输入请求/命令、环境变量、期望结果、允许规范化规则和预期错误代码。

不比较 JVM 堆栈、Java exception class、RDF4J/OWLAPI 对象身份、JDBC 调用顺序、JAR/Maven
布局或无稳定排序保证的 SQL 字节串。若 Ontop 的结果受随机 facts base IRI、时间、UUID
或数据库未排序输出影响，fixture 必须显式提供 base/时钟/排序，或只比较规定的结构性
不变量；不能以不稳定输出断言等价。

## 五层证据与通过条件

| 层 | 从 Ontop 取得的对照结果 | `rtop` 的通过证据 | 禁止的替代证据 |
| --- | --- | --- | --- |
| 1. 输入归一化 | `.obda`、Turtle R2RML、facts、OWL/imports 的成功模型或分类失败 | Rust 自有 Ontology/RDFFact/SQLPPMapping/prefix 模型，或同一错误代码与输入位置 | “所选 parser 能读取该标准格式”或 Java AST 序列化。 |
| 2. 查询语义 | SPARQL compliance manifest 和小型 VKG fixture 的 tuple/boolean/graph 结果 | 规范化后相同 RDF terms、binding、多重性和 ASK 值；无支持时同类失败 | 只比较是否返回 200 或只比较 SQL。 |
| 3. 改写 | Ontop 的 IQ/reformulation、映射约束与失败分类 | 每阶段可选结构快照；最终在同一数据源得到层 2 相同结果 | 跨 dialect 强行逐字比较 SQL。 |
| 4. 数据源与方言 | Docker 服务端上的 Ontop 结果、SQL/参数、类型和取消情景 | 同服务端、相同 fixture、固定 adapter 的结果、错误、取消、流式资源释放 | H2/JDBC fixture 作为 Rust adapter 已支持的证明。 |
| 5. 交付协议 | CLI/HTTP/Docker 可观察输入输出 | 退出码、stdout/stderr 错误代码、文件、HTTP status/header/content negotiation/body 与健康检查 | Spring/RDF4J API 或 JAR/镜像内部文件布局。 |

层 1–3 是核心上线门槛。层 4 按已选择的 adapter 各自成为门槛；没有合格非 JVM adapter
的数据源不在发布声明中。层 5 只覆盖 #6 决定保留的 CLI/HTTP/OCI 面。

## fixture 与结果的可复现格式

每个迁移的参考 fixture 在 `tests/compat/<id>/`（实现阶段创建）保存：`case.toml`、输入
文件、服务端初始化、原始 Ontop 输出、规范化对照结果、`rtop` 期望和 provenance。`case.toml`
至少有 `id`、`scope`（input/query/rewrite/adapter/delivery）、`ontop_commit`、`jdk`、
`database_image_digest`（若有）、`dialect`、`adapter`、`normalization`、`expected` 与
`source_paths`。

规范化规则固定如下：

1. tuple 结果按 SPARQL 结果格式解析，保留变量名、RDF term、行的多重性；只有查询含
   `ORDER BY` 时才比较行顺序；
2. graph/quad 用 canonical N-Quads（或明确的 blank-node 同构比较），保留语言标签、
   datatype 和 context；
3. SQL 只记录为诊断/同方言回归快照：规范化空白与生成 alias 后可比较，不作跨方言
   对照结果；参数的值和类型另记录；
4. 错误比较稳定 `ErrorCode`、HTTP status/CLI exit category、输入 span 和主要原因，不将
   Java 类名、路径前缀或 stack trace 固化；
5. 日志和 query ID 不作对照结果，除非协议明确返回并且有格式约束。

## 参考执行与采集

Ontop CI 的权威构建矩阵是 JDK 11/17 的 `./mvnw install --fail-at-end`；工作流另以
JDK 11 对 PostgreSQL、MySQL/MariaDB、Dremio、SQL Server、Oracle、DB2、Spark、Trino、
Presto、DuckDB、TDEngine 等启动 Docker 情景，私有云后端需要凭证。参考采集必须在容器
内执行，不能使用主机 JDK 8：

```sh
docker run --rm -v "$PWD/../ontop:/src:ro" -w /src \
  maven:3.9-eclipse-temurin-11 ./mvnw install --fail-at-end
docker run --rm -v "$PWD/../ontop:/src:ro" -w /src \
  maven:3.9-eclipse-temurin-17 ./mvnw install --fail-at-end
```

实际采集应先运行最小关联模块（`test/sparql-compliance`、`test/rdb2rdf-compliance`、
目标 docker/lightweight group），把 XML/JUnit 结果、请求与响应保存为对照结果；随后用
**独立服务端数据库**启动 `rtop`。绝不把 Ontop 进程、JDBC driver 或 H2 嵌入 `rtop`
测试运行时来取得“通过”。含私密数据库凭证、闭源 driver 或未获 Docker 再分发权的案例
留为 adapter 的受控验收，不能阻断核心层 1–3。

来源：`.github/workflows/main.yml` 的 JDK 11/17 矩阵与各 Docker group；
`test/sparql-compliance`、`test/rdb2rdf-compliance`、`test/lightweight-tests`、
`test/docker-tests` POM/测试资产。

## 失败、范围与发布规则

- 每个 Ontop 成功情景必须有一个 `rtop` 成功对照结果；每个明确不支持、解析、类型、
  本体不一致、数据源和协议失败也必须有分类对照结果。
- 遇到参考行为不确定、依赖 Java 宿主或需要未获许可的 driver 时，case 标为
  `excluded`/`deferred` 并引用路线图决定；它不计入通过率，也不能默默跳过。
- 一项新 dialect 或 adapter 的发布声明，至少要求该服务端容器上的连接/认证、metadata、
  参数、NULL、数值、日期/时区、取消、流式读取和代表性 SPARQL 语料均通过层 4。
- 发布 gate 输出 `compatibility-report.json`：全部 case ID、层级、环境 digest、状态、
  对照结果 hash、失败差异、scope/exclusion 理由。报告缺失或任何 in-scope case 未通过即为
  失败，不允许以总通过百分比覆盖。
- 变更 Ontop baseline、Docker image、Rust parser、dialect 或 adapter 版本，必须重采集
  受影响对照结果并经评审；不能覆盖旧证据。

## 进入 Rust 实现阶段的工作包

1. `input-compat`：实现并锁定 #4 的成功/失败 fixture（层 1）。
2. `sparql-core`：解析、IQ、OWL 2 QL 改写、优化和 SPARQL manifest（层 2–3）。
3. `postgres-adapter`、`mysql-adapter`：各自独立 Docker 层 4 gate；其他 adapter 另立票。
4. `http-cli-oci`：以 #6 的 protocol/CLI/镜像情景为层 5 gate。
5. 每个工作包必须把新 case 接入 `compatibility-report.json`；没有覆盖的新功能不能宣称
   Ontop 等价。
