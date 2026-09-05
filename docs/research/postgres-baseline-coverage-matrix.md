# PostgreSQL 固定基线覆盖矩阵

本矩阵对应 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`，并以
`postgres-baseline-test-inventory.md` 的“18 个直接 docker-tests class + 2 个 annotation-only
class”为 #14 的 20 项优先工作集。状态的含义是：**已覆盖** 必须有 PostgreSQL 服务端
case；**Java API 范围外**只排除 Java 对象接口，不排除其可观察的配置/连接行为；**缺口**
必须有后继 ticket。

| 基线 class | 继承/直接观察重点 | rtop case 或依据 | 状态 | 后继 |
| --- | --- | --- | --- | --- |
| `PgsqlDatatypeTest` | manifest：boolean/character/datetime/general/numeric | `postgres-datatype-manifest-scalars`：直接以只读基线 `.obda` + 34 条 `.rq` 逐条执行；31 条 `rsi:size=1`，三个 literal 负例为 0 | 已覆盖 | — |
| `AnnotationMovieTest` | ontology annotation 的 RDF 查询结果 | `postgres-annotation-movie-exact-counts`：直接基线 OBDA 与八条原始 SPARQL 在 PostgreSQL Docker 中逐条以 CLI stdout 计数，444090/443300/112576/546032/100000/100000/131645/444090 全部一致；另有 ontology label=4 | 已覆盖 | — |
| `AnnotationTest` | DOID annotation 查询 | 基线类为 `@Ignore`（20 分钟）；固定 doid.obda 的原始 `rdfs:comment "NT MGI."` query 在 76 行 PostgreSQL fixture 返回 76 行 | 已覆盖 | — |
| `BindWithFunctionsPostgreSQLTest` | 继承 91 个启用 BIND/function 断言（另有 5 个 `@Ignore`） | `postgres-bind-functions-coverage.md` 将 91 个方法逐项映射到 postgres:17 Docker gate；涵盖字符串、数值、日期、SHA256、REGEX、随机标识符、term/type、COALESCE/IF、IRI/BNODE 和 books mapping 输入 | 已覆盖 | — |
| `DistinctInAggregatePostgresTest` | DISTINCT aggregate | `postgres-distinct-aggregates`：直接基线 `university.obda` + 四个 GROUP BY query，SUM/AVG/COUNT/GROUP_CONCAT(DISTINCT) 均在 PostgreSQL Docker 通过 | 已覆盖 | — |
| `GroupConcatTest` | GROUP_CONCAT | `postgres-group-concat-mapping`：直接基线 `vkg.obda`，语言 FILTER 与 `DISTINCT; separator="; "` 的两个查询均在 PostgreSQL Docker 通过 | 已覆盖 | — |
| `ImdbPostgresTest` | 40 条 IMDB SELECT 结果计数 | 基础、公司区域、东亚多 join、排序、full-information 及 Q1–Q5 fixture 合计直接执行全部 40 条原始 SPARQL；逐项 stdout 行数由 `scripts/test-postgres-compat.sh` 断言，并在 `postgres-imdb-*` cases 中取证 | 已覆盖 | — |
| `LeftJoinProfPgSQLTest` | 继承 OPTIONAL/MINUS/VALUES/subquery/aggregate | `postgres-left-join-min-max-null-aggregates`：直接基线 `redundant_join_fk_test.obda` + RDF/XML OWL；PostgreSQL Docker gate 覆盖 52 个父类方法，逐项映射见 `left-join-prof-final-audit.md` | 已覆盖 | — |
| `LowerMovieTest` | mapping SQL `LOWER` | `postgres-lower-movie-source-sql`：两条基线查询返回预期 typed literal | 已覆盖 | — |
| `MetaMappingExpanderTest` | 动态 class IRI template 展开 | `postgres-epnet-meta-mapping-template`：直接基线 `EPNet.obda` 的 `:AmphoraSection{rp_id}-{rp_id}` 在 PostgreSQL Docker 中生成并匹配 `:AmphoraSection4-4` | 已覆盖 | — |
| `OrderByTest` | 裸变量、ASC、DESC、多列排序 | `postgres-order-by-multiple-directions`；PostgreSQL Docker，三个排序条件 | 已覆盖 | — |
| `PgSqlMetadataInfoTest` | JDBC `DriverPropertyInfo` | Java/JDBC 类型 API 不迁移；`postgres-cli-query-validate-and-exit-categories` 和 adapter 服务端连接 case 验证 Rust 配置可加载、连接和诊断 | Java API 范围外（可观察连接已覆盖） | — |
| `PostgresIdentifierTest` | 大小写/quoted view 与 alias | `postgres-identifier-quoted-aliases`：四条原始语义查询通过 | 已覆盖 | — |
| `PostgresLowercaseIdentifierTest` | 小写未引用标识符 | `postgres-lowercase-unquoted-identifiers`：基线 mapping 对真实小写 table/column 成功 | 已覆盖 | — |
| `PrefixSourceTest` | source/target prefix 展开 | `postgres-prefix-inside-iri-source`：直接基线 mapping 的 mo/mo2 双 prefix 查询通过 | 已覆盖 | — |
| `QuotedAliasTableTest` | 引用 table/alias 的映射 SQL | `postgres-npd-quoted-table-aliases`：直接基线 mapping 对带引号 alias 的 source 查询成功 | 已覆盖 | — |
| `RegexPostgresSQLTest` | PostgreSQL SQL regex 与取反 | `postgres-source-sql-regex-and-negation`：直接基线 mapping 的 `~*`/`!~*` 均在 PostgreSQL Docker 通过 | 已覆盖 | — |
| `UnboundVariableIMDbTest` | unbound、SUBSTR 错误传播、IMDB series | 上述 VALUES/subquery/UNION/BIND 错误路径由 runtime case 覆盖；`postgres-imdb-series-limit` 直接读取 `ontologyIMDBSimplify.obda`，在 PostgreSQL Docker 中返回 Series 的 DISTINCT LIMIT 10 | 已覆盖 | — |
| `GeoSPARQLPostGISTest` | annotation-only；PostGIS geometry/geography | `postgres-postgis-geosparql-geometry-geography`：独立 PostGIS Docker 服务执行 sfIntersects、buffer 与 intersection 的四条原始查询，结果计数为 36/0/1/0 | 已覆盖 | — |
| `ReplaceTest` | annotation-only；SPARQL REPLACE | `sparql-bind-replace`：原始纯 BIND 查询返回 `"ZBC ZZ"^^xsd:string` | 已覆盖 | — |

## 注解发现的其余 lightweight class

下列 eight classes 并不改变上述 20 项的计数口径，但它们是 `@PostgreSQLLightweightTest`
发现的 PostgreSQL 语料，已被显式纳入后继工作，避免成为未解释缺口。

| class | 后继 |
| --- | --- |
| `BindWithFunctionsPostgreSQLTest` | #17 |
| `CastPostgreSQLTest` | #21 |
| `ConstraintPostgreSQLTest` | #21 |
| `DistinctInAggregatePostgreSQLTest` | #18 |
| `LeftJoinProfPostgreSQLTest` | #18 |
| `NestedDataArrayPostgreSQLTest` | #20 |
| `NestedDataJSONPostgreSQLTest` | #20 |
| `NestedDataJSONBPostgreSQLTest` | #20 |

## 已覆盖项的取证

`postgres-order-by-multiple-directions` 使用 `postgres:17` Docker 服务端、
`tests/compat/postgres-order-by/` 中的最小 address fixture，以及
`scripts/test-postgres-compat.sh`。它把基线 `OrderByTest` 的四种写法收敛为一个查询：
`ORDER BY DESC(?country) ?number DESC(?street)`；运行报告的来源路径、环境与实际结果在
`compatibility-report.json`。该缩小 fixture 保留排序的可观察结果，不迁移 OWLAPI 的
statement 或测试辅助 API。

因此，#14 当前没有“未解释”的优先工作集缺口：一项已用服务端 case 覆盖、一项明确仅为
Java API 范围外，其余均已拆入可执行后继 issue。关闭 #14 前仍需逐张关闭后继票，或将其
结果回填为新的服务端 compatibility case。
