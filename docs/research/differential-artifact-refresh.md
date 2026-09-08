# 差分 artifact 保鲜规程

## 目的

`docs/research/differential-artifacts/` 中的 passed 证据由两个独立进程的原始
输出和一个版本化 normalizer 共同构成。normalizer 内容变化时，不能仅把矩阵或
override 的哈希替换成新值；必须从已保存的原始输出重建规范化结果，或在同一固定
PostgreSQL 17 digest 下重新运行两端进程。

## 路由来源

`scripts/test-ontop-rtop-differential.sh` 是 comparison、case 名及专用参数的
唯一来源。使用下列只读模式提取路由；该模式不要求 `ONTOP_HOME`，也不会创建
容器、数据库或修改 artifact：

```sh
DIFFERENTIAL_DESCRIBE_ONLY=true DIFFERENTIAL_CASE=clisimplemapping \
  ./scripts/test-ontop-rtop-differential.sh
```

输出至少含有 `artifact_case_id`、`comparison`、`case_name`、`variant` 与
`student_kind`。动态 family 必须逐个传入 harness 已登记的白名单参数，例如
`LUBM_QUERY`、`SUITE_FILTER_QUERY`、`SUITE_DATATYPE_QUERY`、
`SUITE_MODIFIER_QUERY` 和 `SUITE_SIMPLECQ_QUERY`。不得以 artifact 目录名反推
comparison。

使用 `scripts/list-ontop-rtop-differential-routes.sh` 可一次性导出 JSONL 路由
清单。2026-09-08 审计时该工具导出 294 条唯一 artifact 路由；352 个已保存
provenance case 中有 58 个未映射到当前的单 case harness，其中 48 个被闭合矩阵
标为 passed。它们主要是复合 suite、专用 adapter 生命周期场景和旧 datatype
manifest 命名，必须先恢复其专用路由，不能以 `http-json` 或 `materialize` 猜测。

## 刷新不变量

一次可接受的刷新必须满足全部条件：

1. 在写入前验证 `ontop.raw`、`rtop.raw` 的 SHA-256 与 provenance 一致；原始
   输出不能被刷新步骤改写。
2. 用 describe 输出的 comparison、case_name、student_kind 调用
   `normalize-ontop-rtop-differential.sh`。未知 comparison 必须失败，不能落入
   其他 fixture 的解析路径。
3. normalizer 必须确认两端结果一致；否则保留失败指纹，不得提升为 passed。
4. 仅在所有目标 artifact 都成功后，更新各 provenance 的 normalized SHA-256、
   matrix atom 和 override 中的 normalizer SHA-256；随后重新生成 matrix。
5. 最后执行 `validate-three-layer-closure-matrix.sh --require-closed`。这个 gate
   是通过结论的唯一证据，结构校验通过不足以关闭 #106。

## 当前审计结论

历史 artifact 存在三个 normalizer 哈希，而仓库只能取得当前脚本内容。因此先
恢复并显式化现有路由，再逐 artifact 重算；不能将旧哈希直接替换为当前哈希。
`query` 路由已经从 normalizer 的无条件兜底迁移为显式分支，未知 comparison 会
以 exit 64 拒绝，避免把错误类别当作 CSV 查询证据。

在最新临时副本演练中，294 条已解析路由对应 282 个结果与已有 normalized.json
逐字相同，24 个仍由 harness 的专用 rejection、
HTTP 或并发分支处理而失败。N-Quads 已统一为逐 triple 元素；SPARQL Results JSON
则在递归 canonical 后排序，避免对象字段插入顺序影响 bag 的排序键。

实际刷新已使用当前 normalizer
ad0089f6de3128854c47d1b226eaf61aae948f022f98cda07553f3c09cac2d73：282 个
artifact 被重新验证；所有 artifact 的两端 raw SHA-256 均先与 provenance 核对，
raw 文件没有改写。override 更新并重新生成矩阵后，有 269 个 passed 原子使用当前
normalizer hash。严格闭合 gate 仍因 62 个旧身份原子失败，
因而这不是 #106 的关闭结论。
