## 问题陈述

`intent-engine-ontop` 需要以 rtop 替换其 Ontop 服务，同时不修改该项目的 Python 源代码、SPARQL HTTP 调用和结果消费逻辑。替换不能只让现有七个 endpoint 的少量查询返回结果：它必须覆盖这些调用穿过的 Ontop PostgreSQL 替换内核，即 SPARQL HTTP、native OBDA/R2RML 映射、OWL 2 QL 本体装载和改写、SPARQL 代数、PostgreSQL SQL 改写与优化、结果协议、取消和资源行为。

当前 rtop 已有 PostgreSQL adapter、部分映射/本体/SPARQL 骨架和 JSON endpoint，但其查询执行仍逐三元组读取并在内存 join，尚不具备 Ontop 的整条 SQL 改写与约束优化链。已发现的直接缺口包括 `IN`/`NOT IN` 等 SPARQL 代数、完整函数和 decimal 语义、HTTP 结果格式/dataset 参数、imports closure/XML Catalog/URL 本体、并发请求隔离，以及 PostgreSQL JSON/array/PostGIS/metadata 等语义。将“请求成功”或“现有业务样例通过”当作完成，会遗漏可观察错误、空值、多重性、语义反例和资源行为。

本规格扩展并引用 #35 的 PostgreSQL 可观察等价与证据闭环，不改写其历史认证结论；它定义 `intent-engine-ontop` 替换所要求的完整 PostgreSQL endpoint 内核与新的完成门槛。

## 方案

以一个最高层的 PostgreSQL SPARQL endpoint compatibility ledger 为唯一主 seam。该 seam 在 rtop 原生部署配置下加载 Ontop endpoint 可接受的 native OBDA 或 R2RML 映射、OWL 2 QL 本体及 imports closure，并以 PostgreSQL 执行 SPARQL 请求；它观测 HTTP 输入/输出、RDF term、bag/order、错误、取消和资源释放。所有底层 parser、mapping、ontology、reformulation、optimizer 和 adapter 测试均服务于此 seam，不能另行定义不一致语义。

固定 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 为可迁移功能上限；以 `intent-engine-ontop` 已锁定 Ontop 5.1.0 镜像在七个 endpoint 情景中的结果为集成验收锚点。二者不同必须形成版本差异证据，不得静默选择。使用方源代码与 HTTP 消费不改；Compose、环境变量、挂载和连接配置可迁移为 rtop 原生形式。

## 用户故事

