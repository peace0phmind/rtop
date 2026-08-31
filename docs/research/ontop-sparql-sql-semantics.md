# Ontop SPARQL→SQL 与方言边界审计

**参考版本：** Ontop `version5` 提交 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。
本文只记录该提交中由生产入口和测试证明的能力；SQL serializer 的存在不等于对应
数据源已经具备无 JVM 的可用 adapter。

## 决策

`rtop` 的核心必须实现独立的 SPARQL parser/AST、查询改写、优化的中间查询（IQ）和
SQL 代数；其输出是带参数的 `NativeQuery`，交给 `DataSource` adapter 流式执行并转换为
Rust 自有的 tuple、boolean、graph/quad 结果。SQL 方言只实现为该代数的纯 serializer、
类型与标识符策略；目录元数据、认证、传输、连接池和 JDBC `ResultSet` 绝不进入核心。

## 从 SPARQL 到结果的实际管线

| 阶段 | 参考入口与已观察行为 | `rtop` 保留的语义 / 边界 |
| --- | --- | --- |
| 解析和查询种类 | `KGQueryFactoryImpl` 用 RDF4J `QueryParserUtil.parseQuery(QueryLanguage.SPARQL, …)` 创建 select、ask、construct、describe、insert/delete 查询对象 | 接受这五个查询种类的文本入口；Rust parser 及 AST 替代 RDF4J `ParsedQuery`/`TupleExpr`。更新语义须以测试逐项确定，不能由有对象类型推定完整 SPARQL Update 支持。 |
| 翻译与改写 | `QuestQueryProcessor.reformulateIntoNativeQuery`：KG query→IQ、tree-witness 本体改写、mapping expansion、优化、native SQL IQ；失败有 invalid/unsupported/typing/not-fully-translatable 分类 | OWL 2 QL + 映射驱动的改写和这些可诊断错误类别属于核心语义；Guice cache 和 logger 是可替换运行时设施。 |
| SQL 生成 | `SQLGeneratorImpl`、`PostProcessingProjectionSplitter` 将 native IQ 变为 SQL，并拆分须在数据库结果之上计算的投影 | 保留“数据库 SQL + 后处理投影”的语义，不要求方言支持所有 SPARQL 函数。 |
| 执行和结果 | `QuestStatement` 分别走 tuple、boolean、construct、describe；SQL 路径的 `SQLQuestStatement`/`JDBCTupleResultSet` 通过 JDBC 语句、游标和 `ResultSet` 流式读取 | 保留结果种类、惰性/流式消费、关闭与取消的资源语义；以 `DataSource`、`NativeQuery`、`RowStream` 替代 JDBC、Hikari/Tomcat 连接池和 Java result objects。 |

源码：`core/kg-query/.../KGQueryFactoryImpl.java`、
`core/kg-query/.../RDF4JTupleExprTranslator.java`、
`engine/reformulation/core/.../QuestQueryProcessor.java`、
`engine/reformulation/sql/.../SQLGeneratorImpl.java`、
`engine/system/core/.../QuestStatement.java` 与 `engine/system/sql/.../SQLQuestStatement.java`。

## SPARQL 兼容范围的证据

`test/sparql-compliance` 包含 RDF4J SPARQL 与 SPARQL 1.1 manifest executor；因此它是
`rtop` 的查询语义基线，而不是对 RDF4J Java API 的依赖。Docker/lightweight 测试另为
特定后端、函数、地理空间、constraints、federation 等提供回归语料。首期不可将测试树
中存在的每个扩展宣称为默认能力：每项应被登记为 core、已选 dialect，或不支持的失败
fixture。

特别地，`RDF4JTupleExprTranslator` 对不符合其预期的 parser AST 抛内部转换异常，例如
嵌套 complex projection、意外 join 和非空 projection alias。这说明对照不能只比较
“能解析”：必须比较支持查询的结果，并把不支持查询的错误类别也固定为 fixture。

## SQL 映射 source 解析的限制

