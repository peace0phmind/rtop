# PostgreSQL R2RML 发现分类

固定基线的 27 个 RDB2RDF manifest 中有 21 个包含 R2RML entry（D000–D016、D018–D020、
D026）；它们都是 `in-scope`：通过 PostgreSQL logical table 或 SQL query 观察 mapping
的可见结果，不能因目前缺少 Rust fixture 而排除。
`discover-postgres-baseline-assets.sh` 的 `r2rml_manifest_assets` 逐项输出其稳定 asset ID
与源路径。

当前 `compatibility-report.json` 已关联全部 21 个 R2RML 目录：D000–D016、D018–D020、
D026。Direct Mapping 是独立交叉集合：24 个 manifest 含 26 条 DirectMapping entry，
其中 D001、D004 等同时也含 R2RML entry；它们全部转由 #51 跟踪，仍为 in-scope，
绝非排除项。

后续严格账本把每个 manifest 再展开为其 mapping test entry；本文件只冻结发现与
适用性分类，避免 R2RML 目录被 Rust 已有测试数吞没。
