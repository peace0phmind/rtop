# PostgreSQL 限定的剩余功能迁移计划

**参考基线：** `../ontop` 提交 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。
**范围决定：** 关系数据源只验证 PostgreSQL；不新增 MySQL/MariaDB 或其他方言工作。PostgreSQL 第一阶段已覆盖其直接 Docker/轻量语料；固定 Docker PostgreSQL manifest 已逐条覆盖 170 条 in-scope 查询（另有 1 条 Java 显式忽略）；本计划处理其余跨语言可观察功能，而不是重复计算数据库数量。
**目标状态：** 本文件是 Issue #8 的 PostgreSQL 限定后续目标；在下述“最终完成门槛”满足前不得标记为完成。此前的第一阶段完成结论只代表已盘点 PostgreSQL 直接语料通过，不替代本文件的功能账本验收。

## 当前证据审计（2026-09-05）

本地固定基线与 PostgreSQL 17 Docker 验收已完成：覆盖账本有 310 个 `passed`
资产，R2RML 固定 64 个 mapping 变体、11 个非元 CLI 任务、4 个 in-scope HTTP
路由组、26 个 Direct Mapping manifest output、170 个 PostgreSQL Docker manifest
query（含 DockerPostgresTestSuite 122 项及 LUBM 14 项）均由对应门禁反查。`general-Type: all` 是
`PgsqlDatatypeTest` 源码显式 `IGNORE` 的唯一 general manifest 项，记录为
`baseline-ignored`，不计入通过数。

当前尚不能将整个目标标为完成：`.github/workflows/postgres-final-gate.yml` 仍只在本地
未提交工作树；2026-09-05 通过 GitHub Actions API 查询该 workflow 在默认分支返回
404，且没有远端 run。故 `scripts/test-final-gate.sh` 的本地通过不是“干净 CI Docker
runner 已通过”的替代。提交并推送这套变更后，必须由该 workflow 在远端重跑并保存
provenance artifact，才满足最终完成门槛第 3 条。

## 完成定义与共同规则

每个工作包先把 Ontop 的功能原子或 manifest 条目登记为 `passed`、`baseline-ignored`、`excluded`、`deferred`、`unsupported` 或 `unknown`，再将其映射到一个或多个 `compatibility-report.json` case。`passed` 必须在独立的 `postgres:17` 服务端（使用 PostgreSQL fixture 时）由 Rust CLI 或 HTTP 服务得到结果；纯输入/事实情景可不启动数据库，但不得把其结果算作另一种数据库支持。

**认证版本承诺：** 当前仅认证 `postgres:17` 的固定 image digest；它是 PostgreSQL 访问支持的发布承诺下限，而非 PostgreSQL 所有主版本的兼容声明。每个新增 PostgreSQL 主版本必须先有独立 Issue、服务端 gate 和账本 provenance。

不迁移的 Java 宿主能力：RDF4J/OWLAPI 进程内 binding、Protégé、JDBC/JNDI/DataSource、Maven/JAR/OSGi/JRE 发行物。它们记录为 `excluded`，不构成待实现功能。SPARQL Update、完整 OWL 2 DL、portal、自动 restart 等仍需单独产品决定，默认不进入本计划。

### 审查决定（默认选项，已确认）

1. **范围单位。** “严格继承”指 PostgreSQL 限定对照范围内的跨语言可观察行为，不指复制 Java 类、RDF4J/OWLAPI 对象或 JDBC API。每个基线测试方法、参数化 manifest entry、CLI 任务和 HTTP 路由组都必须在覆盖账本中出现。
2. **状态语义。** `passed` 是唯一的实现完成状态；基线已有 `@Ignore` 可标为 `baseline-ignored`，Java 专属契约可标为 `excluded`，两者都必须附基线来源与理由。`deferred`、`unsupported`、`unknown` 与没有 `rtop` case 的条目均是未完成，阻止目标关闭。
3. **交付默认值。** 保留所有能由原生 CLI、SPARQL HTTP 或 OCI 观察的核心 VKG 行为；`/ontology` 与 `/predefined/{id}` 纳入实现，portal、自动 restart、Java 专属接口默认 `excluded`。任何改变此默认值的项目必须先更新 Issue #8 和本文件。
4. **服务端证据。** 含映射、改写或关系数据源的成功情景必须使用独立 PostgreSQL 服务端；纯 parser/facts 输入情景记录为 `input`，不得冒充数据库覆盖。每个服务端 case 固定 PostgreSQL 镜像 digest、初始化 SQL 与 adapter 版本。

