# PostgreSQL 迁移变更—Issue 对照

本清单对应固定基线 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`，用于
本次提交前把工作树中的全部已跟踪和未跟踪迁移资产关联至 GitHub Issue。路径规则覆盖
目录下每个文件；重叠的运行时文件由多个功能 Issue 共同使用，列为“共享实现”。

| Issue | 变更路径 | 作用 |
| --- | --- | --- |
| #35 | `CONTEXT.md`、`docs/adr/**`、`docs/research/postgres-only-*`、`docs/research/postgres-baseline-*`、`docs/research/postgres-*-audit.md`、`compatibility-report.json` | PostgreSQL 限定规格、发现、方法审计、覆盖账本和结果证据。 |
| #40 | `tests/compat/postgres-d{000..026}/**`、`src/mapping.rs`、`src/lib.rs`、`src/sparql.rs`、`tests/runtime.rs` | 64 个 R2RML mapping 与对应 PostgreSQL 查询/错误边界。 |
| #41 | `tests/compat/postgres-native-obda/**`、`src/mapping.rs`、`src/main.rs`、`tests/runtime.rs` | Native OBDA 加载、SQL source 和诊断。 |
| #42 | `tests/compat/postgres-facts/**`、`src/facts.rs`、`src/lib.rs`、`tests/facts.rs`、`tests/runtime.rs` | RDF facts 与映射结果联合查询。 |
| #43 | `tests/compat/postgres-ontology/**`、`src/ontology.rs`、`src/lib.rs`、`tests/runtime.rs` | imports、TBox 和 PostgreSQL 改写。 |
| #44 | `tests/compat/postgres-datatype-manifest/**`、`tests/compat/postgres-identifiers/**`、`tests/compat/postgres-nested/**`、`tests/compat/postgres-metamapping/**`、`tests/compat/postgres-cast/**`、`src/config.rs`、`src/datasource.rs`、`src/model.rs` | PostgreSQL 类型、标识符、JSON/array 与复杂值。 |
| #45 | `tests/postgres_adapter.rs`、`src/datasource.rs`、`src/lib.rs` | PostgreSQL 取消、流式提前停止与连接复用。 |
| #46 | `tests/compat/postgres-d{000..026}/**`、`src/main.rs`、`src/mapping.rs`、`tests/runtime.rs` | R2RML/OBDA 映射 CLI 互操作。 |
| #47 | `scripts/test-delivery-compat.sh`、`src/main.rs`、`src/config.rs` | 非元 CLI 工作流。 |
| #48 | `tests/compat/postgres-http/**`、`src/server.rs`、`src/main.rs` | SPARQL、ontology、predefined 和 development HTTP 契约。 |
| #49 | `Dockerfile`、`.dockerignore`、`scripts/test-oci-compat.sh`、`Cargo.toml`、`Cargo.lock` | 无 JVM OCI 交付、endpoint、healthcheck 与 secret-file。 |
| #50 | `.github/workflows/postgres-final-gate.yml`、`scripts/test-final-gate.sh`、`scripts/collect-final-gate-provenance.sh`、`scripts/discover-postgres-baseline-assets.sh`、`scripts/validate-*`、`compatibility-report.json`、`docs/research/postgres-only-coverage-ledger.json` | 固定基线发现、最终 gate、hash/provenance 和 CI artifact。 |
| #51 | `tests/compat/postgres-direct-d{000..025}/**`、`src/lib.rs`、`src/mapping.rs`、`tests/runtime.rs` | 26 个 Direct Mapping manifest output。 |
| #52 | `tests/compat/postgres-lubm/**`、`scripts/test-postgres-lubm-compat.sh`、`src/sparql.rs`、`src/ontology.rs`、`src/lib.rs` | 14 条 LUBM PostgreSQL manifest query 与可复现 fixture。 |
| 已有 #36 对照项 | `tests/compat/postgres-suite/**`、`scripts/test-postgres-suite-compat.sh`、`scripts/test-postgres-compat.sh`、`tests/compat/postgres-imdb/**`、`tests/compat/postgres-annotation/**`、`tests/compat/postgres-aggregates/**`、`tests/compat/postgres-expressions/**`、`tests/compat/postgres-geospatial/**`、`tests/compat/postgres-order-by/**`、`tests/compat/postgres-query-kinds/**`、`tests/compat/postgres-constraints/**` | DockerPostgresTestSuite、轻量 PostgreSQL class 及其继承方法语料。 |

共享实现文件 `src/lib.rs`、`src/mapping.rs`、`src/sparql.rs`、`src/main.rs`、
`tests/runtime.rs` 和 `scripts/test-postgres-compat.sh` 的分段变更分别由上表中的原子
fixture和覆盖账本定位；它们不应被误解为无 Issue 归属的杂项改动。
