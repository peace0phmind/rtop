# SPARQL 与映射层：Ontop Java 资产、rtop 实现和 Rust 测试缺口审计

调查日期：2026-09-07。对照工作树严格固定为
`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`，本调查未修改该工作树、rtop 源码或测试。

## 结论

`src/sparql.rs` 和 `src/mapping.rs` 不是对 Ontop Java 类的逐行移植：前者自己解析有限的
SPARQL 1.1 子集，后者自己读取 native OBDA/R2RML 并生成 PostgreSQL SQL；Ontop 则经 RDF4J
parser、IQ/SQL unfolding 和 Java/RDF4J 宿主对象实现。因此“功能已具备”的可证口径应是：**给定
同一输入、PostgreSQL fixture 和可观察输出，rtop 与固定 Ontop 基线行为一致**，而不是源代码
文本相似。

已有测试足以证明许多主通路，但并非每个已实现分支都有 Rust `#[test]`。尤其：

| 模块 | LLVM 行覆盖（既有复核） | 判断 |
| --- | ---: | --- |
| `src/sparql.rs` | 78.42% | 运行时语义例子密集；主要缺解析拒绝、`VALUES` tuple/`UNDEF`、dataset 的多图 bag 行为及 query-form 边界。 |
| `src/mapping.rs` | 71.83% | R2RML 主输入和查询通路很强；serializer、Direct Mapping 的各外键分支、读取入口及负向结构组合仍主要由 shell/CLI gate 或未覆盖。 |

这也解释为何它们的覆盖率低于 Issue #104 所要求的层 90% 行/80% 分支门槛；不能用“已有
Java 测试”自动推导 Rust 已覆盖。Issue #104 明确要求每个原子连接 Ontop 来源、rtop 实现、独立
差分、fixture、结果和覆盖率归因，当前门槛仍未闭合。

## 取证方法与范围

1. 阅读 rtop 的实现和所有同仓 Rust unit/runtime/CLI 测试，以函数和可观察行为，而非测试名，建立映射。
2. 阅读固定 Ontop 基线的参数化 R2RML runner、转换/错误测试和 SPARQL 1.0/1.1 runner。它们是本调查的 Java 一手来源；runner 读取的 manifest、`.rq`、`.ttl`、`.srx` 也是一手测试资产。
3. `docs/research/ontop-layer-evidence-and-llvm-coverage-2026-09-07.md` 的 333 项账本和 Docker/CLI 差分证明已登记情景能通过，但 shell 子进程不在 `cargo llvm-cov --all` 同一 profile 中，不能替代函数路径归因。
4. 对“Java 没有测试”的行为，候选 oracle 只能取 W3C 的一手规范或固定 Ontop 源逻辑；不得用 rtop 自己的 parser/serializer 生成预期。SPARQL 的规范为 <https://www.w3.org/TR/sparql11-query/>，R2RML 的规范为 <https://www.w3.org/TR/r2rml/>。

不纳入 Java/RDF4J/OWLAPI 对象 API、JDBC 或其它方言；但其中承载的 PostgreSQL 外部 RDF/查询语义不能因宿主 API 被排除而漏测。

## 映射层

### 已实现、已有对应 Java 资产、且 Rust 已有行为测试的主路径

| 可观察能力 | Ontop 一手来源 | rtop 实现 | Rust 现状 |
| --- | --- | --- |
| R2RML `logicalTable`、subject/object 的 template/column/constant、term type、class 与多 POM | `test/rdb2rdf-compliance/src/test/java/RDB2RDFTest.java:114-181` 参数化读取 D000--D025 manifest；其 `r2rml*.ttl` 与预期 N-Quads | `mapping.rs:564-981` | `tests/runtime.rs:2694-2758, 3520-3603`，并由 compat ledger 枚举 64 个 mapping 文件。 |
| language/datatype、非法 language、datatype 与 language 互斥 | `RDB2RDFTest` 的 D014/D015 资产；`mapping/sql/all/.../R2RMLConversionTest.java:48-91` | `mapping.rs:905-928, 1949-1969` | `tests/runtime.rs:3293-3426`。 |
| `RefObjectMap` 与单/复合 join | RDB2RDF D014 等 manifest mapping | `mapping.rs:843-899, 1676-1711, 1821-1872` | `tests/runtime.rs:2502-2520, 3452-3469`。 |
| named graph、predicate map constant、graph map 的 IRI 拒绝 | RDB2RDF D006--D009 | `mapping.rs:642-731, 1043-1104` | `tests/runtime.rs:2132-2247, 3606-3663`。 |
| 原生 OBDA 多 mapping、target 的 `;`/`,`、literal、BNODE、named graph | Ontop SQL mapping tests/fixture 和 CLI 输入资产；非 R2RML Java parser 与 `R2RMLConversionTest` 是转换侧来源 | `mapping.rs:480-561, 2018-2303` | `tests/runtime.rs:2522-2673`。 |
| R2RML 读入后执行的错误分类 | `BasicR2RMLMappingMistakeTest.java:16-74`（unbound source、invalid SQL、无 object map） | `mapping.rs:576-930, 1971-1991` | `tests/runtime.rs:2734-2761, 3365-3384, 3633-3663`。 |

