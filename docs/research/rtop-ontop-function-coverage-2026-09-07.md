# rtop 相对 Ontop 的功能覆盖调查

调查日期：2026-09-07。对照源码固定为
`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`；本报告只说明 rtop 已取证的
**PostgreSQL 17 限定、外部可观察 VKG 行为**，不把 Ontop 的 JVM 宿主 API 或其它数据库
方言误计为 Rust 产品功能。

## 结论

rtop 已覆盖 Ontop 在该限定范围内的核心 VKG 路径：加载原生 OBDA/R2RML、Turtle/N-Quads/RDF/XML
facts 和 OWL 2 QL 的已取证子集；通过 PostgreSQL 执行映射；执行范围内 SPARQL；并以 CLI、HTTP
endpoint 和无 JVM OCI 镜像交付。当前机器可验证的覆盖账本为 **333/333（100%）范围内条目均为
`passed`**。但它只是已有范围内资产的功能证据，**不能**推出“全部 Ontop 已等价”或“三层功能已
100% 等价”：相关 Issue #104–#108 仍开启，且 #104 要求补足实际代码覆盖测量与双边归因。

本次随后以 LLVM 插桩实际测得 Rust **行覆盖率为 61.80%**、**函数覆盖率为
56.75%**（`cargo llvm-cov --all --locked --summary-only`）。LLVM/Rust 在此构建中没有
输出可统计 branch（汇总中每个文件 `Branches` 都是 0），所以分支覆盖率仍为**未测量**，不能
写作 0% 或通过 #104 要求的 80%。

本次实际执行 `cargo test --all --locked`：**174/174** Rust 测试通过（35 lib、1 main、5 CLI、4
config、5 facts、7 PostgreSQL adapter、117 runtime）；该命令显示 `0 measured`，也进一步表明它不
产生代码覆盖率。

## 已覆盖功能与证据

