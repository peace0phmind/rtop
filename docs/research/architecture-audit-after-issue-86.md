# 第 20 张迁移票据后的架构审计

日期：2026-09-06。触发条件：自 #63 后关闭 #64、#67、#71、#74、#77、#79、#81、
#82、#83、#86 共十张 PostgreSQL 替换内核票据。

## 结论

实施一项重要改进：将服务器由 `LoadedConfiguration` 与 `PostgresDataSource` 构造
`VkgRuntime` 的分支收敛为 `server::runtime_from_loaded` 深模块。它的 interface 只有
已加载配置与 PostgreSQL adapter，内部处理普通 mapping 与 Direct Mapping 的选择。
`/sparql`（包括可取消执行）、预定义查询和 `/ontop/reformulate` 现在均经过此 seam。

改进前，三个入口各自实现同一个 Direct Mapping 分支；变更映射构造时会要求维护者同时
理解并同步多个调用点，缺少 locality。改进后，调用方只关心自身的查询、取消或诊断职责；
映射选择的 implementation 集中在一个模块后面，获得更高的 depth 和可验证的一致性。

未实施项：不引入第二个数据源 adapter 或抽象连接池 seam。当前只有 PostgreSQL 这一种
真实 adapter，额外 interface 没有第二个实现证明其 leverage，反而会扩大调用方必须知道的
错误、取消和类型转换语义；这不符合 PostgreSQL 限定的 ADR。

## 验证

运行 `cargo fmt --check`、`cargo test --all --locked`、
`./scripts/test-postgres-compat.sh`、`./scripts/test-delivery-compat.sh`、
`./scripts/validate-coverage-ledger.sh`、两份 JSON 的 `jq empty` 与 `git diff --check`。
全部通过；测试输出的既有 dead-code warning 未造成失败。交付 gate 覆盖 HTTP SPARQL
endpoint、Direct Mapping、预定义查询、取消清理和主 FIBO 全 RDF term 对照。
