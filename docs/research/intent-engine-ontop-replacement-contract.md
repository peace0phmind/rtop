# `intent-engine-ontop` 的 Ontop 使用面与 rtop 替换契约

日期：2026-09-05。目的：只读分析相邻工作树 `../intent-engine-ontop` 当前实际向 Ontop 索取的能力，作为以 `rtop` 替换其 Ontop 模块的验收边界。Ontop 源码对照固定为 `../ontop` 的 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`；两个对照工作树均未修改。

## 结论

该项目没有链接 Ontop Java 库、没有调用 `ontop query`/`materialize` 等 CLI，也没有使用 RDF4J、OWLAPI 或 Ontop 的管理 API。它将 Ontop 作为 **PostgreSQL 上的虚拟知识图 SPARQL HTTP 服务**：将 OWL/Turtle 本体、native OBDA 映射和 JDBC properties 挂入容器，向 `/sparql` 以表单 `query=` POST 发送只读查询，读取 SPARQL Results JSON，并逐项与独立 SQL oracle 比较。

因此第一阶段的替换目标不是复刻 Ontop 全产品面，而是“七个可独立配置的 PostgreSQL VKG endpoint + 稳定 SPARQL HTTP 契约”。其中 FIBO 的 `LoanContract rdfs:subClassOf fibo-loan:Loan` 查询改写是明确的必要本体能力；`owl:sameAs` 的反例则是必要的语义风险项，不能未经验证地声称等价。

## 运行路径与外部契约

```text
Python CLI/runner --POST form(query=<SPARQL>)--> http://host:port/sparql
                                                  |
                                      Ontop: OWL + OBDA + JDBC properties
                                                  |
                                             PostgreSQL (npd/fibo/fibo_scale)
                                                  |
SPARQL Results JSON <-- results.bindings ---------+
       |