### 发散审查决定（默认选项，已确认）

1. **发现集不是文件计数。** 资产单位是可执行的测试方法、参数化实例或 manifest entry，而不是 Java 文件、`.rq` 文件或 case 总数。一个文件中的所有参数化实例都必须独立入账；只被 `@Ignore` 的实例也必须登记。
2. **PostgreSQL 适用性分类。** `in_scope` 仅由可复现规则产生：显式 PostgreSQL Docker/profile/annotation/manifest 的资产，或可在不改变观察语义的前提下以 PostgreSQL fixture 执行的核心输入、SPARQL、映射、本体、CLI/HTTP 情景。不能移植到 PostgreSQL 的通用合规测试不是静默跳过，而是 `excluded` 并注明其不可替代的 Java/非 PostgreSQL前提；不得把 1,365 个查询文件直接当作 PostgreSQL 分母。
3. **对照结果优先级。** Ontop 已提交的 expected result 是首选基线；expected 缺失、依赖动态值或存在歧义时，必须在固定提交、JDK 11/17 与记录的容器环境实际运行 Ontop，保存规范化对照结果 hash。`rtop` 的自定义期望值不能单独构成等价证据。
4. **比较规则。** tuple 比较变量、RDF term、行多重性，只有显式 `ORDER BY` 才比较顺序；ASK 比较 boolean；graph/quad 比较 canonical N-Quads 或 blank-node 同构；失败比较稳定错误类别、输入位置和主要原因。SQL 仅作同 PostgreSQL 方言诊断快照。
5. **交付范围收敛。** `/ontology` 与 `/predefined/{id}` 都是外部 HTTP 行为，纳入账本和实现；portal、portal config、自动 restart 是 UI/进程生命周期而非 VKG 查询契约，维持 `excluded`。11 个非元 CLI 任务均为外部功能，默认必须 `passed`，不能以 `excluded` 替代实现。
6. **目标生命周期。** 已关闭的“PostgreSQL 第一阶段”只代表既有直接语料完成。本文件代表新的后续目标；在创建新的活动 goal 或其等价的 GitHub 子 Issue 集之前，不得声称旧 goal 的完成状态覆盖本计划。

这些决定采用 [ADR-0001](../adr/0001-postgresql-limited-observable-equivalence.md) 的范围边界；它是有意的长期取舍，不应由单个实现 Issue 悄然改变。

### 第三轮发散审查决定（默认选项，已确认）

1. **断言强度不能被夸大。** 账本记录 `assertion_strength`。基线给出 tuple/RDF/boolean/graph expected 时，`passed` 必须比较完整规范化结果；只有 Ontop 基线本身仅以 `rsi:size` 断言时才可比较基数，并标为 `cardinality-only`。其中以 `rsi:size` 驱动的 DockerPostgresTestSuite 122 条 PostgreSQL manifest 仍是有效的“继承原断言”证据，但不能据此声称 RDF term 逐项等价。
2. **忽略项必须可审计。** `baseline-ignored` 必须记录忽略注解的精确来源、适用 profile/参数化实例和 Ontop 忽略理由；父类或同文件中其他测试被忽略不构成证据。`excluded` 必须指向 ADR-0001 的具体 Java 或非 PostgreSQL前提，不能以实现困难、耗时或缺少 fixture 为理由。
3. **时间与资源是正确性。** 每个 PostgreSQL-backed case 声明 timeout（默认 5 分钟）和清理策略；额外记录取消、连接复用、流式提前停止、容器清理的适用性。超时、泄漏、挂起或依赖前一条测试残留状态均为失败，不能标 `deferred` 后关闭。
4. **可复现不等于本机曾通过。** 最终 gate 必须在 CI 的干净 Docker runner 重跑，固定 Ontop commit、PostgreSQL digest、Rust `Cargo.lock`、初始化 SQL hash 和 OCI Dockerfile hash；密码仅以 secret-file/环境注入，绝不写入账本。外部 registry 不可用时记录为外部阻断，不得伪造 OCI 通过。
5. **架构审计必须闭环。** 每关闭 10 个 #8 子 Issue，审计若发现重要边界/模块改进，必须先实现并通过完整回归再继续关闭下一批；仅非重要建议可记录为不实施。Issue 计数仅统计 #8 的已关闭子 Issue，不混入历史或无关 Issue。

