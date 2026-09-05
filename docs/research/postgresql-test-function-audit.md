# PostgreSQL 测试与功能迁移复核

关联 Issue：[ #35：PostgreSQL 17 限定的可观察 VKG 功能等价与证据闭环](https://github.com/peace0phmind/rtop/issues/35)。
远端最终 gate 证据见已关闭的 [#50](https://github.com/peace0phmind/rtop/issues/50)。

复核日期：2026-09-05。对照基线为只读的
`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。范围严格限于
PostgreSQL 限定对照范围；MySQL、Oracle、SQL Server 等其他方言以及 JVM 对象 API
不计入分母。

## 结论

当前可执行 PostgreSQL 对照的**覆盖比例为 100%（310/310 个账本原子）**，运行状态也为
100%：`compatibility-report.json` 有 237 个 `passed` case 和 1 个基线明确忽略的 case；
310 个账本原子均映射到其中的 `passed` case。该比例不是把 Java 的 class 数、JUnit
method 数和 manifest query 混为一个分母，而是以每个可复核输入/预期结果的账本原子为
分母。

| 基线资产单位 | 分母 | 已通过 | 证据 |
| --- | ---: | ---: | --- |
| 直接命名的 Docker PostgreSQL test class | 18 | 18 | `discover-postgres-baseline-assets.sh` |
| `@PostgreSQLLightweightTest` class | 10 | 10 | 同上；包含 class 名不含 PostgreSQL 的 GeoSPARQL、Replace |
| Docker PostgreSQL manifest query | 170 | 170 | 122 个 suite query，加 48 个 datatype manifest query；另有 1 个 Java `@Ignore` 的 general 类型项，不计通过分母 |
| R2RML mapping 文件 | 64 | 64 | #40 的逐文件 source-path 覆盖 |
| RDB2RDF Direct Mapping manifest output | 26 | 26 | #51 的逐 output 对照 |
| LUBM PostgreSQL manifest query | 14 | 14 | #52 的逐 query 对照 |
| PostgreSQL 交付接口资产 | 15 | 15 | CLI、HTTP、OCI 的 source-path 覆盖 |

这些资产之间存在重叠（例如一个 class 可执行多个 manifest query），因此不能把表中数字
相加后再次宣称是“测试数”。严格 gate 以 310 个不重复的账本原子判定。

## 功能逐组比对

| Ontop PostgreSQL 行为 | rtop 状态 | 实测证据/限制 |
| --- | --- | --- |
| Native OBDA、R2RML、Direct Mapping 读取及错误分类 | 已迁移并通过 | 64 个 R2RML 与 26 个 Direct Mapping 输出均在 PostgreSQL 17 上比对；覆盖 prefix、模板、blank node、named graph、join、NULL、标识符和无效输入。 |
| SPARQL 查询形式与图模式 | 已迁移并通过 | SELECT/ASK/CONSTRUCT/DESCRIBE、BGP、OPTIONAL、UNION、MINUS、VALUES、子查询、DISTINCT、ORDER/LIMIT/OFFSET 均有运行时和服务端 gate。 |
| 表达式、类型、聚合 | 已迁移并通过 | BIND/REPLACE/regex/IRI/BNODE/日期时间/哈希、CAST/constraint、DISTINCT aggregate、GROUP_CONCAT，以及 170 个 manifest query 均通过。 |
| PostgreSQL 方言与复杂值 | 已迁移并通过 | quoted/unquoted identifier、source SQL regex/LOWER、JSON/JSONB/array、PostGIS GeoSPARQL、meta-mapping、IMDB/annotation 回归均有 PostgreSQL 容器 case。 |
| 本体和 RDF facts | 已迁移并通过 | imports、subclass/subproperty、domain/range、inverse、disjoint 错误、Turtle/N-Quads/RDF/XML facts 与映射联合查询已有 case。 |
| adapter 生命周期 | 已迁移并通过 | 参数绑定、流式提前停止、取消后的稳定诊断与连接复用在真实 PostgreSQL 容器验证。 |
| CLI、HTTP、OCI | 已迁移并通过 | 查询/物化/校验/映射转换、HTTP ontology/predefined/development 行为、无 JVM 镜像、默认 endpoint、healthcheck、secret-file 均纳入最终 gate。 |
| JDBC `DriverPropertyInfo`、OWLAPI/RDF4J Java 对象 | 有意不迁移 | 这是 JVM API，不是 Rust 外部契约；替代证据为配置加载、连接与稳定诊断。不能表述为 Java API 等价。 |

## 本次执行

在当前工作树运行 `./scripts/test-final-gate.sh` 成功。它依次执行 Rust 格式检查、
142 个 Rust 测试、基线发现/方法/账本校验，以及 PostgreSQL 17、PostGIS、CLI、HTTP、
OCI 的实际 gate。运行中只有 Rust `dead_code` 警告，没有失败。

## 仍需注意的风险

1. “全部通过”只表示固定 Ontop commit、PostgreSQL 17 和已发现资产的可观察行为；不外推到其他 PostgreSQL 版本、其他方言或未冻结的 Ontop HEAD。
2. 直接 class 的继承测试未被按 JUnit method 数简单计数；其 96 个 BIND、52 个 LEFT JOIN、86 个 CAST、10 个 nested-data、8 个 constraint 等方法由 `validate-postgres-method-audits.sh` 审计其逐项映射。这是避免重复计数且保留继承语义的方式。
3. 当前 `compatibility-report.json` 是已记录的结果快照；最终 gate 重新运行脚本来证明快照、fixture hash 和当前实现未漂移，不能只信任 JSON 的 `passed` 字段。

## 主要一手来源

- `scripts/discover-postgres-baseline-assets.sh` 与 `scripts/validate-postgres-baseline-discovery.sh`：固定提交中的 class、manifest 和交付资产发现。
- `scripts/validate-coverage-ledger.sh` 与 `docs/research/postgres-only-coverage-ledger.json`：310 个原子、source path、参考结果 hash、PostgreSQL 17 环境和 `rtop` case 的可机读映射。
- `compatibility-report.json`：238 个执行 case（237 passed、1 baseline-ignored）。
- `scripts/test-final-gate.sh`：本次重跑的端到端验证入口。
