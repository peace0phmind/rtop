# Ontop PostgreSQL 替换层：能力与缺口清单

**调查日期：** 2026-09-06  
**Ontop 基线：** `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。  
**已决定的范围：** `SPARQL HTTP → native OBDA/R2RML → OWL 2 QL → reformulation/optimization → PostgreSQL`；只需 PostgreSQL 方言。不迁移 RDF4J/OWLAPI 的 Java 对象 API、Protégé、Portal、CLI materialize 或其他数据库方言。`intent-engine-ontop` 的七个服务是集成验收，不能把它们未触发的语法/语义当作层能力的边界。

本文件是静态盘点，故只将“源码明确没有对应表示/分支”的事项标作**确定缺口**；标作“待运行验收”的项目不应被误报为已兼容。每个最终支持情景仍须在 `compatibility-report.json` 留下来源、PostgreSQL 环境与结果。

## 结论

rtop 已经有 PostgreSQL adapter、native `.obda`/R2RML 的相当一部分读取、TBox 基本闭包、`/sparql` 的 JSON 结果及 BGP/OPTIONAL/UNION/VALUES/聚合等执行骨架。但它尚不是 Ontop PostgreSQL 查询层的功能等价实现。确定阻断项如下：

| 层 | 确定缺口 | 证据与影响 |
| --- | --- | --- |
| 查询改写与优化 | **整条 SPARQL 到单条/少量 PostgreSQL SQL 的改写、下推和语义优化不存在** | rtop 的 [`select_bgp`](../../src/lib.rs#L544-L587) 先对每个三元组模式逐个 `select`，再在 Rust 内存中 join；OPTIONAL 也按每条左行重新执行右侧 [`lib.rs`](../../src/lib.rs#L608-L629)。Ontop 的对应链路有 `QueryRewriter`、`NativeQueryGenerator`、`SQLGeneratorImpl`，并有 FK、UNIQUE、left join、aggregation 等优化器和测试（`../ontop/engine/reformulation/*`、`../ontop/core/optimization/src/main/java/...`）。这不仅是性能差异：大结果集会改变可完成性，且没有 Ontop 的基于约束的重复消除/空值/left join 等可观察语义保障。 |
| SPARQL 1.1 查询 | **大量语法和代数尚无 AST**：property path、`FILTER EXISTS`/`NOT EXISTS`、`IN`/`NOT IN`、`HAVING`、`SERVICE`、`FROM`/`FROM NAMED`、`BINDINGS` 旧语法，以及表达式排序 | rtop 的 AST 只有 BGP、Join、LeftJoin、Minus、Union、Subquery、Values、Bind、Filter（[`sparql.rs`](../../src/sparql.rs#L44-L66)）；`OrderByTerm` 只容纳变量和方向（[`sparql.rs`](../../src/sparql.rs#L195-L199)），而 parser 没有上述关键字分支。Ontop 的 SPARQL 1.1 资产直接包含 property-path、exists、negation、functions/in、aggregates/HAVING、service、dataset 与 bindings manifests（`../ontop/test/sparql-compliance/src/test/resources/testcases-dawg-sparql-1.1/`）。其中 `IN` 已被 `intent-engine-ontop` 实际调用，是近期直接阻断。 |
| SPARQL 函数与数值语义 | **不等同于标准/基线函数集；decimal 使用 `f64`** | `evaluate_function` 的明确分支只覆盖源码所列函数（[`lib.rs`](../../src/lib.rs#L1431-L1728)）；未识别函数返回 `None`，即 expression error。算术与 `SUM` 使用浮点路径（见 [`lib.rs`](../../src/lib.rs#L1169-L1187)、[`lib.rs`](../../src/lib.rs#L1237-L1255)），不能静态保证 `xsd:decimal` 的精确值和词法。PostgreSQL 语料包含 `BindWithFunctionsPostgreSQLTest`、`CastPostgreSQLTest` 及 SPARQL compliance 的 functions/aggregates；应以其实际启用项建立逐函数结果测试。 |
| SPARQL 图与 HTTP 协议 | **HTTP 输出协商及 dataset protocol 参数不完整** | Ontop `/sparql` 接受 `default-graph-uri`、`named-graph-uri`（[`SparqlQueryController.java`](../../../ontop/client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/SparqlQueryController.java#L33-L66)），并按 `Accept` 输出 SELECT/ASK 的 JSON、XML、CSV、TSV 与 CONSTRUCT/DESCRIBE 的 Turtle、RDF/JSON、JSON-LD、RDF/XML、N-Triples、N-Quads（[`SparqlQueryExecutor.java`](../../../ontop/client/endpoint-core/src/main/java/it/unibz/inf/ontop/endpoint/processor/SparqlQueryExecutor.java#L53-L140)）。rtop 只为 graph 返回 Turtle、为 bindings/boolean 返回 JSON（[`server.rs`](../../src/server.rs#L145-L164)），也未读取两个 dataset 参数（[`server.rs`](../../src/server.rs#L60-L86)）。注意 Ontop 本身对 SPARQL Update 返回 501（同文件 L127-L130），故 Update **不是**本替换范围缺口。 |
| OWL 2 QL 装载与 imports | **输入格式与 import resolution 不完整；完整 OWL 2 QL 公理翻译未证明** | rtop 仅按首字符选择 Turtle 或 RDF/XML（[`ontology.rs`](../../src/ontology.rs#L196-L224)），imports 仅接受 `file://`（[`ontology.rs`](../../src/ontology.rs#L281-L286)）。Ontop 通过 OWLAPI 获得 imports closure（[`OWLAPITranslatorOWL2QL.java`](../../../ontop/ontology/owlapi/src/main/java/it/unibz/inf/ontop/spec/ontology/owlapi/OWLAPITranslatorOWL2QL.java#L62-L83)），并可配置 XML catalog IRI mapper（`../ontop/mapping/owlapi/.../OntopMappingOntologyConfigurationImpl.java:86-93`）。因此应以 Ontop 的实际边界为准：支持其可加载的本地/URL/catalog imports closure 及 OWL 2 QL 公理的可观察改写；不能把“只接受离线闭包”预先当作已决定的缩减。 |
| PostgreSQL 语义与元数据 | **Ontop 的 PostgreSQL type/function/identifier/metadata/constraint 语义尚未被完整移植或逐项验证** | Ontop 专门实现 `PostgreSQLDBTypeFactory`、`PostgreSQLDBFunctionSymbolFactory`、`PostgreSQLQuotedIDFactory`、metadata provider 与 SQL serializer（`../ontop/db/rdb/src/main/java/.../PostgreSQL*`）。rtop 有原生 PostgreSQL adapter 和 metadata 基础类型（[`datasource.rs`](../../src/datasource.rs#L1-L92)），但不能由此推出全部类型、数组/JSON/JSONB、PostGIS、引用标识符、约束推理或 SQL 函数的等价性。基线 PostgreSQL Docker/lightweight 测试明确含 nested array/JSON/JSONB、constraints、casts、PostGIS、identifier、regex 等。 |

## 映射层的细分结论

### 已有实现轮廓，必须运行验收

- native `.obda`：prefix、mapping declaration、目标多三元组、IRI template、typed/language literal、source SQL、命名图；解析入口见 [`mapping.rs`](../../src/mapping.rs#L442-L503) 与 target parser [`mapping.rs`](../../src/mapping.rs#L1924-L2069)。
- R2RML：`rr:sqlQuery`/`rr:tableName`、subject/predicate/object/graph map、class、template/column/constant、datatype/language、blank node 和 ref-object map/join condition 都有处理分支（[`mapping.rs`](../../src/mapping.rs#L520-L932)、[`mapping.rs`](../../src/mapping.rs#L1672-L1754)）。
- RDB2RDF Direct Mapping：已有 PostgreSQL catalog → rules 的路径（[`mapping.rs`](../../src/mapping.rs#L238-L260)）。

这些只是“代码路径存在”。RDB2RDF 基线有 D000–D026 的 W3C manifest（`../ontop/test/rdb2rdf-compliance/src/test/resources/`）；其原测试跑 H2，不能直接当 PostgreSQL 通过证据，但每项 R2RML/Direct Mapping 语义都应转为 PostgreSQL fixture 后验收。特别注意：Ontop 自己也在 `RDB2RDFTest` 的 `IGNORE` 集中记录了不支持/词法差异项（`../ontop/test/rdb2rdf-compliance/src/test/java/RDB2RDFTest.java:58-89`）；rtop 的目标是 Ontop 可观察行为，不能把这些已知 Ontop 限制“修正”为超集。

### 映射层仍需查验的缺口

- native OBDA 的完整文法和元映射（动态 predicate/class IRI、SQL 查询边界、黑盒 view/metadata 设置）没有“全部测试资产已跑”的证据；`MetaMappingExpanderTest` 是 PostgreSQL 基线资产。
- R2RML 每条规则虽然有 parser 分支，仍须以 D000–D026 全量 manifest 和 PostgreSQL 执行确认：percent encoding、NULL、复合 join condition、base IRI、term-map 默认值、图、blank-node 同一性、datatype lexical form。
- 映射 source SQL 是 PostgreSQL SQL 的开放输入。rtop 使用 `sqlparser` 验证/处理 source SQL；这不是 Ontop PostgreSQL SQL parser/serializer 的同义证明。包含 JSON、array、PostGIS 或 PostgreSQL 专有语法的原始 assets 必须逐项跑。

## 本体改写：已有与尚缺

rtop 已明确实现的查询可观察路径包括 subclass、subproperty、domain/range、inverse、命名 `equivalentClass`、部分 intersection 和 disjoint facts 校验（[`ontology.rs`](../../src/ontology.rs#L11-L166)、[`ontology.rs`](../../src/ontology.rs#L235-L286)）。这覆盖 `intent-engine-ontop` 当前 `LoanContract ⊑ Loan` 和错误 equivalence 反例的一部分。

不过，这不能等价为“OWL 2 QL 已完整”。Ontop 的 translator 遍历 imports closure 中的每个 OWL axiom（上述 `OWLAPITranslatorOWL2QL.java`），而 rtop 的 loader 是一组 RDF predicate 的局部归一化。必须按 Ontop PostgreSQL 资产与 translator 可接受的 OWL 2 QL 公理建立正/负对照，包括 object/data existential restriction、property inclusion/inverse、equivalence、disjointness、class/property assertion 与不一致输入。对 Ontop 不支持或显式忽略的非 OWL 2 QL 公理，应复现其行为（拒绝、警告或忽略），不得实现更强推理。

## PostgreSQL 对照资产应如何使用

以下是“全层”验收的来源集合，而不是仅七个业务端点：

| 来源 | 覆盖重点 | 当前 rtop 结论 |
| --- | --- | --- |
| `test/docker-tests/src/test/java/.../postgres/` | datatype、BIND/functions、GROUP_CONCAT、IMDB 多 join、OPTIONAL/MINUS/VALUES/subquery/aggregate、identifier、regex、order、native mapping、meta mapping、nested JSON | 现有仓库已有多份 compatibility case，但必须重新按测试方法/资源核对，不能以目录存在宣称全量通过。 |
| `test/lightweight-tests/.../postgresql/` | cast、constraint、distinct aggregate、nested array/JSON/JSONB、PostGIS、replace | 不可省略；其中 PostGIS 仍属于 PostgreSQL 使用情景，不是“其他方言”。 |
| `test/sparql-compliance` | SPARQL 1.0/1.1 query 的 parser、代数、expression、结果语义 | 资产原本含内存/RDF4J 测试依赖，需筛出 Ontop query endpoint 可观察语义并用 PostgreSQL 映射 fixture 执行；Update 因 Ontop endpoint 501 排除。 |
| `test/rdb2rdf-compliance` | Direct Mapping 与 R2RML 标准映射语义 | 语义必须迁移；H2 的运行容器不迁移，应改由 PostgreSQL fixture 验证。 |

## rtop 当前可复用、但不能提前关闭的能力

| 能力 | 静态依据 | 验收重点 |
| --- | --- | --- |
| PostgreSQL 连接、取消、流式读取基础 | [`datasource.rs`](../../src/datasource.rs#L93-L147)，并有 adapter cancellation 测试 | 连接失败分类、超时/取消、连接复用与 HTTP 断开后的资源释放。 |
| `/sparql` GET、form POST、`application/sparql-query` POST、JSON bindings | [`server.rs`](../../src/server.rs#L55-L107) | 保留既有 JSON 请求；补齐其余 Ontop 可观察协商后再做协议矩阵。 |
| BGP、OPTIONAL、MINUS、UNION、subquery、VALUES、BIND、FILTER | AST/执行分支 [`sparql.rs`](../../src/sparql.rs#L44-L66)、[`lib.rs`](../../src/lib.rs#L590-L705) | 以 SPARQL compliance 与 PostgreSQL `LeftJoinProfPgSQLTest` 等实际 assets 验证 bag、unbound、error 与顺序语义。 |
| SELECT/ASK/CONSTRUCT/DESCRIBE | query enum [`sparql.rs`](../../src/sparql.rs#L17-L42) | 现有 CONSTRUCT/DESCRIBE 是受限实现；验证模板、图、结果格式与 dataset 后才可标完整。 |

## 实施优先级

1. **先让业务集成可运行：** `IN`/`NOT IN`、exact decimal，然后替换七个 endpoint 的配置为 rtop 配置，保持 Python HTTP 调用不变。
2. **补核心改写 seam：** 将 SPARQL algebra + TBox/mapping 展开生成 PostgreSQL SQL；把 join/filter/aggregate/order/limit、VALUES、OPTIONAL/MINUS/UNION 下推。没有它，不能满足用户所定的“reformulation/optimization 全层”目标。
3. **以资产封闭 SPARQL：** 把 Ontop 适用的 SPARQL-compliance query manifests 分批变为 PostgreSQL endpoint tests；先补 AST，再补 function/type/error/term lexical 语义。
4. **以资产封闭 mapping/OWL：** D000–D026 + Ontop OWL 2 QL/PostgreSQL 用例；每项同时记录 Ontop 基线结果和 rtop 结果，保留 Ontop 已知限制。
5. **封闭 HTTP 与 PostgreSQL 方言：** `Accept` result-format matrix、dataset 参数、错误/取消；然后 nested data、PostGIS、constraints、metadata/identifier 的 PostgreSQL gate。

在第 2–5 项完成并以固定 Ontop 基线逐项取证前，不应宣称 rtop 可以以“相关层完整功能”替代 Ontop；最多只能宣称特定 `intent-engine-ontop` 业务查询子集已通过。
