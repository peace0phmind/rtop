# FIBO 固定兼容夹具

这些文本夹具由 `intent-engine-ontop@ee2c837c06f3b8babe8b7fbe43944fa1f58add50`
的 `artifacts/fibo/` 只读复制而来，已纳入 rtop 版本控制。它们只用于 rtop 的
PostgreSQL HTTP 兼容验收；测试和运行时不读取相邻工作树，也不要求检出该项目。

每个文件的 SHA-256 已登记在 PostgreSQL coverage ledger 中；scale facts 的
SHA-256 同时由 `scale/generated/manifest.json` 固定。
