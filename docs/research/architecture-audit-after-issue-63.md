# 第 10 张迁移票据后的架构审计

日期：2026-09-06。触发条件：关闭 #54 至 #63 共十张 PostgreSQL 替换内核票据。

## 结论

实施一项重要改进：将 ontology 输入的相对路径、`file://`、HTTPS URL 和 XML Catalog
重定向解析收敛为 `ontology::resolve_input` 这个深模块。配置模块现在只读取声明值，
不再了解 URL 是否须经 Catalog 映射；imports closure、循环去重与远程拒绝仍位于同一
ontology module 的 interface 之后。

这提高了 Catalog seam 的局部性：将来扩展受控文档来源时，无须同时修改配置和本体加载器。
该改进是重要的，因为此前两个位置共同决定了同一网络隔离 invariant。

未实施项：不为当前唯一 PostgreSQL adapter 引入新的 adapter seam；没有第二个真实
实现可证明该额外 interface 带来 leverage，反而会扩大调用方必须了解的表面积。

## 验证

运行 `cargo fmt --check`、`cargo test --all --locked`、
`./scripts/test-delivery-compat.sh`、`./scripts/test-postgres-compat.sh`、
`./scripts/validate-coverage-ledger.sh`、JSON 校验与 `git diff --check`。其中交付
endpoint gate 覆盖 XML Catalog、imports closure、等价闭包和 OWL 2 QL 限制行为。
