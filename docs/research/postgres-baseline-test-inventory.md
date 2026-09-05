# Ontop PostgreSQL 直接命名测试清单

## 口径与取证方法

本清单的唯一参考是只读工作树 `../ontop` 的 Git 提交
`5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`（`version5`）。文件存在性以
`git ls-tree -r HEAD` 验证，内容以该工作树中相同提交的源码读取；没有使用其中的未跟踪
文件。本文件不把 JDBC/OWLAPI/RDF4J Java API 作为 Rust 接口契约，而是提取它们所断言的
PostgreSQL + 映射 + SPARQL 可观察结果。

Issue #14 所说的“18 个直接 PostgreSQL/PgSQL 命名 Java 测试文件”有精确定义：

- 根为 `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/`；
- 选择 `postgres/` 的**非 nested 直接子文件**和
  `datatypes/PgsqlDatatypeTest.java`；
- 排除参数化聚合器 `testsuite/DockerPostgresTestSuite.java`，它的名字含
  Postgres 但不是这 18 个直接测试类之一；排除 `postgres/nested/`，其中的类名并不含
  Postgres/PgSQL；也不把 `test/lightweight-tests` 的另一组 PostgreSQL 类混入此批。

因而“18”不能被误述为全仓库所有名字含 PostgreSQL/PgSQL 的 Java 文件数。后两组仍是
后续票据必须单独盘点的 PostgreSQL 语料。聚合器本身引用
`test/docker-tests/src/test/resources/testcases-docker/manifest-scenario-postgres.ttl`，继承
`test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/utils/OntopTestCase.java` 的
`runTest()`；它覆盖 manifest 中的 ASK 与 stockexchange 场景，故也不应因未列入
18 项而被当作范围外行为。固定基线实际 include 五个 manifest、122 个 `qt:query`
条目（ASK=1、datatype=66、filter=34、modifier=10、simple CQ=11）；其服务端迁移由 #34
单独追踪。

### 公共执行链与继承测试

除元数据类外，大部分直接类继承
`test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractVirtualModeTest.java`：其
`createReasoner(owl, obda, properties)` 建立虚拟模式连接，测试通过 OWLAPI statement 执行
SPARQL。这是 Java 执行机制，Rust 要继承的是相应的 RDF 结果、顺序和（少数用例所查的）
改写 SQL 性质。

| 父类 | 被哪些直接类继承 | 继承的测试/可观察功能 | 源码与 fixture 引用 |
| --- | --- | --- | --- |
| `AbstractVirtualModeTest` | 下表中标作“直接”的 12 个类 | 无 `@Test` 方法；提供连接、statement、结果计数/有序结果断言 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractVirtualModeTest.java` |
| `AbstractBindTestWithFunctions` | `BindWithFunctionsPostgreSQLTest` | 96 个 `test*`：逻辑、数值/哈希、字符串、日期时间、`BIND`、`COALESCE`、除零、`BNODE`、`IRI`、`IF`、`lang`/`datatype`/`regex`/`replace` 等 SPARQL expression 结果 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractBindTestWithFunctions.java`; `/pgsql/bind/sparqlBind.owl`, `sparqlBindPostgreSQL.obda`, `sparqlBindPostgreSQL.properties` |
| `AbstractDistinctInAggregateTest` | `DistinctInAggregatePostgresTest` | `testGroupConcatDistinct`、`testSumDistinct`、`testAvgDistinct`、`testCountDistinct`；断言 `21`、`10.5000`、`2` 及两种允许的拼接顺序 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractDistinctInAggregateTest.java`; `/distinctInAggregates/{university.ttl,university.obda,*Distinct.rq}` 与 `/pgsql/university.properties` |
| `AbstractLeftJoinProfTest` | `LeftJoinProfPgSQLTest` | 52 个 `test*`，覆盖 OPTIONAL/未绑定变量、MINUS、join 消除、聚合、NULL 聚合、GROUP_CONCAT、子查询、VALUES；其中还断言改写 SQL 不含多余 `LEFT` 或重复 `professors` 表 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractLeftJoinProfTest.java`; `/redundant_join/{redundant_join_fk_test.owl,redundant_join_fk_test.obda}` 与 `/pgsql/redundant_join_fk_test.properties` |
| `AbstractDbMetadataInfoTest` | `PgSqlMetadataInfoTest` | JUnit 3 `testPropertyInfo`：由 properties 读取 JDBC URL/用户名/密码，并读取 driver `DriverPropertyInfo`；这是 JDBC 元数据 API 语义，Rust 不迁移 API，但保留连接配置可用性的独立适配器检查 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractDbMetadataInfoTest.java`; `/pgsql/datatypes-pgsql.properties` |
| `OntopTestCase` | `PgsqlDatatypeTest`（以及排除的聚合器） | 参数化的 `runTest`，通过 manifest 为每一项加载 query、期望 Turtle、ontology、OBDA、properties，再比较 RDF4J 查询结果 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/utils/OntopTestCase.java` |

