# Ontop 非数据库功能迁移比例审计

**审计日期：** 2026-09-05
**参考基线：** `../ontop` 的 Git 对象 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。所有 Ontop 取证均通过 `git ls-tree` 或该提交下受跟踪源码完成，未读取或修改其未跟踪文件。

## 结论

不存在一个严谨的“除数据库兼容数量外的总体功能迁移比例”。Ontop 的 Java 模块、JUnit 类、资源文件和 rtop 的 Rust 测试/`compatibility-report.json` case 并非相同粒度，也没有一份完整的 Ontop 能力 atom 到 rtop case 的一对一映射。因此，把代码行、模块数、`573` 个 Ontop `*Test.java` 与 `136` 个 Rust `#[test]` 相除都会产生误导。

目前可复算、可对外报告的分项比例是：

| 可比能力面 | 分子（rtop 已取证） | 分母（Ontop 基线） | 比例 | 解释 |
| --- | ---: | ---: | ---: | --- |
| R2RML 映射语料覆盖（代理指标） | 64 个具备 #40 `passed` 账本证据的变体 | 64 个 `r2rml*.ttl` 变体 | **100%** | 反映固定语料逐项有证据，不等价于 R2RML 标准完整符合率。 |
| 非元 CLI 任务 | 11 | 11 | **100%** | `query`、文件工作流和 mapping 互操作均以 PostgreSQL gate 取证；不计 `version`、`help`。 |
| HTTP 路由组 | 4 | 4 个 in-scope 路由组 | **100%** | `/sparql`、`/ontop/reformulate`、`/ontology`、`/predefined/{id}`；portal/restart 三组明确 excluded。 |

除这些比例外，可以确认 rtop 已有可执行证据的能力包括：原生 OBDA、部分 R2RML、Turtle/N-Quads/RDF/XML facts、部分 OWL 2 QL 推理、SELECT/ASK/CONSTRUCT/DESCRIBE、部分 SPARQL algebra/expression、原生 CLI、HTTP 和 OCI。它们尚未各自建立完整基线枚举，故报告为“已取证”，不杜撰百分比。

## 统计边界

本审计明确排除数据库**数量**及其相关证据：数据库厂商/方言、JDBC/ODBC 或原生传输层、连接认证、SQL metadata、性能、Docker 数据库镜像，以及每个数据库的测试 case 数。也排除已在迁移决策中明确不迁移的 Java 宿主接口：RDF4J/OWLAPI 进程内 binding、Protégé、JAR/Maven/OSGi 和 JDBC Java 配置。

SPARQL、R2RML 和 OWL 的端到端语义可能使用数据库 fixture，但其语义能力不因“不按数据库厂商计数”而自动排除；本报告仅避免把数据库种类或案例量放进比例分母。

`完成`只表示存在 rtop 本地实现和测试/compatibility case；不表示已经逐条与 Ontop 黑盒结果等价。功能等价所需的 Ontop 输出、环境与结果规范化要求，见 [rtop-functional-equivalence-acceptance.md](./rtop-functional-equivalence-acceptance.md)。

## 分项取证

### R2RML：64/64（100%）

分母为 Ontop `test/rdb2rdf-compliance/src/test/resources` 中受跟踪、匹配 `/r2rml[a-z]*\.ttl` 的 64 个映射变体；分子为覆盖账本中 #40 `passed` 条目按 source path 反向匹配的 64 个变体。该匹配已由 `validate-coverage-ledger.sh` 强制执行。rtop 的读取和运行实现在 `src/mapping.rs`，R2RML graph/direct-object 回归见 PostgreSQL gate。

该数值是“语料已取证覆盖率”：一份 mapping 文件可涉及多个 R2RML feature，一个 case 也可能覆盖多个变体；不能据此声称 R2RML 规范条款已 100% 实现。

### CLI：11/11（100%）

Ontop 在 `client/cli/src/main/java/it/unibz/inf/ontop/cli/Ontop.java:35-64` 注册可执行功能任务。排除 `version` 与 `help` 后，11 项为 `query`、`materialize`、`bootstrap`、`validate`、`endpoint`、`extract-db-metadata`、隐藏 `compile`，以及 `to-r2rml`、`to-obda`、`pretty-r2rml`、`v1-to-v3`。