映射的 SQL source 会经 `JSqlParser` 解析为关系代数（`SelectQueryParser`、
`BasicSelectQueryParser`、`ExpressionParser`），不是任意数据库 SQL 的透传。已明确拒绝
的例子包括 WITH、DISTINCT、TOP、GROUP BY/HAVING、ORDER BY、LIMIT/OFFSET/FETCH、
FOR UPDATE、outer join、PIVOT/UNPIVOT、lateral subselect、values list、table function、
subselect/EXISTS/ANY/ALL、analytic 和 JSON expression，以及许多未建模的 SQL 函数。

`rtop` 必须把“映射 SQL source 可解析的子集”与“生成给数据源的 SQL”分开建模，并为
`InvalidMappingSql`、`UnsupportedMappingSql`、`InvalidSparql`、`UnsupportedSparql`、
`TypeError`、`NotFullyTranslatable` 和 `DataSourceError` 提供不同错误代码。不得把源 SQL
parser 的这些局限错误伪装成数据源执行错误。

源码和测试：`db/rdb/.../BasicSelectQueryParser.java`、`SelectQueryParser.java`、
`ExpressionParser.java`，及 `db/rdb/src/test/.../SQLParserTest.java`、
`ExpressionParserTest.java`、`SelectQueryAttributeExtractorTest.java`。

## 已实现的方言层与迁移分类

下表按 `db/rdb` 中同时存在 serializer、函数符号或类型/标识符策略的生产代码归类。
这只证明 SQL 生成实现存在；没有非 JVM 传输 adapter 或目标服务端验收的行仍不能纳入
`rtop` 发布承诺。

| 分类 | 参考方言 | `rtop` 决定 |
| --- | --- | --- |
| 首批可选方向 | PostgreSQL、MySQL/MariaDB | 作为核心 SQL 代数的优先实现候选；各自必须用 Rust wire adapter 与 Docker 服务端验证。 |
| 有非 JVM 路线但逐源选择 | Athena、BigQuery、Dremio、DuckDB、Oracle、DB2、Trino、Presto、Spark SQL、TDEngine、Teiid、Denodo | 方言代码可留作语义参考；只有相应 protocol/ODBC adapter 已获许可、固定版本并通过端到端验收时才实现。ODBC 仅为 adapter，不能决定核心 API。 |
| JDBC/测试专用，不迁移为运行时后端 | H2、HSQLDB | serializer 的存在和大量 H2 fixture 都不构成 `rtop` 数据源支持；它们依赖 Java 嵌入式运行时。 |
| 其余已有 serializer | ADP、CData DynamoDB、MonetDB、Redshift、SQL Server、SAP HANA、Snowflake | 不作为首期承诺；按目标产品、adapter 可行性和独立方言验收重新立票。 |

实现证据：`generation/serializer/impl` 中的各 `*SelectFromWhereSerializer`，
`model/term/functionsymbol/db/impl` 的各 `*DBFunctionSymbolFactory`，以及
`model/type/impl`、`dbschema/impl` 的厂商策略。传输与 metadata 的 JDBC 实现（例如
`JDBCMetadataProviderFactory`、`JDBCConnectionPool`）是 Java 宿主专属，排除。

## 实施顺序与验收切片

1. 用 SPARQL compliance manifest 固定 parser、select/ask/construct/describe 的结果与
   错误 fixture；Update 另行逐操作取证后才开放。
2. 对本体改写、mapping expansion、优化和 SQL serialization 分层快照 IQ/参数化 SQL；
   SQL 文本只在固定 dialect 版本下比较，不能跨方言比较字符串。
3. 以 PostgreSQL、MySQL/MariaDB 的 Docker 服务端运行端到端语料，再对每个实际选择的
   adapter 复制：NULL、datatype/timezone、参数、取消、流式读取、identifier quoting 和
   函数回退。
4. 将 H2、JDBC pool、JDBC metadata 和 Java `ResultSet` 测试标为参考/实验资产，不能
   作为 Rust 发布的通过证据。