## 18 项直接测试

“声明方法”只列该 class 自己声明的 JUnit `@Test`；“继承”列上表中额外实际执行的测试。
资源路径均相对于 `test/docker-tests/src/test/resources`，前导 `/` 与 Java 的 classpath 字符串
一致。

| # | 基线 Java 测试（相对路径） | 父类；声明方法 | 直接 fixture | 可观察功能 |
| --- | --- | --- | --- | --- |
| 1 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/datatypes/PgsqlDatatypeTest.java` | `OntopTestCase`；参数化 `parameters()`，实际测试为继承的 `runTest()` | `/testcases-docker/manifest-datatype-pgsql.ttl`（忽略 `general-Type: all`）；该 super-manifest 指向 boolean、character、datetime、general、numeric 的 `manifest-pgsql.ttl`、各自 `datatypes-pgsql.obda/.properties`、`.rq` 与期望 `.ttl` | PostgreSQL RDF literal 类型：布尔、字符/CHAR、日期时间和数值映射及查询结果；是多案例 manifest，不是一条单独的 `@Test`。 |
| 2 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/AnnotationMovieTest.java` | `AbstractVirtualModeTest`；`testAnnotationInOntology`, `testAnnotationIRI`, `testAnnotationLiteral`, `testAnnotationString`, `testAnnotationDatabaseValue`, `testNewSyntaxUri`, `testClassUndefined`, `testDataPropertyUndefined`, `testObjectPropertyUndefined` | `/pgsql/annotation/{movieontology.owl,newSyntaxMovieontology.obda,newSyntaxMovieontology.properties}` | ontology annotation、IRI/literal/string/database 值 annotation，及未知 class/property 时的查询行为。 |
| 3 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/AnnotationTest.java` | `AbstractVirtualModeTest`；`testAnnotationInOntology` | `/pgsql/annotation/{doid.owl,doid.obda,doid.properties}` | DOID ontology annotation 的映射查询结果。 |
| 4 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/BindWithFunctionsPostgreSQLTest.java` | `AbstractBindTestWithFunctions`；`testHashSHA256`（`@Ignore`，要求启用 `pgcrypto`） | `/pgsql/bind/{sparqlBind.owl,sparqlBindPostgreSQL.obda,sparqlBindPostgreSQL.properties}` | 继承 expression 套件；该类以 PostgreSQL mapping/连接装入公共 BIND/function 断言，并明确记录 SHA-256 依赖的服务器扩展。 |
| 5 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/DistinctInAggregatePostgresTest.java` | `AbstractDistinctInAggregateTest`；无新增 `@Test` | `/distinctInAggregates/{university.ttl,university.obda,sumDistinct.rq,avgDistinct.rq,countDistinct.rq,groupConcatDistinct.rq}`；`/pgsql/university.properties` | `DISTINCT` 聚合与 PostgreSQL `GROUP_CONCAT` 的结果和允许顺序。 |
| 6 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/GroupConcatTest.java` | `AbstractVirtualModeTest`；`testPostgresGroupConcat1`, `testPostgresGroupConcat2` | `/pgsql/gconcat/{vkg.ttl,vkg.obda,vkg.properties,query1.rq,query2.rq}` | 文件化 SPARQL 查询的 GROUP_CONCAT 分组/排序结果。 |
| 7 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/ImdbPostgresTest.java` | `AbstractVirtualModeTest`；40 项：`testOneQuery`, `testCompanyLocationQuery`, `testIndividuals`, `testFindActress`, `testFindActor`, `testFindMovie`, `testFindTvSeries`, `testFindWriter`, `testFindProducer`, `testFindDirector`, `testFindEditor`, `testFindMovieGenre`, `testFindTvSeriesGenre`, `testFindMovieBudget`, `testFindTvSeriesBudget`, `testFindMovieGross`, `testFindTvSeriesGross`, `testFindMovieProductionYear`, `testFindTvSeriesProductionYear`, `testFindMovieActors`, `testFindMovieMaleActors`, `testFindMovieFealeActors`, `testFindMovieDirectors`, `testFindMovieProducers`, `testFindMovieEditors`, `testFindMovieProdicingCompany`, `testFindMovieFromAsianCompany`, `testFindTop25MovieTitlesBasedOnSpecificGenre`, `testFindBottom10MovieTitlesBasedOnRating`, `testFindAllMovieInformationForATitle`, `testFindProductionCompaniesInSpecificRegion`, `testFindMovieTitlesProducedByProductionCompaniesInEasternAsia`, `testFindActionMoviesProducedInEasternAsia`, `testFindNamesThatActAsBothDirectorAndActorAtTheSamTimeProducedInEasternAsia`, `testQ1`, `testQ2`, `testQ3`, `testQ4`, `testQ5`, `testBirthNameContainsZ` | `/pgsql/imdb/{movieontology.owl,movieontology.obda,movieontology.properties}` | IMDB 映射的 40 条 SELECT 结果计数：人物/电影/剧集分类、关系、数值属性、区域/排序/过滤及复杂 join；这是大数据量回归语料。 |
| 8 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/LeftJoinProfPgSQLTest.java` | `AbstractLeftJoinProfTest`；无新增 `@Test` | `/redundant_join/{redundant_join_fk_test.owl,redundant_join_fk_test.obda}`；`/pgsql/redundant_join_fk_test.properties` | 继承 OPTIONAL/LEFT JOIN 优化和聚合套件，兼具结果与 SQL 形状断言。 |
| 9 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/LowerMovieTest.java` | `AbstractVirtualModeTest`；`testLowerInSQL`, `testLower2InSQL` | `/pgsql/{movieontology.owl,lowerMovie.obda,lowerMovie.properties}` | mapping SQL 中 PostgreSQL `LOWER` 与 SPARQL 查询组合。 |
| 10 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/MetaMappingExpanderTest.java` | `AbstractVirtualModeTest`；`testQuery` | `/pgsql/{EPNet.owl,EPNet.obda,EPNet.properties}` | meta-mapping 展开后能产生预期查询答案。 |
| 11 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/OrderByTest.java` | `AbstractVirtualModeTest`；`testBolzanoOrderingAsc`, `testBolzanoOrderingAsc2`, `testBolzanoOrderingDesc`, `testBolzanoMultipleOrdering` | `/pgsql/order/{stockBolzanoAddress.owl,stockBolzanoAddress.obda,stockBolzanoAddress.properties}` | 单列/多列 `ORDER BY`、升降序及确定的 IRI 输出顺序。 |
| 12 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/PgSqlMetadataInfoTest.java` | `AbstractDbMetadataInfoTest`；继承 `testPropertyInfo` | `/pgsql/datatypes-pgsql.properties` | JDBC driver property information（迁移时只作非 JVM PostgreSQL adapter 配置/诊断要求的来源）。 |
| 13 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/PostgresIdentifierTest.java` | `AbstractVirtualModeTest`；`testLowercaseUnquoted`, `testUppercaseAlias`, `testUppercaseUnquotedView`, `testUppercaseQuotedView` | `/pgsql/identifiers/{identifiers.owl,identifiers-postgres.obda,identifiers-postgres.properties}` | PostgreSQL 未引用小写标识符、引用大写 alias、view 中未引用/引用大写 alias。 |
| 14 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/PostgresLowercaseIdentifierTest.java` | `AbstractVirtualModeTest`；`testLowercaseUnquoted` | `/pgsql/identifiers/{identifiers.owl,identifiers-lowercase-postgres.obda,identifiers-lowercase-postgres.properties}` | 对 PostgreSQL 小写未引用表/列名的折叠规则。 |
| 15 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/PrefixSourceTest.java` | `AbstractVirtualModeTest`；`testPrefixInsideURI` | `/pgsql/imdb/{movieontology.owl,newPrefixMovieOntology.obda,movieontology.properties}` | OBDA source/target prefix 展开、prefix 在 IRI 内的组合。 |
| 16 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/QuotedAliasTableTest.java` | `AbstractVirtualModeTest`；`test` | `/pgsql/{extended-npd-v2-ql_a_postgres.owl,npd-v2.obda,npd-v2.properties}` | 映射源 SQL 的引用 alias/table 与 NPD 查询。 |
| 17 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/RegexPostgresSQLTest.java` | `AbstractVirtualModeTest`；`testPostgresRegex`, `testPostgresRegexNot` | `/pgsql/regex/{stockBolzanoAddress.owl,stockexchangeRegex.obda,stockexchangeRegex.properties}` | PostgreSQL SQL regex 语法及 negated regex 在 mapping source 中的结果。 |
| 18 | `test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/postgres/UnboundVariableIMDbTest.java` | `AbstractVirtualModeTest`；`testIMDBSeries`, `testSubStr2WrongArgument`, `testSubStr3WrongArgument` | `/pgsql/imdb/{ontologyIMDB.owl,ontologyIMDBSimplify.obda,movieontology.properties}` | 未绑定变量、`SUBSTR` 参数错误时的 SPARQL 过滤/错误传播，及 IMDB series 查询。 |

## 注解发现的补充范围：20 项工作集的来源

文件名扫描与 PostgreSQL test annotation 是两套不同的发现机制，不能将它们的计数混为一谈。
上面的 18 项是 #14 所固定的 `docker-tests` 直接命名清单。轻量测试模块则以
`test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/PostgreSQLLightweightTest.java`
声明的 `@Tag("pgsqllighttests")` 元注解选择 PostgreSQL 用例；在基线内，
`rg -l '@PostgreSQLLightweightTest' test --glob '*.java'` 返回 10 个实际测试类。

其中八个 class 名本身也包含 PostgreSQL（Bind、Cast、Constraint、DistinctInAggregate、
LeftJoinProf、NestedDataArray、NestedDataJSON、NestedDataJSONB），但它们属于
`test/lightweight-tests`，不在上述“18 个 docker-tests 直接文件”口径内。另有下面两项的
文件名不含 PostgreSQL/PgSQL，只有 annotation 才能发现；它们是将 #14 的首批清单扩展为
**20 个优先工作项（18 + 2 annotation-only）**的原因：

| 注解-only 基线测试（相对路径） | 继承与声明方法 | fixture | 可观察功能 |
| --- | --- | --- | --- |
| `test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/postgresql/other/GeoSPARQLPostGISTest.java` | `AbstractDockerRDF4JTest`；`testAskIntersectsGeomGeog`, `testSelectIntersection1`, `testSelectIntersection2`, `testSelectIntersection3` | `/geospatial/{geospatial.owl,geospatial.obda,postgis/geospatial-postgis.properties}` | PostGIS `GEOMETRY`/`GEOGRAPHY` 混合的 `geof:sfIntersects`、`buffer`、`intersection`；断言结果 WKT 且改写 SQL 不含 `cast(st_astext`。 |
| `test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/postgresql/other/ReplaceTest.java` | `AbstractDockerRDF4JTest`；`testReplace` | `/prof/{prof.owl,prof.obda,postgresql/prof-postgresql.properties}` | SPARQL `REPLACE('ABC AA','A','Z')` 的有序唯一结果为 `"ZBC ZZ"^^xsd:string`。 |

因此，“18 文件名扫描”和“20 工作集”之间的差异是两个 annotation-only 测试，**不是**
声称整个 Ontop 仓库仅有 20 个 PostgreSQL 相关 Java 类。10 个注解类及其八个已命名轻量
类仍应在后续 lightweight 专项中逐项继承。

## 迁移验收的可执行解读

1. 每个表项应在 `compatibility-report.json` 以固定提交、source path、PostgreSQL Docker
   环境、运行命令和实际结果建档；“继承”不等于可忽略。
2. `PgSqlMetadataInfoTest` 的 `DriverManager`/`DriverPropertyInfo` 与所有 OWLAPI/RDF4J
   对象是 Java 专属实现；Rust 侧应以配置解析、连接和稳定诊断的黑盒测试替代，不能声称
   Java 类型迁移完成。
3. `ImdbPostgresTest`、完整 datatype manifest、BIND/LEFT JOIN 父类套件目前是最大的
   覆盖面：若 rtop 尚不支持对应 SPARQL/映射语义，报告应标为待实现或不支持并给出原因，
   不可用小型 PostgreSQL smoke test 代替。
4. 此清单的初始优先顺序是：datatype/标识符（驱动和 literal/SQL 方言）、ORDER/regex/
   GROUP_CONCAT、BIND 与 DISTINCT、LEFT JOIN 改写，最后以可部署的 IMDB fixture 收敛
   大数据量结果计数。该顺序不改变逐项继承的最终要求。

## 复核命令

在 `rtop` 根目录可执行以下只读命令重得 18 项；第二条确认引用资源属于固定基线：

```sh
cd ../ontop
git rev-parse HEAD
rg --files test/docker-tests/src/test/java/it/unibz/inf/ontop/docker -g '*.java' \\
  | rg '/(postgres/[^/]+|datatypes/PgsqlDatatypeTest)\\.java$' | sort | wc -l
git ls-tree -r --name-only HEAD test/docker-tests/src/test/resources/pgsql
```

预期提交为 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`，计数为 `18`。