| 功能族 | rtop 实现与测试证据 | Ontop 基线/验收证据 |
| --- | --- | --- |
| PostgreSQL 数据源与 direct mapping | `src/datasource.rs` 的 `DataSource`/`PostgresDataSource` 和 `src/mapping.rs`；`tests/postgres_adapter.rs` 覆盖参数绑定、取消、流式、数据类型、关系与约束元数据。 | `docs/research/postgres-baseline-coverage-matrix.md` 映射 PgSQL datatype、identifier、regex、metadata、PostGIS 等 20 个优先类；账本逐条钉住基线源路径。 |
| 映射输入 | `src/mapping.rs` 读取原生 `.obda` 与 Turtle R2RML，处理 template、blank node、graph、class、constant/column datatype 与 language、join/ref-object map、NULL 和错误分类；`tests/runtime.rs` 有对应回归。 | `compatibility-report.json` 的 `r2rml-d000`–`d020`、`d026` 以及 native-OBDA cases 指向 `test/rdb2rdf-compliance`、`mapping/sql/all` 的固定基线资产。 |
| RDF facts 与本体 | `src/facts.rs` 支持 Turtle、N-Quads、RDF/XML（含 base IRI）；`src/ontology.rs` 处理 imports/cycle、subclass/subproperty、domain/range、inverse 与不一致分类。 | `compatibility-report.json` 的 facts、`postgres-ontology-domain-range-facts-endpoint`、`owl-ql-inverse-property-fact-inference` 条目指向 Ontop RDF4J 测试和资源。 |
| SPARQL 查询 | `src/sparql.rs` 解析 SELECT/ASK/CONSTRUCT/DESCRIBE、BGP、GRAPH、OPTIONAL、UNION、MINUS、VALUES、FILTER/BIND、聚合、排序、LIMIT/OFFSET、部分 property path；`src/lib.rs` 在 `VkgRuntime` 执行 bag、投影、DISTINCT、表达式和聚合。`tests/runtime.rs` 有 117 项端到端 runtime 断言。 | `compatibility-report.json` 记录 PostgreSQL manifest 的 datatype/filter/modifier/simple-CQ 等逐条 case；`scripts/test-postgres-suite-compat.sh` 执行其固定语料。 |
| PostgreSQL 特性 | 聚合、LEFT JOIN、cast、约束、JSON/JSONB/array、quoted/lowercase identifier、IMDB、annotation、GeoSPARQL/PostGIS、并发等 fixture 均位于 `tests/compat/postgres-*`。 | `docs/research/postgres-baseline-coverage-matrix.md` 给出每一基线 class 到服务端 case 的映射；发现脚本确认 18 个直接类、10 个 annotated 类、27 个 RDB2RDF manifest、170 条范围内 Docker manifest query。 |
| 交付接口 | `src/main.rs` 提供 query/validate/materialize、mapping 转换和 metadata 等 CLI；`src/server.rs` 提供 `/sparql`、ontology、predefined 请求及 SPARQL JSON/XML/CSV/TSV 序列化。 | Issue [#35](https://github.com/peace0phmind/rtop/issues/35) 将 CLI、HTTP、OCI 纳入验收；最终 gate 调用 `scripts/test-delivery-compat.sh` 和 `scripts/test-oci-compat.sh`。 |

## 覆盖率口径

| 指标 | 数值 | 含义与限制 |
| --- | ---: | --- |
| PostgreSQL 限定功能账本 | **333/333 = 100%** | `docs/research/postgres-only-coverage-ledger.json` 中全部 333 个 `in_scope: true` 条目均为 `passed`。这是以测试方法、参数实例、manifest entry、CLI/HTTP 行为计数的已有需求/兼容证据；不是 Ontop 全功能的分母。 |
| 基线发现集 | 18 直接类、10 注解类、27 RDB2RDF manifest、170 条 in-scope Docker manifest query，另 1 条精确 baseline-ignored | `./scripts/validate-postgres-baseline-discovery.sh` 本次实际通过。它是发现完整性证据，不应与 333 个账本条目简单相除。 |
| Rust 回归 | **174/174 passed** | 本次 `cargo test --all --locked` 的测试数量；说明回归状态，不能推出实现代码的百分比覆盖。 |
| Rust 行覆盖率 | **61.80%** | 本次运行 `cargo llvm-cov --all --locked --summary-only`；9,976 个可计行中 3,811 行未执行。 |
| Rust 函数覆盖率 | **56.75%** | 955 个可计函数中 413 个未执行。 |
| Rust 分支覆盖率 | **未测量** | LLVM 报表把每个文件的 `Branches` 计为 0，故没有可除的分母；不能把这个显示误解为 0% 或 100%。 |

`scripts/test-final-gate.sh` 将上述 Rust 回归、基线发现、method audit、覆盖账本、闭包矩阵、兼容报告、PostgreSQL compatibility/suite/LUBM、delivery 和 OCI gate 串为统一验证入口。Issue [#50](https://github.com/peace0phmind/rtop/issues/50) 的关闭评论记录了默认分支远端 Actions artifact 对同一 gate 的通过证据。

## 范围边界与不能据此声称的功能

Issue [#35](https://github.com/peace0phmind/rtop/issues/35) 明确把认证范围限定为固定 digest 的 PostgreSQL 17 和 `VkgRuntime` 这一外部可观察 seam。因而本报告**不**声称覆盖：MySQL/MariaDB、Oracle、SQL Server、Trino、DuckDB 等方言；其它 PostgreSQL 主版本；JDBC/JNDI、RDF4J、OWLAPI、Protégé、Spring/Tomcat、Maven/JAR/OSGi/JRE 等 JVM 进程内 API；portal/自动重启；未独立决策的 SPARQL Update 和完整 OWL 2 DL。

Issue [#1](https://github.com/peace0phmind/rtop/issues/1) 与 [#8](https://github.com/peace0phmind/rtop/issues/8) 仍为路线图/更宽规格；[#104](https://github.com/peace0phmind/rtop/issues/104)–[#108](https://github.com/peace0phmind/rtop/issues/108) 也仍开启。尤其 #104 规定历史账本不能单独证明功能分母或双边实际等价，并要求取得实测行覆盖率至少 90%、分支覆盖率至少 80%；这些是待达成门槛而非当前结果。因此不能因这 100% 的 PostgreSQL 限定账本而解释为“完整 Ontop 等价”。

## 一手来源与复现方式

- rtop 实现：`src/{config,datasource,facts,mapping,ontology,sparql,server,lib,main}.rs`；测试：`tests/*.rs` 与 `tests/compat/`。
- Ontop 一手源码/测试资产：`../ontop` 的固定提交 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`（本次已用 `git -C ../ontop show` 验证该提交）。
- 机器账本与案例来源：`docs/research/postgres-only-coverage-ledger.json`、`compatibility-report.json`；后者为每个情景记录基线 `source_paths`、环境和结果。
- Issue 一手材料：[#35](https://github.com/peace0phmind/rtop/issues/35)（规格与范围）、[#50](https://github.com/peace0phmind/rtop/issues/50)（最终 CI artifact）、[#1](https://github.com/peace0phmind/rtop/issues/1) 与 [#8](https://github.com/peace0phmind/rtop/issues/8)（未收敛路线图）、[#104](https://github.com/peace0phmind/rtop/issues/104)（覆盖率/双边等价门槛）。按 `docs/agents/issue-tracker.md` 用 `gh issue list --state all --json ...` 读取。

建议将对外表述写为：“rtop 对固定 Ontop 基线的 PostgreSQL 17 限定、外部可观察 VKG 资产账本为 333/333 passed；当前 Rust 行覆盖率为 61.80%、函数覆盖率为 56.75%，分支覆盖率尚未可测。”