这些决定采用 [ADR-0002](../adr/0002-postgresql-17-certification-and-evidence-strength.md)；任何扩大 PostgreSQL 版本承诺或降低结果比较强度的修改都必须更新 ADR、账本和 #8。

### 覆盖账本的最低结构

工作包 0 创建 `docs/research/postgres-only-coverage-ledger.md`（或等价机读文件），每行至少包含：`asset_id`、Ontop `source_path`、测试方法或 manifest entry、`scope`、`in_scope`、状态、`status_reason`、`assertion_strength`、`rtop_case_id`、对照结果来源（提交的 expected 或实际 Ontop 执行）、对照结果 hash、`ontop_commit`、JDK/container、PostgreSQL image digest、初始化 SQL hash、timeout、清理策略、执行命令、Issue 编号与复核日期。`baseline-ignored` 另有原始 ignore 来源、profile/参数与理由；`excluded` 另有 ADR-0001 条目。`compatibility-report.json` 的 case 必须保存相同的来源路径、基线 commit、scope、状态和环境 provenance；账本不可仅以聚合计数代替逐条关联。

账本实现必须提供 JSON Schema 或等价的 machine-check，并拒绝：重复 `asset_id`、缺失 source path、`passed` 缺失 `rtop_case_id`/对照 hash/assertion strength、PostgreSQL-backed `passed` 缺少 digest/初始化 SQL hash/timeout/清理策略、`baseline-ignored` 缺少原始来源、`excluded` 缺少 ADR 条目，以及所有 `in_scope` 的非完成状态。现有报告的自由文本 `server_environment` 不能单独满足该校验。

### 最终完成门槛

目标只能在以下条件同时成立时关闭：

1. 覆盖账本对全部发现资产均有分类；`in_scope` 状态均为 `passed` 或 `baseline-ignored`，`excluded` 仅可用于 ADR-0001 所列 Java/非 PostgreSQL前提，且没有 `unknown`、`deferred`、`unsupported`；
2. 每一个 `passed` 的 PostgreSQL-backed 条目均有来自 Rust CLI、HTTP 或 OCI 的真实服务端证据和 `compatibility-report.json` case，且结果比较至少达到账本记录的对照断言强度；
3. `cargo test --all --locked`、`cargo fmt --check`、全部 PostgreSQL gate、`jq empty compatibility-report.json` 与 `git diff --check` 在本机及干净 CI Docker runner 均通过；
4. 所有关联 Issue 已按其验收条件关闭，且每关闭 10 个 #8 子 Issue 都有一次架构审计记录；重要改进已经实施并通过完整回归。

## 工作包与顺序