1. 作为 `intent-engine-ontop` 使用者，我希望不修改 Python 源代码即可把 Ontop endpoint 换为 rtop，以便保留既有业务调用和结果处理。
2. 作为部署者，我希望将 Ontop 环境变量、JDBC properties 与容器挂载迁移为 rtop 原生配置，以便新 Rust 项目不背负 Java 配置兼容层。
3. 作为 SPARQL 客户端，我希望以 form POST、GET 或 SPARQL query body 调用 endpoint 并获得兼容响应，以便不同标准客户端可继续工作。
4. 作为 SPARQL 客户端，我希望获得 JSON、XML、CSV、TSV 绑定结果及 Ontop 可观察的图结果格式，以便 content negotiation 不会改变客户端行为。
5. 作为 SPARQL 客户端，我希望 default/named graph 参数、dataset 子句和图查询遵循基线语义，以便多图查询不被静默降级。
6. 作为映射作者，我希望 endpoint 接受 native OBDA 和 R2RML，以便 Ontop endpoint 的映射输入面完整可替换。
7. 作为映射作者，我希望 IRI template、blank node、graph、join condition、NULL、datatype、language、base IRI 与复杂 source SQL 的结果可对照，以便映射更换不损失 RDF 语义。
8. 作为本体作者，我希望 OWL 2 QL imports closure、文件/URL 本体、XML Catalog、层级、等价、domain/range 和 inverse 产生与基线相同的可观察改写，以便本体可安全参与查询。
9. 作为本体作者，我希望 Ontop 拒绝、忽略或警告的非 OWL 2 QL 输入保持同类可观察行为，以便 rtop 不会意外成为能力超集。
10. 作为查询作者，我希望 BGP、OPTIONAL、MINUS、UNION、VALUES、BIND、subquery、FILTER、property path、EXISTS、IN、HAVING、SERVICE、dataset 和 modifier 均按基线语义执行，以便完整相关 SPARQL 层可替换。
11. 作为分析查询作者，我希望聚合、DISTINCT、GROUP_CONCAT、排序、日期、时区、语言、正则、cast 和函数结果保持语义与词法值，以便金融聚合和 PostgreSQL 查询保持可信。
12. 作为金融分析使用者，我希望 `xsd:decimal` 算术、ROUND 和聚合不因浮点误差改变金额，以便 SQL oracle 与 SPARQL 结果可逐值比较。
13. 作为 PostgreSQL 使用者，我希望 identifier、NULL、constraint、metadata、array、JSON/JSONB、nested data、PostGIS 和 PostgreSQL 函数受完整验收，以便 adapter 不只是连接 smoke test。
14. 作为高负载使用者，我希望查询被改写并下推为 PostgreSQL SQL，而非把虚拟图大规模搬到 Rust 内存 join，以便结果可完成且遵守 Ontop 的约束优化语义。
15. 作为并发客户端，我希望一个请求的超时、取消、失败和未绑定 binding 不影响其他请求，以便 endpoint 具有请求隔离。
16. 作为运维者，我希望客户端断开后相关 PostgreSQL 工作被释放或取消，以便不会耗尽连接或继续无用计算。
17. 作为审计者，我希望不含 ORDER BY 的结果按 bag 和多重性比较、含 ORDER BY 的结果按序列比较，以便偶然行序不会伪装为契约。
18. 作为审计者，我希望完整 RDF term、变量、graph、错误、HTTP status/header 和资源行为被比较，以便“200 OK”不被误当成功能等价。
19. 作为维护者，我希望 Ontop 源码基线和部署镜像行为分别记录，以便版本差异不会静默漂移。
20. 作为维护者，我希望 Ontop PostgreSQL 适用资产逐项进入 coverage ledger，以便当前七个业务 endpoint 不会缩小迁移范围。
21. 作为维护者，我希望 Java 宿主对象身份、RDF4J/OWLAPI API、Protégé、Portal、CLI materialize 和非 PostgreSQL 方言不进入该 ledger，以便 Rust 原生边界清晰。
22. 作为安全与复现维护者，我希望 imports 与 SERVICE 使用固定本地服务、Catalog 或受控夹具进行取证，以便验收不依赖不稳定公网。
23. 作为许可维护者，我希望任何来自 Ontop 的移植遵守 Apache-2.0 notice/attribution 要求，以便功能迁移具备可追溯许可链。
24. 作为发布负责人，我希望只有所有 in-scope ledger 条目完成、七服务集成通过且标准 gate 保存 provenance 后才声明可替换，以便完成声明可复核。

## 实现决策

- 延续 ADR-0001、ADR-0002、ADR-0003、ADR-0004 定义的 PostgreSQL 限定、可观察等价、双锚基线和运行契约。
- 采用单一最高 seam：原生 rtop PostgreSQL SPARQL endpoint compatibility ledger。它是配置、映射、本体、查询、改写、执行和 HTTP adapter 的共同验收入口。
- 查询执行架构从内存逐三元组 join 演进为可组合的 SPARQL algebra → TBox/mapping 展开 → PostgreSQL SQL 改写/优化 → 流式结果解码；所有公共入口共享此内核。
- SQL 文本、计划和 Java 内部对象不是契约；RDF/tuple/boolean/graph 语义、错误、协议和资源行为才是契约。
- 新增 SPARQL、mapping、OWL、PostgreSQL 或 HTTP 行为必须在固定 Ontop 基线中找到同类可观察证据；Ontop 已知限制应复现，而非增强为标准超集。
- endpoint 映射输入包括 native OBDA 和 R2RML；使用方只修改部署配置，不修改源代码或 HTTP 结果消费。
- OWL imports 和 SERVICE 的外部依赖保留基线可观察能力，但测试与发布证据使用受控 URL、XML Catalog 和本地服务；不将公网可用性写入结果语义。
- 并发请求隔离、取消传播、客户端断开清理和连接生命周期是内核能力，不是 HTTP adapter 的可选优化。
- 任何直接移植或翻译 Ontop 代码必须保留 Apache-2.0 所要求的 notice 与出处；优先用基线测试锁定行为后实现 Rust 原生模块。
- 没有新的独立原型代码；已有 rtop 内存执行路径是必须替换的反例，而 `intent-engine-ontop` 的七服务、错误本体和身份反例是集成验收资产。

