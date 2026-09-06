# `intent-engine-ontop` 以 rtop 替换 Ontop 的缺口审计

日期：2026-09-05。范围是 `../intent-engine-ontop` 当前代码在 `live` 模式中确实发送到 Ontop 的 HTTP 查询、其映射/本体输入以及容器启动边界；不把 Ontop 的 Java API、CLI、RDF4J、Protégé、Portal 或浏览器 CORS 当作替换缺口。

## 结论

替换目标是 PostgreSQL 上的虚拟知识图谱（VKG）HTTP 服务，而非 Ontop 的进程内 API。当前 rtop 的原生 `.obda`、`POST /sparql` 表单请求、SPARQL Results JSON、BGP/`OPTIONAL`/`VALUES`/`UNION`/子查询、聚合和 `rdfs:subClassOf` 查询改写已经覆盖了调用面的很大部分。

有两个明确的交付阻断项：

| 严重度 | 明确缺失 | 实际影响 | 建议验收 |
|---|---|---|---|
| 阻断 | SPARQL `IN` 表达式 | 分配归属查询、两个归属反例和规模报告都会解析失败 | 先增加 `IN`（至少 literal list）后，以三个现有 `allocated/full/equal` 查询逐一对 SQL 对照结果验收 |
| 阻断 | Ontop 容器输入约定（环境变量 + JDBC `.properties`）的兼容装载 | 现有 compose 不能不改直接启动 rtop | 在 `intent-engine-ontop` 增加每个服务的 rtop TOML 与挂载/入口配置；以原 7 个端点的 live 命令验收 |

小数精度/舍入是高风险、但不能仅凭静态阅读定为不兼容：rtop 在聚合和算术中使用 `f64`，而项目用 `xsd:decimal` 并把分后聚合舍入作为业务契约。因此它必须通过原有边界舍入情景才可宣告可替换。

## 一手调用证据

