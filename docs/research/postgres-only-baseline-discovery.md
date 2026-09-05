# PostgreSQL 限定基线发现集

固定来源为只读 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。发现命令是：

```sh
./scripts/discover-postgres-baseline-assets.sh
```

其结构、唯一 asset ID、分类值和全部基线 source path 可由以下命令机械校验：

```sh
./scripts/validate-postgres-baseline-discovery.sh
```

该命令的 collection 是发现边界，**不是**覆盖分母：直接或带
`@PostgreSQLLightweightTest` 的 class 必须展开其继承测试方法；Docker 与
R2RML manifest 必须展开至每条 entry；SPARQL compliance manifest 以
`discovery-only` 明示仅作发现输入，
需逐条判断能否在 PostgreSQL 不改变观察语义地运行。CLI 与 HTTP 以外部任务和路由组
为单位，不迁移 Java class/API。

直接 PostgreSQL/轻量测试类同时导出 `declared_test_method_assets`，每项由 class 与
`test*` 方法名组成稳定 ID。父类继承集不混入该数组，继续由
`validate-postgres-method-audits.sh` 以精确父类源进行集合比对。
类上直接标注 `@Ignore` 或 `@Disabled` 的方法分类为 `baseline-ignored`；该分类来自
固定基线注解，而不是以实现缺口替代。

Docker manifest 同时输出每个 `manifest-pgsql.ttl` 的 `qt:query` 实例数与源路径；
因此 11 个 manifest 文件不会被折叠成 11 个覆盖资产。这个列表包括现有
`DockerPostgresTestSuite` 的 stockexchange 语料，也包括 `PgsqlDatatypeTest` 的
boolean/character/datetime/general/numeric 语料，后续账本必须逐实例分类。
固定基线当前共发现 171 条 Docker PostgreSQL manifest entry：170 条 `in-scope`
查询必须有独立 passed 账本资产，唯一 `general-Type: all` 为 `baseline-ignored`。

命令还输出五个目前已知的继承方法集合。它们来自基线父类源码，而不是人工汇总：
`AbstractBindTestWithFunctions`、`AbstractLeftJoinProfTest`、
`AbstractDistinctInAggregateTest`、`AbstractCastFunctionsTest` 和
`AbstractNestedDataTest`。其计数变动会直接改变发现输出，促使账本重新展开；不会被
class 级“已覆盖”条目静默吞掉。

其中 BIND、LEFT JOIN 和 Cast 已有逐方法审计文档。以下命令从父类源重新提取
每一个 `test*` 名，并与文档中的反引号方法名做集合相等比较（包括基线 ignore 或
PostgreSQL 子类禁用的方法）：

```sh
./scripts/validate-postgres-method-audits.sh
```

CLI/HTTP 的外部资产与 ADR-0001 排除理由见
`postgres-delivery-asset-classification.md`；同一分类也由发现命令的
`delivery_assets` 输出提供机器可读入口。

R2RML 与 Direct Mapping 的逐目录 `in-scope` 分类及当前尚无报告来源的目录见
`postgres-r2rml-discovery-classification.md`。

最终账本验证器还会逐项读取发现器导出的 18 个直接 Docker class 和 10 个
`@PostgreSQLLightweightTest` class，要求 `compatibility-report.json` 存在精确引用该
Java source path 的 `passed` case。方法名集合由 `validate-postgres-method-audits.sh`
另行精确比较；两条检查共同避免 class 或声明方法因聚合 fixture 而静默失去证据。

Direct Mapping 与 R2RML 不是互斥目录分类：同一 RDB2RDF manifest 可同时含两类 entry。
发现输出分别给出 `r2rml_entries` 和 `direct_mapping_entries`，从而强制 #51 对每一条
DirectMapping test 取证，而不把带 R2RML 的目录误认为已完成 direct mapping。

本阶段保留的分类规则如下：

- `in-scope`：可用 Rust CLI、HTTP 或 OCI，在 PostgreSQL 17 服务端或同一
  `VkgRuntime` seam 观察的 VKG 行为；后续账本必须把它变为 `passed` 或带精确理由的
  `baseline-ignored`。
- `baseline-ignored`：仅当固定基线中的该方法或参数实例本身存在 ignore 注解，且账本
  保存注解位置与理由。
- `excluded`：仅限 ADR-0001 所列 JVM/JDBC/RDF4J/OWLAPI 进程内对象等不可替代宿主
  前提；不得因实现困难、缺 fixture 或耗时排除。

当前已有的 `postgres-baseline-test-inventory.md`、
`postgres-baseline-coverage-matrix.md` 和 `compatibility-report.json` 仅为
已实现证据来源。#37 的后续修改会以此机械发现输出为输入，生成逐方法/逐 entry 的
完整资产账本，并让 `validate-coverage-ledger.sh` 校验两个 ID 集合相等。