所以答案不是“没有 Java 用例”：RDB2RDF 的参数化 Java runner及其 fixture 是对应的源资产，rtop 的
测试多数重写为紧凑 runtime case 或在 PostgreSQL gate 使用原 mapping/SQL/预期。区别是**没有按
JUnit 方法一一同名移植**。

Java 实现也说明两边只能做语义对应：`mapping/sql/r2rml/.../R2RMLMappingParser.java:50-113`
从 Turtle file/reader/Graph 导入并把解析异常归为 `InvalidMappingException`；
`R2RMLToSQLPPTriplesMapConverter.java:119-251` 展开 subject class、predicate×object、默认/具名图
与 RefObjectMap（无 join 要求相同 SQL，有 join 建 child/parent alias SQL），其 `309-405` 决定
constant/template/column、base resolve、blank node/language/datatype。rtop 的相应职责集中在
`mapping.rs:564-981` 与 `1676-1872`，并没有可作源码文本 diff 的共同中间表示。

### 已实现但缺少 Rust 进程内归因的候选测试

下表的“缺”意为本次 `rg` 审计未找到覆盖该分支的 Rust unit/runtime `#[test]`；它不等于 Docker
账本没有覆盖相近场景。优先级按能补齐实现分支和是否已有独立 oracle 排列。

| 优先级 | 可观察行为与 rtop 位置 | Java 对应/缺失情况 | 应新增的 Rust 测试与 oracle |
| --- | --- | --- | --- |
| P0 | Direct Mapping：无主键的 blank node、FK 指向无 PK、FK 引用列等于/不等于 parent PK、缺 parent relation、空列/非法 base（`mapping.rs:257-397`） | `RDB2RDFTest` 同时加载 DirectMapping 和 R2RML（`RDB2RDFTest.java:151-153`）；它是最接近的 Java asset，但其 H2 setup 不能替代 PostgreSQL 断言 | 用固定 D000--D026 Direct Mapping SQL/expected N-Quads 加四个最小 `RelationMetadata` unit case；再在 PostgreSQL fixture 比 rtop/Ontop canonical N-Quads。缺 parent/空列/非法 base 为 rtop 输入边界，oracle 是稳定 `RuntimeError::Mapping`，不伪称 Java 覆盖。 |
| P0 | R2RML 与 native OBDA 双向转换的 term-map/graph/escaping/round-trip（`mapping.rs:180-249`） | `R2RMLConversionTest.java:48-91` 精确覆盖 column、typed constant、untyped column、language、constant IRI；CLI 还有 `OntopR2RMLToOBDATest` 等 | 将五个 Java 输入资产变为 Rust `parse -> to_r2rml/to_native_obda -> parse -> VkgRuntime query`。断言 RDF 结果而非 Turtle 格式；另对 CLI 保留 stdout/exit 单测。现有 `tests/cli.rs` 仅覆盖少量转换失败/输出。 |
| P1 | `parse_file_relaxed_source_sql` 在 endpoint 保留 native black-box SQL、普通 `parse` 严格拒绝（`mapping.rs:448-460, 1971-1991`） | `BasicR2RMLMappingMistakeTest` 只说明 Ontop 会在其配置路径验证/报告 source SQL；没有 rtop 这个入口 | 两个相同 OBDA：`parse` 断言 SQL 语法错误，relaxed 入口保留 mapping 并由 PostgreSQL 执行期给出服务器错误。该行为属于 rtop 的明确 adapter 契约，oracle 是实现注释和 #104 的稳定错误分类要求。 |
| P1 | R2RML `rr:object` 直接 IRI 与 `rr:objectMap` 多对象、多个 `rr:class` 的完整笛卡尔展开（`mapping.rs:685-981`） | RDB2RDF manifest 有资产，但 Java `R2RMLConversionTest` 只检查单个 POM/object map | 选现有 D002/D006 mapping 以一条源行 materialize，精确比 N-Quads 的条数和每一项；不能只以 `SELECT` 命中一条替代。W3C R2RML 规范的 PredicateObjectMap 产生每个 predicate/object map 的组合，是 Java 缺细粒度断言时的一手逻辑依据。 |
| P1 | `rr:sqlQuery` 尾分号归一化、`rr:tableName`、`rr:sqlVersion rr:SQL1979`（`mapping.rs:596-612, 1558-1593`） | `RDB2RDFTest.java` 将 `tc0003a` 标为 ignore（未定义 SQL version 应拒绝），故不能称有 Java 通过用例；当前 Rust 仅测 SQL1979 可读 | 新增 tableName 与 trailing semicolon 的实际查询测试；对未知 `rr:sqlVersion` 先以 W3C R2RML 的允许值决定“拒绝或忽略”的产品裁决，再写负向测试和差分，不能从 Ontop ignore 推断通过。 |
| P2 | `to_native_obda` 的 template graph 序列化、`mapped_facts` 与 `describe`（`mapping.rs:399-427`） | 不存在同构 Java 方法：Ontop 使用 RDF4J materializer/repository | 单元构造 constant/template graph mapping，验证再读入和 DESCRIBE 的 RDF term/图；Oracle 是 SPARQL DESCRIBE 与 R2RML graph map 的规范语义。需先确认 rtop 选择的 DESCRIBE 边界，避免把 Java repository API 误迁移。 |