所有实际请求都以表单字段 `query` POST 到 `/sparql`，并要求 `application/sparql-results+json`；调用方只读取 `results.bindings`。FIBO 路径在 [temporal.py:135-149](../intent-engine-ontop/intent_engine/temporal.py#L135-L149)，NPD 路径在 [npd_suite.py:114-132](../intent-engine-ontop/intent_engine/npd_suite.py#L114-L132)。`live` 场景再将结果与独立 psycopg SQL 比较（例如 [npd_suite.py:200-224](../intent-engine-ontop/intent_engine/npd_suite.py#L200-L224)）。

下表以“查询族”穷举实际会提交的 SPARQL；同一模板的日期、版本、区域值变化不另计一种语法能力。

| 查询族（调用证据） | 实际 SPARQL 特性 | rtop 静态结论 | 严重度与验收 |
|---|---|---|---|
| NPD 基线（[npd_suite.py:34-95](../intent-engine-ontop/intent_engine/npd_suite.py#L34-L95)） | BGP、`a`、`ORDER BY`、`LIMIT`；`VALUES`；`OPTIONAL`；`ProductionLicence subClassOf Agent` | 支持：查询代数含 `LeftJoin`/`Values` [sparql.rs:44-66](../../src/sparql.rs#L44-L66)，解析分支见 [sparql.rs:494-559](../../src/sparql.rs#L494-L559)，类型查询扩展所有子类 mapping [lib.rs:257-303](../../src/lib.rs#L257-L303)。 | 高：NPD 资产在当前工作树未下载，不能只凭静态声称结果相同。运行原三例，并比较 bindings 的 IRI、整数和未绑定 `updated`。 |
| 余额、放款、粒度、月度时间序列和规模 stock/flow（[compiler.py:371-410](../intent-engine-ontop/intent_engine/compiler.py#L371-L410)、[scale.py:18-45](../intent-engine-ontop/intent_engine/scale.py#L18-L45)） | 多跳 BGP，typed `xsd:date` `FILTER`，`OPTIONAL` + `BOUND`，`GROUP BY`，`SUM`，`ROUND`，算术，`LIMIT`，有序投影 | 静态可见支持：`BOUND` 和 `ROUND` 已实现 [lib.rs:1525-1555](../../src/lib.rs#L1525-L1555)，聚合和投影表达式在 [lib.rs:899-945](../../src/lib.rs#L899-L945)，`OPTIONAL` 的左连接执行在 [lib.rs:612-629](../../src/lib.rs#L612-L629)。 | 高（待运行）：用原 `query.sparql`、`disbursement-query.sparql` 和 12 个月度查询与 SQL 逐行比较；覆盖有 `validTo` 与无 `validTo` 的分支。 |
| 身份版本、按贷款人计数、来源查询（[identity.py:38-73](../intent-engine-ontop/intent_engine/identity.py#L38-L73)） | BGP、`COUNT(DISTINCT)`、`GROUP BY`、`ORDER BY`、`BIND` | 静态可见支持：投影聚合解析支持 `DISTINCT` [sparql.rs:1119-1180](../../src/sparql.rs#L1119-L1180)，`BIND` 图模式和执行见 [sparql.rs:545-548](../../src/sparql.rs#L545-L548)、[lib.rs:693-705](../../src/lib.rs#L693-L705)。 | 中（待运行）：运行 identity v1/v2、loan-count v1/v2、source lookup；特别核对 `xsd:integer` 的 `value` 和 BIND 的 string 值。 |
| 参与方指标（[participation_analytics.py:27-66](../intent-engine-ontop/intent_engine/participation_analytics.py#L27-L66)） | `OPTIONAL`、`BOUND`、日期比较、`COUNT(DISTINCT)`/`SUM` | 静态可见支持，同上。 | 高（待运行）：四个问题与其 SQL 对照逐项运行，至少覆盖空的 `validTo` 绑定。 |
| 分配归属及 full/equal 反例（[compiler.py:289-367](../intent-engine-ontop/intent_engine/compiler.py#L289-L367)） | `IN`、`OPTIONAL`、`BOUND`、decimal 乘/除、`ROUND(SUM())`；equal 反例另有聚合嵌套 `SELECT` | **不支持 `IN`**：`parse_expression` 只处理逻辑、比较、算术和函数 [sparql.rs:1276-1391](../../src/sparql.rs#L1276-L1391)，源码中没有 `IN`/`NOT IN` 实现。嵌套 `SELECT` 本身已有 AST 和执行路径 [sparql.rs:461-489](../../src/sparql.rs#L461-L489)、[lib.rs:653-687](../../src/lib.rs#L653-L687)。 | **阻断**：实现 `IN` 后再验收 allocated、full、equal 三个查询，确认权重乘法、嵌套 participant count 以及未绑定结束日期的 EBV。 |
| 金额边界舍入（[compiler.py:164-194](../intent-engine-ontop/intent_engine/compiler.py#L164-L194)） | `ROUND(SUM(?amount)*100)/100`、反向的逐事实 ROUND、`COUNT` | 语法与函数路径存在；但 `SUM` 用 `f64` 累加 [lib.rs:1169-1187](../../src/lib.rs#L1169-L1187)，二元算术也用 `f64` [lib.rs:1237-1255](../../src/lib.rs#L1237-L1255)。 | 高（待运行）：必须运行 `boundary-rounding-query.sparql` 和 `per-fact-rounding-query.sparql`，按项目的两位 CNY 序列化比较，且加入 0.005、较大累加和负数边界。失败即需 decimal 算术，不能靠格式化掩盖。 |
| FIBO Loan、project extension、弱对齐/错误等价反例（[entailment.py:159-245](../intent-engine-ontop/intent_engine/entailment.py#L159-L245)） | `COUNT(DISTINCT)` 类型查询；正向 `rdfs:subClassOf`；命名 `owl:equivalentClass` 反例 | 支持：本体将 subclass 保存为闭包 [ontology.rs:45-88](../../src/ontology.rs#L45-L88)，命名 equivalent class 归一为双向 subclass [ontology.rs:246-252](../../src/ontology.rs#L246-L252)。项目所需公理明确在 [application-ontology.ttl:249-280](../intent-engine-ontop/artifacts/fibo/application-ontology.ttl#L249-L280)。 | 高（待运行）：主、本体移除、wrong-equivalence、no-weak 四个配置分别运行，期望计数为 2/0/3/2；确认 `skos:closeMatch` 不产生类型蕴含。 |
| 不安全 `sameAs` 反例（[identity.py:74-93](../intent-engine-ontop/intent_engine/identity.py#L74-L93)） | `VALUES` + 三分支 `UNION`，显式查询 `owl:sameAs`，再计数 | 该查询没有依赖一般的 `owl:sameAs` 代换推理，而是显式匹配三元组；rtop 有 `UNION` 执行 [lib.rs:648-651](../../src/lib.rs#L648-L651)，原生 mapping 可解析常量 IRI 三元组 [mapping.rs:475-515](../../src/mapping.rs#L475-L515)。反例 mapping 确实产生该三元组 [unsafe-identity-mapping.obda:72-74](../intent-engine-ontop/artifacts/fibo/generated/unsafe-identity-mapping.obda#L72-L74)。 | 中（待运行）：运行 unsafe 端点查询，期望 alpha 为 2；不要求、也不应把通用 `owl:sameAs` 本体等价推理列入这个项目的缺口。 |

## 映射、本体和部署输入

### 映射

FIBO 使用 Ontop 原生 `.obda`，有 prefix、IRI 模板、`a`、多谓词 target、typed `xsd:decimal`/`xsd:date` literal，以及将可选列拆成 `WHERE … IS NOT NULL` 的独立 mapping；原始证据见 [mapping.obda:1-69](../intent-engine-ontop/artifacts/fibo/generated/mapping.obda#L1-L69)。这些都落在 rtop 原生 parser 的受支持轮廓中：识别 mapping declaration 和 source SQL [mapping.rs:442-503](../../src/mapping.rs#L442-L503)，并专门解析 Ontop target 的多三元组序列 [mapping.rs:1924-2069](../../src/mapping.rs#L1924-L2069)。

因此 FIBO mapping 是“静态可见支持、必须运行验证”，不是已发现缺失。验收应直接让 rtop 加载未改动的 `mapping.obda`，覆盖 17 个 mappingId，并以每一类查询验证 IRI 模板、typed literal datatype、可选属性缺席和 source SQL 过滤。

NPD 的 `.artifacts/npd` 下载资产当前不在工作树（compose 仍声明挂载，见 [compose.yaml:39-43](../intent-engine-ontop/compose.yaml#L39-L43)），故无法对其真实 `.obda` 作逐规则静态解析。这是审计证据缺口，不是 rtop 功能缺口；应在依赖准备后首先做“未改动 NPD mapping 可加载”验收。

### 本体

实际肯定依赖的推理只有 `rdfs:subClassOf`：NPD `ProductionLicence → Agent`（[npd_suite.py:78-95](../intent-engine-ontop/intent_engine/npd_suite.py#L78-L95)）和 FIBO `LoanContract → Loan`（[application-ontology.ttl:279-280](../intent-engine-ontop/artifacts/fibo/application-ontology.ttl#L279-L280)）。错误等价类是被刻意运行的反例，因此命名 `owl:equivalentClass` 双向 subclass 也在验收范围内。`rdfs:domain`、`rdfs:range`、inverse、subproperty 出现在 FIBO 文件，却没有本项目发送的查询依赖它们，故不计入本次必须替代清单。

同理，`owl:sameAs` 的一般语义不被正常功能路径使用；反例显式查询 mapping 产生的该谓词，见上表。把它扩大为全局 sameAs 推理会超出本项目的真实调用面。

### HTTP 与容器

rtop 已支持所需 HTTP 合同：`POST /sparql` + `application/x-www-form-urlencoded` 的 `query` 参数 [server.rs:55-77](../../src/server.rs#L55-L77)，并为 bindings 返回 `application/sparql-results+json` [server.rs:150-160](../../src/server.rs#L150-L160)。因此 Python 客户端无需改协议；`/healthz` 也已提供 [server.rs:72-75](../../src/server.rs#L72-L75)。

但当前 compose 以 `ONTOP_ONTOLOGY_FILE`、`ONTOP_MAPPING_FILE`、`ONTOP_PROPERTIES_FILE` 和 JDBC properties 启动七个 Ontop 服务 [compose.yaml:25-151](../intent-engine-ontop/compose.yaml#L25-L151)。rtop 明确拒绝 JDBC URL/driver 配置 [config.rs:73-93](../../src/config.rs#L73-L93)，且要求 TOML 中的 `mapping` 或 `direct_mapping` 和 PostgreSQL datasource [config.rs:124-152](../../src/config.rs#L124-L152)。这意味着替换必须改 compose/挂载并为 NPD、FIBO、unsafe、no-loan、wrong-equivalence、no-weak、scale 各提供 rtop TOML；不能只把镜像名替换为 `rtop`。

`ONTOP_CORS_ALLOWED_ORIGINS` 不列为缺口：调用方是 urllib，不是浏览器，且没有读取 CORS 响应头的代码。Ontop CLI、Java API、物化、OWLAPI 和 Portal 同样没有本项目调用证据。

## 最小验收顺序

1. 增加 `IN` 并为其建立 parser/runtime 测试（string list 即覆盖真实 role filter）。
2. 在消费项目生成/挂载 rtop TOML，保留原 `.obda`、ontology 和 PostgreSQL 数据；启动七个端点。
3. 以 `live` 模式运行 NPD 三例、FIBO 核心/参与方/身份/归属/金额/本体反例和 `scale.report`，每项继续使用该项目现成的独立 SQL 对照。
4. 对每个结果同时比较 bindings 的值、IRI、缺失绑定和 decimal 的 lexical/value 行为；保存环境、源资产版本和结果到 rtop 的 `compatibility-report.json`，而不是仅记录“HTTP 200”。