`src/main.rs` 提供全部 11 个任务。`scripts/test-delivery-compat.sh` 以 PostgreSQL fixture 验证 materialize、bootstrap、metadata、compile 和 mapping 转换的重新加载或稳定错误类别；账本为每项保留独立 asset。

### HTTP：4/4 in-scope（100%）

Ontop controller 源码给出 7 个跨进程路由组：`/sparql`、`/ontop/reformulate`、`/ontology`、`/ontop/restart`、`/ontop/portalConfig`、`/` 和 `/predefined/{id}`。对应文件为 `SparqlQueryController.java`、`ReformulateController.java`、`OntologyFetcherController.java`、`AutoRestartController.java`、`PortalConfigController.java`、`PortalController.java`、`PredefinedQueryController.java`。

`src/server.rs` 实现 `/sparql`（GET、form POST、raw SPARQL POST、Accept 协商和结果序列化）、开发模式 `/ontop/reformulate`、`/ontology` 与 `/predefined/{id}`。portal、portal config、restart 是 ADR-0001 的 Java UI/进程生命周期 excluded 项；`/healthz` 是 OCI 探针专用新增能力，不计入分母。

### SPARQL、OBDA/facts/OWL 与 OCI：可确认存在，暂无总体百分比

Ontop 基线中 `test/sparql-compliance` 有 107 个 manifest 与 1,365 个 `.rq`/`.sparql` 输入。`compatibility-report.json` 当前有 238 个 case，其中 198 个 scope 含 `query`；scope 可重叠、case 也没有记录完整 SPARQL manifest 条目 ID，因此 **198/1365 不成立**，不能报告为 SPARQL 迁移率。

rtop 的可执行实现/证据包括：

- `src/sparql.rs:18-202` 与 `src/lib.rs:103-225,740-1057`：SELECT、ASK、CONSTRUCT、DESCRIBE、图模式、modifier、aggregate 和表达式的已实现子集；
- `src/facts.rs:10-75`：Turtle、N-Quads（具名图）和 RDF/XML facts；`src/ontology.rs:13-130`：imports 闭包及最小 TBox 语义；
- `Dockerfile:9-15` 与 `scripts/test-oci-compat.sh:21-69`：默认 endpoint、CLI 覆写、healthcheck、无 Java 运行时、`password_file` 挂载。

未知/不应隐含为完成的边界包括完整 SPARQL 1.1（如 SERVICE、update、property paths、完整 graph-store/dataset 协议和全部函数）、完整 R2RML/OWL 2 QL 语义及其 manifest 的逐条等价。`binding/rdf4j`、`binding/owlapi`、`protege` 和 Java/JDBC 宿主 API 则是明确排除项，而不是“未迁移缺口”。

## 可复算命令

```sh
base=5ec07573b18513f33dfcd59ac45fe26a81f9cdbd
git -C ../ontop rev-parse "$base^{commit}"

git -C ../ontop ls-tree -r --name-only "$base" -- test/rdb2rdf-compliance/src/test/resources |
  rg '/r2rml[a-z]*\.ttl$' | wc -l
# 反向逐项匹配 64 个固定 r2rml*.ttl、所有 PostgreSQL manifest 分类和账本 hash。
./scripts/validate-coverage-ledger.sh

git -C ../ontop ls-tree -r --name-only "$base" -- test/sparql-compliance/src/test/resources |
  rg 'manifest.*\.ttl$' | wc -l
git -C ../ontop ls-tree -r --name-only "$base" -- test/sparql-compliance/src/test/resources |
  rg '\.(rq|sparql)$' | wc -l
jq '.cases | length' compatibility-report.json
jq -r '.cases[] | select(.scope | split("/") | index("query")) | .id' compatibility-report.json | wc -l

cargo test --all --locked
```

本工作树审计时，基线 R2RML 分母为 `64`，账本验证器确认 64 项均有 #40 passed source 证据；Docker PostgreSQL manifest 为 `171` 条 entry（`170` 条 passed、`1` 条 baseline-ignored）；SPARQL 发现集为 `107` 个 manifest、`1365` 个查询，报告为 `238` 个 case、其中 `198` 个 scope 含 query。最后的测试命令用于复核 rtop 本地证据；若要形成一个全局比例，仍须建立 Ontop SPARQL manifest/功能原子到 `compatibility-report.json` case 的一对一、带 `passed/deferred/excluded` 状态的映射。