补充的 Java 测试缺口（所以不能声称“Java 已覆盖、Rust 漏移植”）是：没有定位到直接断言
RefObjectMap 多列 join、graph map 合并/去重、reader base、blank node、relative IRI、ObjectMap
`termType`、非法 language 以及 datatype/language 互斥的 Java unit。它们在 rtop 都有明确输入
处理分支；应以 R2RML 规范及 Java converter 上述逻辑为取证，写 Rust 正反例和 PostgreSQL 差分。

## SPARQL 层

### 实现、Java 测试资产和现有 Rust 测试的对应

| 可观察能力 | Ontop 一手来源 | rtop 实现 | Rust 现状 |
| --- | --- | --- | --- |
| SELECT、BGP、FILTER/BIND、算术/逻辑/函数、RDF term 比较 | `MemorySPARQLOntopQueryTest.java:25-108` 的 SPARQL 1.0 manifest；`MemorySPARQL11QueryTest.java:20-113` 的 functions manifest | parser `sparql.rs:220-320, 1456-2133`；求值在 `lib.rs:1192-2685` | `tests/runtime.rs:825-1159, 1338-1522, 1688-1732, 1827-1873`。 |
| OPTIONAL、UNION、MINUS、subquery、VALUES、相关 EXISTS | 两个 parameterized runner 分别读取 SPARQL 1.0 optional/algebra 和 SPARQL 1.1 exists/subquery manifest；`MemorySPARQL11QueryTest.java:70-111` 也明确列出 Ontop ignore | `sparql.rs:550-1381` | `tests/runtime.rs:1109-1336, 1524-1686`。 |
| 聚合、GROUP/HAVING、ORDER、LIMIT/OFFSET | SPARQL 1.1 aggregates manifest（runner 同上） | `sparql.rs:1456-1600, 2144-2309`；执行在 `lib.rs:1168-1577` | `tests/runtime.rs:1565-1623, 1783-1825`，parser unit `sparql.rs:2965-3014`。 |
| property path（IRI、inverse、alternative、sequence、`{n}`） | SPARQL 1.1 property-path manifest；Java runner 也明示 arbitrary/zero length path 被 ignore（`MemorySPARQL11QueryTest.java:75-91`） | `sparql.rs:800-970` | parser `sparql.rs:3015-3044`；运行时有 bounded path/EXISTS (`tests/runtime.rs:1734-1781`)。 |
| FROM/FROM NAMED、GRAPH | SPARQL 1.0 dataset/graph manifest 被 Java 类声明，但仓库注释称 DATASET/GRAPH folder 缺失（`MemorySPARQLOntopQueryTest.java:12-18`），故不能称该 Java runner 已完整运行它们 | `sparql.rs:321-417, 2628-2660` | parser `sparql.rs:3045-3081`，facts/mapping graph runtime 测试在 `tests/runtime.rs:3050-3130`。 |
| ASK、CONSTRUCT、DESCRIBE | SPARQL 1.0 construct manifest 与 Java runner | `sparql.rs:280-317`，执行入口在 `lib.rs` | 一个组合测试 `tests/runtime.rs:1874-1905`，mapping 专项在 3149--3260。 |

