# 已交付模块的逐文件覆盖率门槛

状态：accepted。

rtop 不只交付替换内核，也交付 PostgreSQL adapter、CLI、HTTP 与配置；只对 `ontology`、`mapping`、`sparql`、`lib` 设置总覆盖率会掩盖未经过真实路径的交付模块。因此对所有 `src/*.rs` 已交付模块采用逐文件 LLVM 行覆盖率不低于 90%、函数覆盖率不低于 85% 的 gate，并在同一插桩 profile 中运行 Rust 测试、PostgreSQL Docker、CLI 与 HTTP 情景。门槛由版本化的统一 runner 产生并接入最终 gate；任一子进程未插桩、profile 无法合并或报告缺失都必须失败关闭。Ontop 语义原子仍必须遵守 ADR-0005--0007 的闭合矩阵和独立差分要求；覆盖率不能替代该证据。ADR-0005 的三层分支覆盖率至少 80% 仍是关闭门槛；Rust branch coverage 当前为不稳定且无可靠分母的工具能力时，必须将该门槛报告为外部工具阻断，不能伪造百分比或将三层等价宣告为完成。
