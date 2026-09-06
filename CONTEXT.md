# rtop

`rtop` 是以 Rust 实现的虚拟知识图谱。它在不复制 JVM 宿主功能的前提下，以关系数据源、映射和本体提供 SPARQL 查询能力。

## Language

**虚拟知识图谱（VKG）**：由关系数据源的内容、映射和本体在查询时共同呈现的知识图谱；其 RDF 数据通常不被独立持久化。
_Avoid_: RDF 数据库

**关系数据源**：保存业务数据并执行改写后 SQL 的关系数据库或查询引擎。
_Avoid_: 本体库, 三元组库

**映射**：将关系数据源中的行或 SQL 查询结果解释为 RDF 术语和 RDF facts 的规则。
_Avoid_: ETL

**本体（TBox）**：描述类、属性及其可用于查询推理的关系的知识模型，不包含关系数据源中的业务实例。
_Avoid_: 数据库模式

**RDF fact**：由映射或输入事实得到的单个 RDF 陈述，可作为虚拟知识图谱中的数据实例。
_Avoid_: 数据库行

**查询改写**：依据本体和映射，把针对虚拟知识图谱的 SPARQL 查询转换为针对关系数据源的 SQL 查询的过程。
_Avoid_: 数据迁移

**对照情景**：固定参考实现版本、输入、运行环境和预期比较规则的一组可重复验收条件。
_Avoid_: oracle

**对照结果**：参考实现于对照情景中产生、供 `rtop` 比较的规范化成功结果或错误分类。
_Avoid_: oracle

**PostgreSQL 限定对照范围**：固定 Ontop 基线中可由 Rust CLI、HTTP 或 OCI 与 PostgreSQL 服务端共同观察的 VKG 行为；它不包含 Java 进程内类型或其他数据库方言。
_Avoid_: 所有 Java 测试, 全方言兼容

**覆盖账本**：逐条关联 Ontop 测试资产、其范围状态、`rtop` 对照情景和可复核证据的清单。
_Avoid_: 测试数量, 覆盖率猜测

**对照断言强度**：某个 Ontop 对照情景实际比较的结果信息量，如完整 RDF 术语结果、布尔值、图，或仅基数；它限定该情景能证明的等价范围。
_Avoid_: 全等价, 测试通过

**零代码改动替换**：以 rtop 替换指定使用方的 Ontop 部署时，使用方的源代码、HTTP 请求和响应消费方式均无需修改；Compose、环境变量、挂载输入和连接配置可迁移为 rtop 原生形式。
_Avoid_: 部署配置零改动, 可通过修改调用方源码完成的迁移

**Ontop 功能上限**：rtop 为某个对照使用方补齐的能力，必须可追溯到固定 Ontop 基线的同类可观察行为；不能以替换为名扩大该使用方之外的产品能力。
_Avoid_: 功能增强, 更强实现

**相关 Ontop 层**：`intent-engine-ontop` 调用路径穿过的 Ontop 模块边界；对其功能的 PostgreSQL 适用部分须完整迁移，而非只实现当前查询文本触及的语法分支。
_Avoid_: 当前查询子集, 全部 Ontop 产品模块

**替换内核**：从 SPARQL HTTP 请求经 native OBDA 映射、OWL 2 QL 本体改写、SPARQL 查询代数/优化和 PostgreSQL 方言到结果响应的完整可观察链路；不包括 RDF4J/OWLAPI 的 Java 对象 API、Protégé 或 Portal UI。
_Avoid_: Ontop 全产品, 仅 HTTP 路由

**完整替换验收**：七个 `intent-engine-ontop` endpoint 场景是集成验收；固定 Ontop 基线中替换内核的全部 PostgreSQL 适用测试资产是查漏账本，二者均须有可复核结果。
_Avoid_: 仅集成场景, 测试数量猜测

