# 结构化差分证据重放描述

状态：accepted。

## 背景

覆盖账本曾把供人阅读的 `command` 同时作为机器校验字段。许多差分命令含有
`ONTOP_HOME=/tmp/rtop-ontop-runtime.<随机后缀>`；该路径是 harness 的运行时实现细节，
不改变固定 Ontop 基线、PostgreSQL 夹具、差分 case 或规范化结果。将其全文等值比较会
使已通过且可重放的证据因临时目录名变化而失效。

## 决策

带 Ontop 差分 runner 的账本条目必须包含 `replay` 对象：

- `runner`：版本控制的执行脚本路径；
- `cases`：一个或多个稳定 `DIFFERENTIAL_CASE` 标识；
- `parameters`：仅在语义上影响被测场景时记录的稳定参数。

`command` 保留完整的历史执行文本，供人工审计，但不再用作机器等值比较。临时目录、
PID、端口分配和其他运行时路径不得进入 `replay`。覆盖账本校验必须逐条报告 asset ID、
字段和期望值后失败关闭。

## 后果

这不会降低 ADR-0005--0007 要求的双边差分、固定夹具、provenance 或完整 RDF term
比较强度；它只把执行身份从易漂移的 shell 表示中分离出来。所有既有含临时 Ontop
工作目录的条目一次性迁移，新的或遗留的临时路径进入结构化字段都会使 gate 失败。
