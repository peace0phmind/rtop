# Ontop 对照层证据与 LLVM 覆盖率复核

调查日期：2026-09-07。对照工作树固定为
`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`；该树当前有未跟踪文件，未作任何修改。

## 先给结论

1. **没有、也不应把本项目描述成 Java 与 Rust 的逐行/逐函数“功能代码比对”。** 两者实现语言、
   分层和宿主 API 都不同。实际采取的是行为对照：对固定 Ontop 源测试/manifest/fixture，分别
   启动 Ontop Java 17 CLI（或 endpoint）与 rtop Rust CLI（或 endpoint），在同一 PostgreSQL 17
   digest 上比较规范化输出、错误或拒绝类别。`src/mapping.rs` 到 Ontop Java parser 的对应是由
   资产和可观察行为建立的，不是 AST 或源码文本 diff。
2. **R2RML 不能概称为“整个 R2RML 标准层已完整实现”。** 可以严谨地说：固定 Ontop 基线中
   `test/rdb2rdf-compliance/src/test/resources/**/r2rml*.ttl` 的 **64 个 mapping 文件**，都已有
   #40 的 `passed` 账本证据；校验器逐文件枚举并强制每个文件被 source path 引用。
   这代表该基线的 PostgreSQL 可观察子集逐文件验收，并非 W3C R2RML 规范的无限输入空间，更不
   覆盖其它数据库方言或 Ontop 的 Java API。
3. 基线 Java 测试**不是机械移植**成同名 Rust 测试。项目复用了大量原始 mapping、SQL、SPARQL、
   N-Quads 预期结果和 manifest；Rust 单测还手写了紧凑回归例。Java 测试中属于 RDF4J/OWLAPI、
   JUnit harness 或 JVM CLI 内部对象的断言，按 #35 范围不迁移；但它们的 PostgreSQL 外部行为
   应由双边差分或服务端测试覆盖。
4. `cargo llvm-cov --all --locked --summary-only` 实测行覆盖 **61.80%**、函数 **56.75%**。
   它低的主要原因不是 R2RML parser（`mapping.rs` 行 **71.83%**），而是同一命令只运行 Rust
   test harness，**不运行** Docker/CLI/HTTP 的 `test-final-gate.sh`。因此 datasource **12.15%**、
   main **25.31%**、server **26.24%** 把总数拉低；这些模块的重要集成路径虽由 shell gate 覆盖，
   却没有进入该次 LLVM 插桩的计数。分支列为 0，表示该工具链本次没有产生可统计分支分母，
   不是“分支 0%”。

## R2RML：证据强度与边界

### 已实际完成的对照

`scripts/validate-coverage-ledger.sh` 第 451--471 行从固定 commit 枚举 64 个 `r2rml*.ttl`，
并要求每个文件在 #40、`passed` ledger entry 的 `source_path` 中出现；本次运行该校验通过。
这比“目录出现过”或“写了若干等价 fixture”强得多。

对应的运行入口是：

- `scripts/test-postgres-compat.sh`：以原始 `create.sql`、mapping 和 N-Quads 做 PostgreSQL 17
  CLI 图比较或明确错误类别检查；覆盖 subject/object/predicate/graph term map、class、多
  POM/PredicateMap、logical table/SQL query、blank node、datatype/language、相对 IRI、模板
  escaping/percent encoding、RefObjectMap（无 join、单/复合 join、二级 join）等。
- `scripts/test-ontop-rtop-differential.sh`：以独立 Ontop Java 17 container 和 rtop 进程做
  双边对照。例：`d002a` 的 `r2rmla.ttl`（多 POM + class）比较 materialized N-Quads，
  `d014` RefObjectMap 则比较完整 dataset；相应 provenance 位于
  `docs/research/differential-artifacts/r2rml-d002-multiple-pom-class/` 和
  `docs/research/differential-artifacts/r2rml-d014-ref-object-map-and-inline-terms/`，均记录固定
  Ontop commit、PostgreSQL image digest、两端原始输出 hash、规范化输出 hash 与 `passed`。
- `docs/research/three-layer-closure-matrix.json` 当前有 333 个原子，全部 `passed`；其中
  mapping 117、ontology 28、sparql 188。结构校验脚本会验证每个 `passed` 原子有真实的
  Ontop/rtop raw 输出、hash 和覆盖 case ID。

### 为什么仍不能说“R2RML 整层等价”

`src/mapping.rs` 的 R2RML parser 明确实现的是所需子集：例如只接受映射执行需要的
`rr:logicalTable`、subject/predicate/object/graph map、term type、language/datatype、
RefObjectMap/join condition 等结构；错误信息也明确有“不支持的 termType”分支。它没有把
W3C 语义空间逐项穷尽的声明。D016 的 `BYTEA` fixture 更是 PostgreSQL 适配，尽管其双边
provenance 已显示同一适配下两端结果一致。

更重要的是 ADR `docs/adr/0005-three-layer-equivalence-evidence.md` 规定闭合不仅要差分
artifact，还要 `mapping.rs` 等三层模块行覆盖至少 90%、分支至少 80%。Issue #104 仍开放，
所以当前矩阵 333 passed 是“每个已登记原子有双边通过证据”，不是可以越过覆盖门槛的
“层完整证明”。