固定 Ontop 的实际 translator 不是 parser 单测而是端到端语料的共同实现：
`core/kg-query/.../RDF4JTupleExprTranslator.java:88-145` 分派 BGP/JOIN/LEFT JOIN/MINUS/UNION/FILTER/
projection/slice/distinct/group/bind/values/order/path；`331-366` 将 VALUES 的 unbound 表示为 null；
`486-552` 处理受限 EXISTS/NOT EXISTS。`RDF4JValueExprTranslator.java:68-315` 处理表达式与
COUNT/AVG/SUM/MIN/MAX/SAMPLE/GROUP_CONCAT、逻辑、REGEX、比较、四则、LANGMATCHES、COALESCE 和
IN。rtop 分别对应 `sparql.rs` 的 AST/解析和 `lib.rs` 的求值；同样没有共享实现，故只能在运行时
RDF term/bag/error seam 比较。

### 重要边界：不能因 Ontop 有 Java 测试而声明 rtop 全 SPARQL 1.1

rtop parser 明确拒绝/限缩了 SERVICE、ASK predicate variable、非单 IRI DESCRIBE、任意长度/零长度
property path 和其它无法转换的模式（`sparql.rs:289-317, 858-900, 2241-2260`）。这与 Ontop Java
runner本身也有 ignore 不矛盾，但每个 ignore/拒绝仍应进入 #104 的矩阵；不应默默将“未支持”算成
“已覆盖”。SPARQL Update 已在 Issue #104 范围外。

### 已实现却缺少 Rust 测试的候选

| 优先级 | 行为与位置 | Java 资产状态 | 可补的测试 / 一手 oracle |
| --- | --- | --- | --- |
| P0 | `VALUES (?x ?y) { (...) (UNDEF ...) }` 的 tuple、UNDEF、与 BGP join 的 bag 语义（`sparql.rs:1101-1172`） | SPARQL 1.1 VALUES 语料由 manifest 路径可发现，但当前 rtop 紧凑测试只明确覆盖单变量和 UNION 分支（`tests/runtime.rs:1109-1233`） | 抽取固定 Ontop manifest 的一正例及 `.srx` 预期，转为 facts fixture 后断言 binding 多重性；没有可用 Java case 时按 W3C SPARQL 1.1 Query §10.2 的 solution-sequence/UNDEF 定义写最小例。 |
| P0 | query-form 负向：ASK predicate variable、DESCRIBE 非 IRI/多个资源、未知 form（`sparql.rs:280-317`） | Java runner主要测试支持的 manifest，不能替代 rtop 明确的拒绝分支 | 在 `src/sparql.rs` unit test 断言 `NotFullyTranslatable` 或 `UnsupportedSparql` 类别。oracle 是 rtop 已发布的稳定限制；矩阵应将其标为范围内“拒绝”，而非兼容成功。 |
| P0 | 多个 `FROM` 的 default graph merge、仅 `FROM NAMED` 时默认图为空、`FROM NAMED` 过滤 `GRAPH <g>`（`sparql.rs:321-417`） | Java 1.0 runner 宣称 dataset/graph folder 缺失，故没有可直接当作已跑 Java 用例的证据 | 用 N-Quads facts 三图做端到端 tests，断言 multi-FROM 的 bag（同 triple 位于两图时保留两解）和 NAMED 隔离。W3C SPARQL 1.1 Query §13 指定 dataset clause 语义，是此处一手 oracle。 |
| P1 | `CONSTRUCT` 的 `FROM` dataset clause | Ontop SPARQL11 runner 显式 ignore `constructwhere04`，原因是 Ontop SPARQL-to-IQ 不支持 CONSTRUCT FROM（`MemorySPARQL11QueryTest.java:61-68`） | 当前 rtop `CONSTRUCT` 分支直接 `group`，并未解析 dataset clause（`sparql.rs:305-315`）。先写与 Ontop 同类别的**拒绝**测试；若当前实际静默接受，应登记为差异/缺陷，而不是把此 feature 记为已具备。 |
| P1 | property path 的 inverse、alternative 与 `{n}` 的**运行时**结果（`sparql.rs:800-970`） | Java manifest含正例、且列出仅 arbitrary/zero length 的 ignore | 目前这些组合只在 parser unit 成功断言（`sparql.rs:3015-3044`）；以 facts 的三节点路径分别断言结果与重复计数。预期取 Java manifest `.srx` 或 W3C property-path 规范，不能只测“能解析”。 |
| P1 | `GROUP_CONCAT` separator、`COALESCE` aggregate fallback、HAVING 的 error/empty group（`sparql.rs:1515-1600, 2144-2247`） | Java aggregate manifest是对应来源 | 现有聚合 runtime 测试覆盖 distinct/min/max/empty（1565--1623），但本次未发现上述组合。提取 Java aggregate `.rq/.srx`，或按 W3C SPARQL 1.1 Query §11 聚合定义补最小 positive/empty/error fixture。 |
| P2 | lexical 拒绝：无闭合 quote/bracket、重复 modifier、非法 typed boolean/dateTime、非法 GRAPH token（`sparql.rs:419-447, 2144-2247, 2403-2423, 2628-2660`） | Ontop 有 SPARQL 1.0/1.1 syntax manifest，但 rtop 并未逐项执行；Java parser接受/拒绝的细节不应从类名推断 | 参数化 Rust parser negative suite：每例只断言 stable error category。首选固定 Ontop syntax `.rq`（若其 feature 在 rtop 范围内），剩余按 W3C grammar；这会直接增加未覆盖错误分支。 |

