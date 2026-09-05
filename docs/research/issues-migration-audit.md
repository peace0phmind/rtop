# GitHub Issue 与 PostgreSQL 迁移实现审计

关联 Issue：[ #35：PostgreSQL 17 限定的可观察 VKG 功能等价与证据闭环](https://github.com/peace0phmind/rtop/issues/35)。
远端最终 gate 证据见已关闭的 [#50](https://github.com/peace0phmind/rtop/issues/50)。

审计日期：2026-09-05。范围是 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`
中可在 PostgreSQL 上观察的能力；不把 Java 宿主 API 或其它数据库方言计入完成比例。

## 结论

GitHub 共 52 个 Issue：49 个已关闭、3 个开启。关闭状态不能直接代表迁移完成：
`#2–#7` 是研究/范围决策，`#9–#39` 是早期实现分片；当前可复验的完成依据应是
严格账本及最终 gate。当前工作树执行 `./scripts/test-final-gate.sh` 成功：

- 基线发现：18 个直接 PostgreSQL 类、10 个注解类、27 个 RDB2RDF manifest、170 个范围内
  Docker manifest query；1 个基线明确忽略项。
- 严格覆盖账本：310 条 in-scope 条目有效；`compatibility-report.json` 当前有 237 条
  `passed`、1 条 `baseline-ignored`（它不是完整分母，完整分母在 coverage ledger）。
- Rust 回归：`cargo test` 为 142 个测试通过（18 unit + 2 CLI + 3 config + 4 facts +
  7 adapter + 108 runtime）；有 7 个 dead-code warning，不影响本次通过结论。

因此，现有实现对已入账 PostgreSQL 资产具有通过证据。`#40–#52`（不含父规格 #35）
均已按严格账本和最终 gate 证据关闭；#43 的四个入口已重新逐项核验，#50 的 GitHub Actions
远端 artifact 已成功取得。

## 证据方法与限制

一手证据为：

- GitHub Issues 的正文和评论，使用 `gh issue list --state all --json ...` 读取；链接格式为
  `https://github.com/peace0phmind/rtop/issues/<编号>`。
- 固定 Ontop 工作树的路径、当前 `compatibility-report.json`、
  `docs/research/postgres-only-coverage-ledger.json`、`scripts/test-final-gate.sh` 与 Rust 源/测试。
- 本次本地实际执行的 `cargo test` 与 `./scripts/test-final-gate.sh`。

该结论覆盖当前工作树的本地 PostgreSQL Docker 验收以及 GitHub Actions 默认分支的成功
artifact；它不把无 PostgreSQL 前提的 Java API 当作迁移遗漏。

## 开启 Issue 审计

| Issue | 当前判断 | 当前代码/测试证据 | 遗留工作或建议 |
| --- | --- | --- | --- |
| [#1](https://github.com/peace0phmind/rtop/issues/1) | 应保持开启 | 总路线图，不是单一可验收实现。 | 用 #35 的 PostgreSQL 完成边界回写路线图。 |
| [#8](https://github.com/peace0phmind/rtop/issues/8) | 应保持开启/需缩范围 | 原规格还含 PostgreSQL 之外的首期目标。 | 明确拆出非 PostgreSQL 工作，避免以 PG gate 暗示总规格完成。 |
| [#35](https://github.com/peace0phmind/rtop/issues/35) | PostgreSQL 限定验收已完成，仍保持开启 | final gate 的 310 strict 条目与 #50 远端 CI artifact。 | 作为 #8 的范围/发布规格保留；未来仅在范围或认证版本变化时更新。 |

## 已关闭 Issue 复核

| 分组 | Issue | 判断 |
| --- | --- | --- |
| 研究与边界 | [#2–#7](https://github.com/peace0phmind/rtop/issues/2) | 合理关闭；它们产出基线、领域边界、输入/查询/交付取证，不应作为实现通过计数。 |
| 首轮 PostgreSQL 垂直切片 | [#9–#13](https://github.com/peace0phmind/rtop/issues/9) | 后续源、测试和 final gate 仍证实核心 mapping、facts、查询、adapter、CLI/HTTP/OCI 已有实现；合理关闭。 |
| 早期缺口收敛 | [#14–#34](https://github.com/peace0phmind/rtop/issues/14) | 其 datatype、identifier、expression、aggregate、IMDB、annotation、nested、GeoSPARQL、metadata 等验收后来被 #35 的账本与 #40–#52 的原子测试重新约束。合理关闭，但不能单凭关闭状态宣称全覆盖。 |
| PostgreSQL 账本与查询分母 | [#36–#39](https://github.com/peace0phmind/rtop/issues/36) | 合理关闭：当前 final gate 仍实际执行 discovery、method audit、strict coverage validation。后续 Issue 正文中的 `Blocked by #36/#37` 已过时，应清理。 |
| PostgreSQL 功能与交付闭环 | [#40–#52](https://github.com/peace0phmind/rtop/issues/40) | 均已关闭；其中 #50 在默认分支的 [Actions run 33969080023](https://github.com/peace0phmind/rtop/actions/runs/33969080023) 成功并保存 provenance artifact。 |

## 待办清单（按优先级）

1. 保持 #35 的 PostgreSQL 17 限定结论与 #50 的远端 artifact 链接；任何基线、镜像 digest、Cargo.lock 或 Dockerfile 漂移均应重新认证。
2. 将 #8 与 #1 明确为路线图，拆开 PostgreSQL 已完成范围和其它数据库/Java 宿主范围。
3. 处理 `src/sparql.rs` 的未使用 AST 辅助函数/`BindingValue` warning，避免“已有 UNION/subquery 功能”与死代码产生审计歧义；这不是本次 gate 失败项。