独立 psycopg SQL oracle、JSON trace 与失败分类
```

* Compose 固定 `ontop/ontop:5.1.0`，并为 NPD、主 FIBO、unsafe identity、去 subclass、错误 equivalentClass、去 weak alignment、scale 分别启动七个服务；每个服务使用同一 PostgreSQL 服务但可替换本体、映射或数据库。[`../intent-engine-ontop/compose.yaml:25`](../../intent-engine-ontop/compose.yaml#L25)、[`compose.yaml:45`](../../intent-engine-ontop/compose.yaml#L45)、[`compose.yaml:65`](../../intent-engine-ontop/compose.yaml#L65)、[`compose.yaml:85`](../../intent-engine-ontop/compose.yaml#L85)、[`compose.yaml:102`](../../intent-engine-ontop/compose.yaml#L102)、[`compose.yaml:119`](../../intent-engine-ontop/compose.yaml#L119)、[`compose.yaml:136`](../../intent-engine-ontop/compose.yaml#L136)。
* 输入由环境变量传入镜像：`ONTOP_ONTOLOGY_FILE`、`ONTOP_MAPPING_FILE`、`ONTOP_PROPERTIES_FILE`；FIBO 使用 `application-ontology.ttl`、`mapping.obda` 和 PostgreSQL JDBC 驱动挂载。[`compose.yaml:51`](../../intent-engine-ontop/compose.yaml#L51)-[`compose.yaml:63`](../../intent-engine-ontop/compose.yaml#L63)。properties 仅含 PostgreSQL JDBC URL、用户、口令和驱动类。[`infra/ontop/fibo.properties:1`](../../intent-engine-ontop/infra/ontop/fibo.properties#L1)-[`fibo.properties:4`](../../intent-engine-ontop/infra/ontop/fibo.properties#L4)。这意味着 rtop 需把它们转换为自己的 TOML PostgreSQL 配置；rtop 明确拒绝 JDBC URL/驱动类。[`src/config.rs:73`](../../src/config.rs#L73)-[`config.rs:92`](../../src/config.rs#L92)。
* 客户端均以 `application/x-www-form-urlencoded` 请求体传递 `query`，并要求 `Accept: application/sparql-results+json`；返回体被当作 `document["results"]["bindings"]`。NPD 基线的直接证据见 [`runner.py:123`](../../intent-engine-ontop/intent_engine/runner.py#L123)-[`runner.py:138`](../../intent-engine-ontop/intent_engine/runner.py#L138)，通用 NPD suite 见 [`npd_suite.py:114`](../../intent-engine-ontop/intent_engine/npd_suite.py#L114)-[`npd_suite.py:132`](../../intent-engine-ontop/intent_engine/npd_suite.py#L132)，FIBO 见 [`financial.py:191`](../../intent-engine-ontop/intent_engine/financial.py#L191)-[`financial.py:212`](../../intent-engine-ontop/intent_engine/financial.py#L212)。rtop 已接受该 POST 格式且对 bindings 返回同一 MIME 类型。[`src/server.rs:55`](../../src/server.rs#L55)-[`server.rs:75`](../../src/server.rs#L75)、[`server.rs:150`](../../src/server.rs#L150)-[`server.rs:166`](../../src/server.rs#L166)。
* endpoint 只需要 query 执行；项目架构明确排除 SPARQL update，且只让生成的 `SELECT` 进入 Ontop。[`../intent-engine-ontop/docs/architecture.md:210`](../../intent-engine-ontop/docs/architecture.md#L210)-[`architecture.md:224`](../../intent-engine-ontop/docs/architecture.md#L224)。重建脚本通过环境变量把各 `/sparql` URL 交给 live 用例，并以 SQL oracle 作独立比较。[`scripts/rebuild.sh:54`](../../intent-engine-ontop/scripts/rebuild.sh#L54)-[`rebuild.sh:137`](../../intent-engine-ontop/scripts/rebuild.sh#L137)。
* 固定 Ontop 基线源码中，endpoint 命令把 mapping、ontology、properties 作为启动输入（`../ontop/client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopEndpoint.java:75-153`）；SPARQL controller 支持 form POST（`../ontop/client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/SparqlQueryController.java:33-67`）。本项目只走其中 form POST 分支，而不依赖该 controller 还提供的 GET、直接 SPARQL body、图结果或布尔结果路径。

## 数据与映射能力

* 数据源仅 PostgreSQL：NPD、`fibo`、`fibo_scale` 三个库。Compose 在数据库初始化时加载 NPD SQL、生成的 FIBO schema/facts。[`compose.yaml:4`](../../intent-engine-ontop/compose.yaml#L4)-[`compose.yaml:23`](../../intent-engine-ontop/compose.yaml#L23)。这正落在 rtop 的 PostgreSQL-only 产品边界内（`datasource.kind = "postgres"`）。[`src/config.rs:89`](../../src/config.rs#L89)-[`config.rs:92`](../../src/config.rs#L92)。
* FIBO 映射是 Ontop native OBDA，使用 IRI 模板、`rdf:type`、对象/数据属性、`xsd:decimal`/`xsd:date`、以及 SQL `WHERE ... IS NOT NULL` 排除空属性。例如贷款、余额、有效期和参与方映射见 [`mapping.obda:7`](../../intent-engine-ontop/artifacts/fibo/generated/mapping.obda#L7)-[`mapping.obda:29`](../../intent-engine-ontop/artifacts/fibo/generated/mapping.obda#L29) 与 [`mapping.obda:55`](../../intent-engine-ontop/artifacts/fibo/generated/mapping.obda#L55)-[`mapping.obda:69`](../../intent-engine-ontop/artifacts/fibo/generated/mapping.obda#L69)。rtop 的配置可加载 mapping 文件，并具备 native OBDA/R2RML 转换入口，但不能仅凭静态审阅断言该映射已完全兼容；必须将此文件纳入迁移 gate。[`src/config.rs:6`](../../src/config.rs#L6)-[`config.rs:16`](../../src/config.rs#L16)、[`src/main.rs:39`](../../src/main.rs#L39)-[`main.rs:46`](../../src/main.rs#L46)。

## 实际 SPARQL 轮廓

| 轮廓 | 一手证据 | rtop 替换判定 |
| --- | --- | --- |
| BGP、IRI/字面量、`ORDER BY`、`LIMIT` | NPD 的 licence 查询 [`npd_suite.py:35`](../../intent-engine-ontop/intent_engine/npd_suite.py#L35)-[`npd_suite.py:49`](../../intent-engine-ontop/intent_engine/npd_suite.py#L49) | 已有同类 PostgreSQL/SPARQL 基础；仍应实跑 NPD 资产。 |
| `VALUES` + `OPTIONAL` 与未绑定 JSON binding | [`npd_suite.py:51`](../../intent-engine-ontop/intent_engine/npd_suite.py#L51)-[`npd_suite.py:75`](../../intent-engine-ontop/intent_engine/npd_suite.py#L75) | rtop 解析器声明支持 `OPTIONAL`/`VALUES`；替换时以该 query 的精确 bindings 验收。 |
| 聚合 `SUM`、`COUNT`、`COUNT(DISTINCT)`，`GROUP BY`、`ORDER BY`、算术、`ROUND` | 余额/放款 query [`query.sparql:4`](../../intent-engine-ontop/artifacts/fibo/generated/query.sparql#L4)-[`query.sparql:19`](../../intent-engine-ontop/artifacts/fibo/generated/query.sparql#L19)，计数 query [`fibo-loan-count-query.sparql:1`](../../intent-engine-ontop/artifacts/fibo/generated/fibo-loan-count-query.sparql#L1)-[`fibo-loan-count-query.sparql:5`](../../intent-engine-ontop/artifacts/fibo/generated/fibo-loan-count-query.sparql#L5) | rtop 有 aggregate AST；货币精度、日期比较和 JSON 词法值必须以 oracle 比较验证。 |
| `OPTIONAL`、`BOUND`、日期比较、`&&`/`!`、`IN` | allocated attribution query [`allocated-attribution-query.sparql:3`](../../intent-engine-ontop/artifacts/fibo/generated/allocated-attribution-query.sparql#L3)-[`allocated-attribution-query.sparql:17`](../../intent-engine-ontop/artifacts/fibo/generated/allocated-attribution-query.sparql#L17) | 高优先级实测项。 |
| 嵌套 `SELECT` 子查询 + aggregate | equal attribution counterexample [`equal-attribution-counterexample.sparql:3`](../../intent-engine-ontop/artifacts/fibo/generated/equal-attribution-counterexample.sparql#L3)-[`equal-attribution-counterexample.sparql:19`](../../intent-engine-ontop/artifacts/fibo/generated/equal-attribution-counterexample.sparql#L19) | 高风险项；rtop 虽有子查询测试资产，须用此 query 验证。 |
| `UNION` 与显式 `owl:sameAs` 三元组 | identity 反例 [`identity.py:80`](../../intent-engine-ontop/intent_engine/identity.py#L80)-[`identity.py:93`](../../intent-engine-ontop/intent_engine/identity.py#L93) | 查询语法可测；是否采用 owl:sameAs 的语义闭包不得假定。该项目只把它作为错误身份合并的反例 endpoint。 |

## 本体推理与控制实验

FIBO application ontology 将 `app:LoanContract` 声明为 `fibo-loan:Loan` 的 `rdfs:subClassOf`。[`application-ontology.ttl:280`](../../intent-engine-ontop/artifacts/fibo/application-ontology.ttl#L280)-[`application-ontology.ttl:282`](../../intent-engine-ontop/artifacts/fibo/application-ontology.ttl#L282)。项目随即以对超类 `fibo-loan:Loan` 的 `COUNT(DISTINCT ?loan)` 检验此蕴含，并以“删除 subclass 公理而映射/数据/SQL 不变”的独立 endpoint 作为 ablation。[`entailment.py:130`](../../intent-engine-ontop/intent_engine/entailment.py#L130)-[`entailment.py:156`](../../intent-engine-ontop/intent_engine/entailment.py#L156)。因此替换的最低推理契约是：**映射产生 `app:LoanContract` type 时，查询 `fibo-loan:Loan` 必须命中；移除该公理后必须不命中。**

rtop 的 TBox 明确按 subclass/subproperty、domain/range、inverse 做查询改写；`subclasses_of` 被注释为虚拟 mapping 的 type 查询改写用途。[`src/ontology.rs:25`](../../src/ontology.rs#L25)-[`ontology.rs:35`](../../src/ontology.rs#L35)、[`ontology.rs:79`](../../src/ontology.rs#L79)-[`ontology.rs:130`](../../src/ontology.rs#L130)。这与最低契约方向一致。另一方面，项目还构造了 `owl:equivalentClass` 过强对齐和 `owl:sameAs` 身份合并的反例 endpoint。[`compose.yaml:65`](../../intent-engine-ontop/compose.yaml#L65)-[`compose.yaml:83`](../../intent-engine-ontop/compose.yaml#L83)、[`compose.yaml:102`](../../intent-engine-ontop/compose.yaml#L102)-[`compose.yaml:117`](../../intent-engine-ontop/compose.yaml#L117)：这些不是可以跳过的“错误输入”，而是用来证明异常语义差异可观测的回归测试。

## 可执行的迁移验收契约

1. 将每个 Ontop 服务的三项输入转换为一个 rtop TOML（`mapping`、`ontology`、`[datasource]`）；映射、本体、数据库保持原文件/原库，不改 Python 调用者。不能复用 `.properties`，因为 rtop 不接受 JDBC 配置。
2. 在七个独立 rtop 进程上保留现有 host port 与 `/sparql`，支持 form POST `query=` 和 `application/sparql-results+json`；客户端的超时、状态分类、trace JSON 字段无需改动。
3. 以 `scripts/rebuild.sh` 中所有 live scenario 为端到端验收集，逐个比较现有独立 SQL oracle 与 rtop 的 `results.bindings`；特别包括 NPD BGP/OPTIONAL/subclass，FIBO 时态余额/放款、粒度、身份、货币、归属、scale。
4. 以四个本体变体锁住逻辑：主本体、删除 loan subclass、错误 `equivalentClass`、删除 weak `skos:closeMatch`；再以 unsafe identity 映射/ontology 锁住显式 `owl:sameAs` 反例。预期不能只看“成功响应”，必须与每个 scenario 的正确/反事实结果一致。
5. 在映射解析、子查询聚合、`OPTIONAL+BOUND` 日期条件、decimal `ROUND` 和 `owl:sameAs` 这五项完成实际运行前，替换状态应为“待验收”而非“功能等价”。

## 不属于当前替换范围

没有证据表明此项目使用 Ontop CLI 的 `query`、`materialize`、`bootstrap`、`validate`、metadata extraction 或 mapping 转换命令；固定对照源码虽公开这些命令（[`../ontop/client/cli/src/main/java/it/unibz/inf/ontop/cli/Ontop.java`](../../ontop/client/cli/src/main/java/it/unibz/inf/ontop/cli/Ontop.java) 的 commands 注册），但它们不在本项目调用链中。同样没有发现对 `/ontology`、`/ontop/reformulate`、预定义查询、Portal、CORS 响应头或 JDBC 以外数据库方言的客户端依赖；Compose 中的 CORS 环境变量只是镜像配置，Python runner 没有浏览器端调用证据。[`compose.yaml:31`](../../intent-engine-ontop/compose.yaml#L31)-[`compose.yaml:43`](../../intent-engine-ontop/compose.yaml#L43)。