| 顺序 | 工作包 | 剩余功能与交付物 | PostgreSQL 验收 | 退出条件 |
| ---: | --- | --- | --- | --- |
| 0 | 覆盖账本 | 建立 Ontop manifest/功能原子到 case ID 的一对一账本，含来源路径、状态、原因、结果 hash；以它替代当前不能相除的总比例。 | 对需关系数据源的 case 固定 `postgres:17` digest、初始化 SQL 与 `rtop` adapter。 | 所有目标资产均有一个状态，且 `compatibility-report.json` 可机械校验。 |
| 1 | SPARQL 查询语义 | 以 `test/sparql-compliance` 的 107 个 manifest、1,365 个查询为**发现输入**，先依适用性规则生成 PostgreSQL 限定分母，再按 query form、BGP、OPTIONAL/UNION/MINUS/VALUES、subquery、dataset/GRAPH、property path、表达式、聚合、排序/分页分组实施。 | mapping 驱动的情景使用基线 PostgreSQL fixture；facts-only 情景记录为 `input/query`，并以同一 Rust query seam 对照。比较 RDF term、行多重性、ASK 与 graph 结果。 | 账本中每个 in-scope manifest entry 都有真实 Ontop 对照和 rtop 结果；不以“可解析”或 `unsupported` 代替结果相同。 |
| 2 | R2RML 与原生 OBDA 完整语料 | 补齐尚未取证的 28 个 R2RML mapping 变体：logical table、subject/predicate/object/graph map、constant/column/template、term type、ref object map/join、datatype/language、base IRI、结构错误；同时补齐 `.obda` 的 reader、行号诊断、重复 mapping ID、拒绝规则和转换边界。 | 每个成功 mapping 用 PostgreSQL 表与原始 SPARQL 运行；结构/解析失败也通过 PostgreSQL 配置入口触发，确保不依赖 JDBC。 | R2RML 分母 64 的每项已有状态，成功项在 PostgreSQL 返回对照结果，失败项有稳定错误代码。 |
| 3 | facts 与本体（TBox） | 扩展并逐项审计 Turtle/N-Quads/RDF/XML 的 format/base IRI/error 行为；补全 imports closure、XML catalog 决策、已观察 OWL 2 QL class/property 公理、assertion、disjoint/inconsistency 与不支持特性的分类。 | 映射 + facts + ontology 的联合查询放在 PostgreSQL 服务端上执行；输入失败 case 固定为 Rust 诊断。 | 每个已观察的 Ontop 入口语义有 fixture；完整 OWL 2 DL 不被误报为支持。 |
| 4 | PostgreSQL 上的改写与 SQL source 边界 | 将当前受限 SQL source parser、约束/优化和 mapping expansion 的差异拆为合法、非法、未支持三类；补足 PostgreSQL parameter type、NULL、date/time、numeric、identifier、regex、JSON/array 与错误传播的语义回归。 | 全部通过独立 PostgreSQL 服务端，比较外部查询结果与稳定错误；SQL 文本只作同方言诊断快照。 | 每个状态均有服务端 case，且不把任意 SQL parser 接受度当作 Ontop 等价。 |
| 5 | CLI 文件任务 | 在已实现 `query`/`validate`/`endpoint` 外，实现 `materialize`、`bootstrap`、`extract-db-metadata`、`to-r2rml`、`to-obda`、`pretty-r2rml`、`v1-to-v3` 与 `compile`；每项先冻结 Ontop 的输入、生成文件和失败类别。 | `materialize`、`bootstrap`、metadata 必须对 PostgreSQL 实例执行；转换/编译任务使用 PostgreSQL mapping fixture 验证输出可重新加载查询。 | 11 个非元 CLI 任务均为 `passed`，不保留 `deferred/excluded` 缺口。 |
| 6 | HTTP 协议剩余路由 | 完成 `/ontology` 的开关、下载和 404 契约，以及 `/predefined/{id}` 的定义、参数、错误和结果契约；portal config/root、restart 维持 `excluded`。 | `/sparql`、`/ontology`、predefined query 均由 PostgreSQL-backed VKG 情景验证 GET/form/raw POST、status、headers 与内容协商。 | 7 个参考路由组均具有实现或 ADR-0001 允许的排除理由；不以 `/healthz` 充数。 |
| 7 | OCI 与发布回归 | 将所有保留 CLI/HTTP 情景接入无 JVM OCI gate：默认 endpoint、命令覆写、secret-file、healthcheck、PostgreSQL 可达性与错误诊断。 | OCI 容器连接独立 PostgreSQL 服务端；记录镜像与 PostgreSQL digest。 | 每个保留交付功能至少有一个 OCI + PostgreSQL 端到端 case。 |
| 8 | CI 与证据保鲜 | 将标准最终验证和账本 schema 校验接入干净 Docker CI；采集账本、规范化对照结果及 image digest 作为构建产物。 | CI 启动固定 PostgreSQL 17 digest，隔离每个 fixture，并验证超时、清理及失败诊断。 | CI 重跑全部发布 gate；镜像/基线/lockfile 变化会使受影响账本条目失效并要求重采集。 |

