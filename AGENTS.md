# Agent Instructions

使用中文进行交互和文档书写。

## Agent skills

### Issue tracker

Issues and specs are tracked in this repository's GitHub Issues. See `docs/agents/issue-tracker.md`.

### Triage labels

Use the default five canonical triage labels. See `docs/agents/triage-labels.md`.

### Domain docs

This repository uses a single-context domain-document layout. See `docs/agents/domain.md`.

## Ontop 对照基线

- 参考仓库位于 `../ontop`，基线提交固定为 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`（`version5`）。迁移、取证或兼容性验收时从此工作树读取源码与测试资产。
- `../ontop` 是只读对照；其中存在用户未跟踪文件，禁止修改、清理或纳入 `rtop` 提交。
- R2RML、SPARQL compliance、OWL、PostgreSQL 与 Docker 对照资产都在该工作树中。每个支持情景须将来源、环境与结果写入 `compatibility-report.json`。
- OCI gate 使用 `docker build` 验证无 JVM 镜像、默认 endpoint 与 healthcheck；若基础镜像 registry 不可用，记录外部阻断，不得把镜像 gate 标为通过。