Java 层也未见 `RDF4JTupleExprTranslator` 的直接 unit test；而端到端 `ExistsTest`、
`OptionalBindTest`、`ValuesNodeQueryTest` 与 PostgreSQL `OrderByTest` 承担了部分覆盖。因此还应
把“VALUES 多列/UNDEF/空集合/外部 bindings”、“聚合 × 空/null/distinct × HAVING”、“BNODE/IRI
base/LANGMATCHES/COALESCE/IN 组合”列为规范驱动的待测矩阵，不能反向声称 Java 已提供一一对应
案例。`rdfs:subClassOf*` 是 Java translator 特化、任意/零长度路径则是其明确拒绝路径；rtop 当前
只声称有限 IRI/inverse/alternative/sequence/`{n}` path，故前者不是“已实现待补测”的 rtop 功能。

## 建议的补齐顺序与验收方式

1. 先把上述 P0 测试写为 `src/sparql.rs` / `src/mapping.rs` 的确定性 unit tests，以及使用
   `VkgRuntime + FakeSource` 的 runtime tests。它们直接提高这两个文件的 LLVM 覆盖率并固定错误类别。
2. 对每个正向 P0/P1 行，补一条独立的 PostgreSQL 17 Ontop-vs-rtop 差分；输入必须来自固定
   Ontop asset 或 W3C fixture，比较器必须比较 RDF term、bag、多重性、图及稳定错误，不能调用
   被测 rtop parser/serializer 生成 expected。
3. 对 Java 缺口，报告中所列 W3C 规范段落是唯一可接受的一手逻辑取证；新 case 应将规范 URL、
   fixture hash、PostgreSQL image digest、Ontop 是否有该 case、以及“成功/拒绝”写入闭合矩阵。
4. 重新用带矩阵测试 ID 的插桩执行测 `mapping.rs`/`sparql.rs`，而不是只重跑 333 个 shell 原子。
   在代码和分支均达 Issue #104 门槛前，不应称两层“功能完整覆盖”。

## 可复核的一手文件

- rtop：`src/sparql.rs`、`src/mapping.rs`、`src/lib.rs`、`tests/runtime.rs`、`tests/cli.rs`。
- Ontop R2RML：`test/rdb2rdf-compliance/src/test/java/RDB2RDFTest.java`、`mapping/sql/all/src/test/java/it/unibz/inf/ontop/spec/mapping/parser/R2RMLConversionTest.java`、`mapping/sql/all/src/test/java/it/unibz/inf/ontop/spec/mapping/parser/BasicR2RMLMappingMistakeTest.java`。
- Ontop SPARQL：`test/sparql-compliance/src/test/java/it/unibz/inf/ontop/test/sparql/MemorySPARQLOntopQueryTest.java`、`test/sparql-compliance/src/test/java/it/unibz/inf/ontop/test/sparql11/MemorySPARQL11QueryTest.java`，以及它们的 `src/test/resources/testcases-*` manifest/fixture。
- 范围与门槛：GitHub Issues #35、#104、#107、#108；尤其 #104 的 Testing Decisions。
