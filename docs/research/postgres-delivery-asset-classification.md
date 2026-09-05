# PostgreSQL 限定交付资产分类

本文件是 #37 发现集的交付面分类，来源固定为
`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。每项的 Rust 验收入口必须是
CLI、HTTP 或 OCI，不能以 AirLine、Spring、RDF4J 或 JDBC 类型替代。

| 资产 | 分类 | 理由 |
| --- | --- | --- |
| `query`、`materialize`、`bootstrap`、`validate`、`endpoint`、`extract-db-metadata`、`compile` | in-scope | 11 个非元 CLI 任务中的 PostgreSQL 可观察工作流；均须在后续账本逐项转为 passed。 |
| `to-r2rml`、`to-obda`、`pretty-r2rml`、`v1-to-v3` | in-scope | 同属 `OntopMappingOntologyRelatedCommand`，但必须作为四项独立 CLI asset 验收，不能由该 Java 基类聚合结案。 |
| `/sparql`、`/ontop/reformulate`、`/ontology`、`/predefined/{id}` | in-scope | 是可由 HTTP 客户端观察的协议；`/ontology` 和 predefined 的实现范围采用 PostgreSQL 后续计划中的默认决定。 |
| `/` portal、`/ontop/portalConfig`、`/ontop/restart` | excluded | 分别依赖静态 UI/TOML portal 或 Spring 进程内重启；ADR-0001 允许排除，原因不是实现难度。 |
| `help`、`version`、bash completion | excluded | Java AirLine/UI 可用性表面，不是 PostgreSQL VKG 或保留的 11 个非元任务；原生 Rust CLI 可独立提供等价帮助。 |

## 映射互操作命令的固定基线原子（#46）

以下四项不可因命令入口存在而标为通过；来源均为
`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 的实际实现与 CLI 测试。

| 命令 | 基线来源 | 必须保留的外部行为 | Rust 验收门槛 |
| --- | --- | --- | --- |
| `mapping to-obda` | `OntopR2RMLToOBDA.java`、`OntopR2RMLToOBDATest` | Turtle R2RML 输入被序列化为 native `.obda` | 输出必须由 Rust runtime 在 PostgreSQL fixture 重新加载并查询；不得只复制 TTL 或接受 JDBC dummy 配置。 |
| `mapping to-r2rml` | `OntopOBDAToR2RML.java`、`OntopOBDAToR2RMLTest` | native `.obda` 输出 Turtle R2RML；默认要求数据库 metadata，`--force` 才允许跳过 | 输出必须被 Rust R2RML loader 重新加载，metadata/force 的错误类别各有独立 case。 |
| `mapping pretty-r2rml` | `OntopR2RMLPrettify.java` | 解析 Turtle，使用内联 blank node 的规范化 Turtle 输出 | 输出不是输入文件的字节拷贝；须能重新加载并保留完整 RDF terms。 |
| `mapping v1-to-v3` | `OntopMappingV1ToV3.java`、`OntopMappingV1ToV3Test` | 对 native mapping 提取旧 source connection 声明；重写全限定 placeholder 对应的 SQL alias；R2RML 还会 canonicalize/pretty-print | 必须以独立 fixture 检查 alias、source 声明迁移或拒绝、overwrite/output 行为；JDBC properties 不能进入 rtop 运行时配置。 |

较早的交付接口审计曾将 `/ontology` 和 `/predefined/{id}` 写为首期不交付；这与
`postgres-only-remaining-function-plan.md` 的已确认默认决定冲突。本分类以较新的
PostgreSQL 限定计划为准，并要求 #48 给出真实 PostgreSQL HTTP 证据后才可标为 passed。
