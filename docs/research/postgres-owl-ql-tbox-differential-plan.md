# OWL 2 QL TBox 双边差分缺口

`postgres-owl-ql-tbox-closure-and-limit-endpoint` 需要单独证明四个 PostgreSQL
endpoint 可观察结果：`equivalentClass`、`equivalentProperty`、无 filler 的
`exists R.Thing` 归一化，以及带 `Professor` filler 的 left existential 不产生蕴含。

已有 imports、subproperty/inverse 与 domain/range artifact 只覆盖相邻行为，不能代替
这四项。闭合时应以 `mapping-algebra-bag.obda`、`algebra-init.sql`、
`imports-{root,child}.ttl` 和四个 `tbox-*.rq` 在相同 Ontop/rtop PostgreSQL endpoint
上运行，并将四份原始结果聚合为一个 provenance。