## Issue 执行与验收节奏

工作包不是可直接关闭的票据；每项先按覆盖账本拆成可独立验收的 GitHub 子 Issue，并在 #8 下记录依赖关系。工作包 0 是其余项目的 blocker；之后工作包 1、2、3 可按互不重叠资产并行，工作包 4 依赖其所用的查询/映射/本体资产，工作包 5 依赖 mapping/adapter 契约，工作包 6 依赖 query/ontology 契约，工作包 7 依赖已保留的 CLI/HTTP 功能，工作包 8 依赖全部保留功能的 gate。每张 Issue 的正文必须包含基线资产清单、预期账本状态变化、PostgreSQL gate、`compatibility-report.json` 变更与完整回归命令。

关闭第 10、20、30……张 #8 子 Issue 前，执行一次架构审计：检查 `VkgRuntime`、输入/查询改写、`DataSource` 和交付 adapter 的边界是否仍保持单向依赖；把采纳或拒绝的改进、理由及回归命令写入 #8 评论。重要改进必须在继续关闭下一批 Issue 前实施并通过完整回归；架构审计不以重命名或格式化替代可验证改进。

## 推荐里程碑

1. **M1：可计量基线。** 完成工作包 0，并输出各能力面的分母、状态和阻塞原因。之后才重新发布总迁移比例。
2. **M2：查询与映射。** 完成工作包 1–2；这会优先缩小 SPARQL 与 R2RML 的未知区，而不是增加数据库种类。
3. **M3：输入和 PostgreSQL 改写闭环。** 完成工作包 3–4，确保 ontology、facts、mapping 和 PostgreSQL 查询改写可以组合验收。
4. **M4：用户交付面。** 完成工作包 5–7；每个保留的 CLI/HTTP/OCI 功能都有 PostgreSQL 服务端证据。
5. **M5：可再现发布证据。** 完成工作包 8；干净 CI 重跑全部 gate 并保存可复核产物。

## 标准最终验证

```sh
./scripts/test-postgres-compat.sh
./scripts/test-postgres-suite-compat.sh
./scripts/test-oci-compat.sh
cargo test --all --locked
cargo fmt --check
jq empty compatibility-report.json
git diff --check
```

覆盖账本还必须有独立的机械校验命令：确认基线发现集与账本 `asset_id` 完全相等，并拒绝任何 in-scope 的非完成状态。该命令在工作包 0 落地后加入这里，且成为每张关联 Issue 的关闭 gate。

## 当前不排期项

- MySQL/MariaDB、Oracle、SQL Server、Trino、DuckDB 等所有非 PostgreSQL adapter、方言和 Docker gate；
- JDBC 属性、driver metadata 的 Java 类型 API，以及 Java 配置兼容层；
- RDF4J/OWLAPI 进程内 API、Protégé、Spring/Tomcat 具体实现、JAR/Maven 发行；
- 未作产品决定的 SPARQL Update、完整 OWL 2 DL、portal UI、自动重启。

这些项目若将来重新纳入，必须先新建独立 issue，不能借本 PostgreSQL 限定计划宣称已覆盖。

## 依据

- [非数据库功能迁移比例审计](./ontop-non-database-migration-ratio.md)：当前可量化缺口与统计限制。
- [输入语义审计](./ontop-input-semantics.md)：OBDA、R2RML、facts 与本体的功能原子。
- [交付接口审计](./ontop-delivery-interface-contracts.md)：CLI/HTTP/OCI 的保留与排除边界。
- [PostgreSQL 覆盖矩阵](./postgres-baseline-coverage-matrix.md)：已完成 PostgreSQL 第一阶段的证据，避免重复实现。