## 测试决策

- 每个主能力在最高 endpoint seam 上以真实 PostgreSQL 服务端验证；细粒度 parser、mapping、ontology、optimizer 和 adapter 测试只用于定位回归。
- coverage ledger 的分母来自固定 Ontop 基线中与替换内核可观察行为相关的 PostgreSQL 资产，以及可无语义损失转换为 PostgreSQL 情景的 SPARQL、RDB2RDF 与 OWL 资产。
- 每个条目记录基线来源、版本锚、适用性、Ontop 结果、rtop 结果、断言强度、环境、timeout、清理策略、状态和复核命令；in-scope 的 deferred/unknown/unsupported 阻止完成。
- 以 Ontop 已提交 expected 为优先结果来源；无稳定 expected 时在固定基线环境采集规范化结果，并保存 provenance。
- tuple 比较保留变量、RDF term、datatype/language、空绑定和多重性；graph 使用规范化 RDF 比较；仅显式排序查询比较顺序。
- HTTP 矩阵覆盖方法、请求媒体类型、Accept、结果格式、dataset 参数、错误、超时、取消和客户端断开。
- 并发测试至少验证相互独立的成功/失败/取消请求，以及取消后 PostgreSQL 连接仍可复用。
- 外部 imports 与 SERVICE 测试使用本地 fixture server、XML Catalog 或固定资源，记录网络访问、失败和缓存行为。
- 以 `intent-engine-ontop` 七服务的结果、SQL oracle 和正反例作为端到端门槛；主服务、移除 subclass、错误 equivalentClass、weak alignment、unsafe identity、NPD 和规模服务均不得遗漏。
- 标准 gate 同时执行账本完整性、固定 PostgreSQL 容器、SPARQL/mapping/OWL/PostgreSQL/HTTP/OCI tests，并将结果和环境 provenance 归档；基础镜像 registry 不可用只能标外部阻断。

## 范围外

- 修改 `intent-engine-ontop` 的 Python 源代码、SPARQL 请求生成或结果消费逻辑。
- RDF4J/OWLAPI Java 对象 API、Protégé、Portal、Spring/Tomcat 实现、Maven/JAR/OSGi/JRE 发行物。
- materialize、bootstrap、metadata extraction 等未经过本替换调用路径的 CLI 产品面。
- MySQL、Oracle、SQL Server、Trino、DuckDB 等非 PostgreSQL 方言及其认证。
- OWL 2 DL 一般推理、Ontop 基线未提供同类可观察行为的额外语义。
- SPARQL Update；Ontop endpoint 对其自身即不提供成功执行语义。
- 同一 SQL 文本、查询计划或固定性能数字。

## 补充说明

- 本规格对应 `docs/research/intent-engine-ontop-replacement-contract.md`、`docs/research/intent-engine-ontop-rtop-gap-audit.md` 和 `docs/research/ontop-postgres-replacement-layer-inventory.md` 的取证结论。
- 当前最关键的阻断不是单个语法关键字，而是缺少完整 PostgreSQL SQL 改写与约束优化链；`IN`、decimal、imports、HTTP 格式和并发契约是可并行但不可替代的工作流。
- 历史 #35 的 PostgreSQL 认证仍是先例和证据来源；本 Issue 的完成不能由该历史结论自动推断。