**OWL 2 QL 替换语义**：替换内核须实现 Ontop 可观察的 OWL 2 QL 查询改写，包括 imports、类/属性层级、等价、domain/range 和 inverse；不迁移 OWLAPI Java API 或一般 OWL 推理。
_Avoid_: OWLAPI 兼容, 全 OWL 推理

**imports 闭包兼容**：rtop 以固定 Ontop 基线的 OWL imports 装载行为为准，包括 imports closure、URL 或文件本体输入及 XML Catalog IRI 重定向；不将仅支持 `file://` 视为完整兼容。
_Avoid_: 仅本地文件 imports, 自行限定离线 imports

**优化兼容**：rtop 须保持 Ontop PostgreSQL 优化相关测试所断言的结果和资源行为，但不要求生成同一 SQL 文本、同一查询计划或相同性能数值；性能以代表性防退化阈值验收。
_Avoid_: 逐字 SQL, 无性能边界

**双锚基线**：固定 Ontop 源码提交限定可迁移功能的上限，而使用方已锁定镜像在其既有情景中的可观察结果限定替换验收；两者不一致时记录版本差异，不把任一方静默当作另一方。
_Avoid_: 单一版本推断, 静默版本漂移

**语义保真**：对 Ontop 已知的限制、拒绝、警告和边界结果，rtop 复现其可观察行为，而非把它们“修正”为标准或更强功能。
_Avoid_: 标准优先, 无意的能力超集

**端点映射输入兼容**：替换内核接受 Ontop endpoint 可加载的 native OBDA 与 R2RML 映射；格式是否被当前使用方采用不缩小端点映射层的完整边界。
_Avoid_: 仅当前 `.obda`, 映射格式超集

**PostgreSQL 资产资格**：只有能够表达替换内核可观察行为的 Ontop 测试资产才进入覆盖账本；Java 对象身份、JVM 异常类和其他方言环境不迁移，但其 SPARQL、映射或 OWL 语义须转换为 PostgreSQL 情景后验收。
_Avoid_: 直接照搬 Java 测试, 因宿主语言排除语义

**请求隔离**：一个 SPARQL 请求的取消、失败、连接和未绑定结果不得影响并发请求；客户端中断后相关 PostgreSQL 工作须可释放或取消。
_Avoid_: 串行可用, 进程级取消

**结果顺序断言**：未含 `ORDER BY` 的 tuple 结果按 SPARQL bag 语义比较，含 `ORDER BY` 的结果按规范化序列比较；不将偶然数据库返回顺序视为兼容契约。
_Avoid_: 总是逐行序列比较, 忽略多重性

**三层功能分母**：固定 Ontop 基线中可由 PostgreSQL VKG 外部观察的 OWL 2 QL 本体、原生 OBDA 映射与 SPARQL 功能原子；由源码入口、启用测试、manifest、fixture 及明确 ignore 行为共同导出。Java 对象 API 不在分母，但其承载的三层语义不得遗漏。
_Avoid_: Rust 已有分支, 当前业务查询子集, Java API 数量

**闭合矩阵**：每个三层功能原子到 Ontop 来源、行为定义、rtop 函数、Ontop-vs-rtop 差分用例、结果 provenance、代码覆盖和 Issue 的完整映射；任何空字段均表示该原子未完成。
_Avoid_: 单独的测试数量, 单独的覆盖账本, 硬编码预期

**差分证据**：Ontop 与 rtop 在同一固定 PostgreSQL 夹具上，对成功、失败、拒绝、ignore、边界及资源行为进行规范化比较的结果；tuple 保留 RDF term、bag、多重性和必要的顺序。
_Avoid_: 仅 SQL oracle, 仅 rtop 预期值

**层代码覆盖率**：闭合矩阵用例对本体、映射、SPARQL 与运行时层实现路径的行/分支覆盖度；它辅助证明测试经过实现路径，但不能替代差分证据。
_Avoid_: 语义等价证明, 全仓库测试数量