## Ontop Java 测试的对应关系

固定基线包含至少以下 R2RML 专门 Java 测试。更完整地说，`RDB2RDFTest.java` 是参数化
端到端 runner：27 份 manifest 中 21 份含 R2RML、共有 69 条 R2RML entry；资源本身有 64 个
`r2rml*.ttl`。该 runner 的循环只到 D025，因此 D026 是 rtop 固定资产账本的扩展项，不能倒过来
声称其由该 Java runner 覆盖。以内容搜索，固定 Ontop 基线有 46 个 Java 测试文件提到 R2RML，
还包括其它数据库方言的 Docker 测试；这些不都属于 rtop 的 PostgreSQL 替换范围。

| Ontop Java 一手测试 | rtop 的对应方式 | 是否逐字移植 |
| --- | --- | --- |
| `test/rdb2rdf-compliance/src/test/java/RDB2RDFTest.java` | 复用它读取的 manifest、SQL、mapping 和期望 RDF；`test-postgres-compat.sh`/双边 differential 逐资产执行。 | 否 |
| `mapping/sql/all/.../BasicR2RMLMappingMistakeTest.java` | D002/D007/D012/D015 等原始非法 mapping 的加载/执行拒绝及错误类别；双边 differential。 | 否 |
| `mapping/sql/all/.../R2RMLConversionTest.java` | `src/mapping.rs` 的 `to_r2rml`/R2RML reader；`tests/cli.rs` 和 `scripts/test-delivery-compat.sh` 的 pretty、to-obda、to-r2rml、reload。 | 否 |
| `client/cli/.../OntopR2RMLPrettyfyTest.java`、`OntopOBDAToR2RMLTest.java`、`OntopR2RMLToOBDATest.java` | Rust CLI 的 mapping 子命令和 `test-ontop-rtop-differential.sh` 的 CLI shared-input suite。 | 否 |
| `binding/rdf4j/.../RDF4JR2RMLConstantLangTagTest.java`、`RDF4JR2RMLBnodeProfTest.java` | PostgreSQL 外部 RDF term 行为（language、blank node）由 D005/D012/D015 等 mapping/图对照覆盖。RDF4J repository 对象 API 本身范围外。 | 否 |
| `core/model/.../R2RMLIRISafeEncoderTest.java` | `tests/runtime.rs` 与 D010/D019/D020 的模板/IRI/percent encoding 行为；双边 artifact。 | 否 |

换言之，“没有对应 Java 用例”不正确：有大量**引用原始 Java 测试资产**和双边执行的对应；
“每一个 JUnit 方法都存在一个同名 Rust `#[test]`”则不成立，也不该作为跨语言替换的验收口径。

## 低 LLVM 覆盖率的具体归因

| 文件 | 行覆盖 | 主要原因 |
| --- | ---: | --- |
| `src/mapping.rs` | 71.83% | Rust runtime 单测（117 项）和大量 R2RML 资产已打到解析/执行主路径，但仍有 serializer、格式/错误组合、Direct Mapping 等未打到分支。它不是总分低的主因。 |
| `src/sparql.rs` | 78.42% | 解析和运行时例子较多，剩余为语法/错误及边界组合。 |
| `src/ontology.rs` | 83.54% | 已接近门槛但尚未达到 ADR 的 90%。 |
| `src/datasource.rs` | 12.15% | PostgreSQL adapter 的关键测试主要通过 Docker/shell gate 或非 LLVM `cargo test` 路径；大量连接、取消、stream、metadata、错误恢复函数未被该一次插桩测量。 |
| `src/main.rs` | 25.31% | CLI 集成测试和交付脚本以子进程 `cargo run` 运行，未由 `cargo llvm-cov --all` 收集。 |
| `src/server.rs` | 26.24% | HTTP endpoint 由 delivery/compat shell 测试启动，未纳入该 LLVM test profile。 |
| `src/config.rs` | 57.66% | 配置解析的错误/兼容组合覆盖较少。 |

本次 `llvm-cov` 统计总数为 9,976 行中 3,811 行未执行、955 函数中 413 未执行。因而 174/174
`cargo test` 和 333/333 兼容原子通过不能替代代码覆盖率；前两者回答“登记行为是否通过”，
后者回答“插桩执行了多少实现路径”。

## 复核命令与一手来源

```sh
cargo llvm-cov --all --locked --summary-only
./scripts/validate-coverage-ledger.sh
./scripts/validate-three-layer-closure-matrix.sh
jq '.atoms | group_by(.layer) | map({layer: .[0].layer, count: length})' \
  docs/research/three-layer-closure-matrix.json
```

Issue 一手范围/门槛：#35（PostgreSQL 外部可观察 seam）、#40（R2RML 分母与逐资产证据）、
#104（差分与 90% 行/80% 分支门槛）。按仓库规则可用 `gh issue view 35 --comments`、
`gh issue view 40 --comments`、`gh issue view 104 --comments` 获取完整正文和评论。
