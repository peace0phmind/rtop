# Ontology imports/catalog 双边差分最小计划

矩阵原子 `postgres-ontology-imports-url-catalog-endpoint` 不能仅复用
`postgres-ontology-imported-subclass-rewrite-endpoint`：后者证明相对文件 import
closure，却不证明顶层 HTTPS IRI 与 XML Catalog 的精确重定向。

闭合所需的单一双边 endpoint 场景应固定使用：

- `tests/compat/postgres-http/imports-catalog.xml`；
- `imports-root.ttl` 与 `imports-child.ttl` 的 root → child → root 环；
- `mapping-algebra-bag.obda` 和 `algebra-init.sql`；
- `imports-catalog.rq`，断言 Parent 查询保留三个 person IRI binding。

运行时必须将相同 catalog 对 Ontop 的 `--xml-catalog` 和 rtop 的 `xml_catalog`
配置入口生效后再启动两个 PostgreSQL endpoint。artifact 应同时记录顶层 URL、catalog
映射、环去重以及 query binding；未映射 HTTP(S) import 的无网络拒绝另由 Rust 单测
回归，不得把它替代成功的双边 URL/catalog 证据。
