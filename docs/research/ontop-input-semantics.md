# Ontop 输入语义审计：OWL、RDF facts、OBDA 与 R2RML

**参考版本：** Ontop `version5` 提交 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。
本清单只记录该提交实际入口的行为；不以 RDF、OWL 或 R2RML 标准的完整能力补足。

## 结论

`rtop` 必须把四类输入转换为 Rust 自有的 `Ontology`、`RDFFact`、`SQLPPMapping`
及 prefix 模型，并保留下列格式选择、base IRI、imports 和错误分类。RDF4J、OWLAPI
和 R2RML Java manager 的对象不得穿过 adapter 边界。

## Ontop 原生 `.obda`

| 项目 | 参考行为 | `rtop` 兼容要求 |
| --- | --- | --- |
| 入口 | `OntopNativeMappingParser.parse(File/Reader)` | 支持文件及 reader/字符串入口 |
| 结构 | 识别 `[PrefixDeclaration]` 与 `[MappingDeclaration]`；空行和首个非空字符为 `;` 的注释跳过 | 保留这两个区段和注释规则 |
| 映射项 | 每项由 `mappingId`、`target`、`source` 构成；target 可续行，source 可多行 | 输出 SQL source + 已解析 RDF target atoms 的内部映射 |
| 明确拒绝 | `[ClassDeclaration]`、`[ObjectPropertyDeclaration]`、`[DataPropertyDeclaration]` 已废弃；`[SourceDeclaration]` 自 3.0 起不支持；未知语法拒绝 | 不支持 JDBC source declaration；数据源配置另立 `rtop` 配置 |
| 错误 | 不存在/不可读文件为 `MappingIOException`；逐行读取错误含文件名、行号和原始消息；无效 target 或缺 id/source/target 汇总为 `InvalidMappingException` | 将 I/O、语法/行号、映射校验分开报告 |
| 非能力 | `parse(Graph)` 明确抛错：原生语言没有 RDF serialization | 不把 RDF graph 当 `.obda` 输入 |

源码：`mapping/sql/native/.../OntopNativeMappingParser.java`。

## R2RML

| 项目 | 参考行为 | `rtop` 兼容要求 |
| --- | --- | --- |
| 文件/reader 格式 | `R2RMLMappingParser.parse(File/Reader)` 始终调用 `Rio.createParser(RDFFormat.TURTLE)` | 文件和 reader 的 R2RML 入口仅承诺 Turtle；不要宣称 RDF/XML、JSON-LD 等 R2RML 文件支持 |
| base IRI | 文件为 `file://<mappingFile>`；reader 固定 `http://example.org/baseIRI/`（源码 TODO） | 保留可观察的相对 IRI 解析策略，或以兼容测试覆盖替代策略 |
| graph 入口 | 接受 Commons-RDF `Graph`，并从 namespace 生成 `prefix:` → IRI | Rust RDF graph adapter 可提供等价入口，但其 graph 类型不可成为核心类型 |
| 转换/校验 | `RDF4JR2RMLMappingManager.importMappings` 后交给 `R2RMLToSQLPPTriplesMapConverter`；RDF 解析异常及 `InvalidR2RMLMappingException` 转为 `InvalidMappingException` | 将 Turtle 语法错误与 R2RML 结构/语义校验错误区分，并输出 `SQLPPMapping` |
| 序列化相关边界 | R2RML serializer 输出 Turtle；重复 `.obda` mapping ID 报 `Duplicate mapping IDs found in obda file` | #6 的 CLI 审计可保留转换命令语义；本票据不扩大输入格式 |

源码：`mapping/sql/r2rml/.../R2RMLMappingParser.java`、
`R2RMLMappingSerializer.java`；测试：`test/rdb2rdf-compliance` 和 CLI 的
OBDA↔R2RML 转换测试。

## RDF facts

| 项目 | 参考行为 | `rtop` 兼容要求 |
| --- | --- | --- |
| 来源 | file、URL 或 reader；无来源时返回空 optional | 支持等价来源；网络读取作为输入 adapter 行为 |
| 格式 | 优先 `factFormat`（按文件名后缀交给 Rio 推断；未知值回退 Turtle）；否则仅从 **facts file 名** 推断 | 只对 Rust 所选解析库实际支持、且有对照测试的格式作出承诺；URL/reader 未给显式格式不能从其名称推断 |
| 无格式错误 | 无有效显式格式且无法从文件名推断：`No valid fact file format was provided...` | 保留这个独立的配置错误类别 |
| base IRI | 使用 `factsBaseIRI`；未指定时生成随机 `http://<UUID>.example.org/data/` | Rust 配置应允许显式 base IRI；默认随机 base 是参考行为，需作为兼容回归项 |
| 内部模型 | 收集 RDF statements；subject/object bnode→bnode constant、IRI→IRI constant、literal→语言或 datatype literal；有 context 则为 quad，否则 triple | 保存 triple/quad、blank node、IRI、语言标签和 datatype，不保存 RDF4J Statement |
| 错误 | I/O、RDF parse/handler、非法参数均包装为 `FactsException`，解析前缀为 `An error occured while parsing the facts file:` | 保留 I/O 与解析错误的可诊断来源 |

源码：`mapping/owlapi/.../OntopMappingOntologyConfigurationImpl.java` 的
`loadFactsFromFile`、`loadFactsWithReader`、`statementToFact`。

## OWL 本体与 OWL 2 QL 转换

| 项目 | 参考行为 | `rtop` 兼容要求 |
| --- | --- | --- |
| 来源 | file、reader（读为 UTF-8 bytes）或 URL；均交 OWLAPI `loadOntologyFromOntologyDocument` | Rust loader 应把文件/reader/URL 的文档解析与转换分开；实际文件格式集取决于所选 Rust parser 的已验证能力 |
| imports | `getImportsClosure(owl)`；所有 imports 的 vocabulary 和 axioms 合并翻译 | 需要明确支持 imports closure；XML catalog/导入解析失败须可诊断 |
| XML catalog | 配置时设置 OWLAPI `XMLCatalogIRIMapper`；catalog I/O 错误转为 ontology creation error | 若要保留该功能，作为 Rust import resolver adapter 单独实现；不可假称已由一般 URL loader 覆盖 |
| 支持的公理 | translator 显式处理 subclass/equivalent/disjoint classes，object/data property sub/equivalent/disjoint/inverse/domain/range，reflexive/irreflexive/symmetric/asymmetric，class/object/data assertions、different individuals、datatype definitions | 以这些已观察到的转换为最初范围，并逐项形成 fixture |
| 非 OWL 2 QL/不支持 | 复杂 class expression、匿名 individual 等在转换时被拒绝或记录 `NOT_SUPPORTED_EXT` 警告；functionality、transitivity、negative assertions、same individual、property chain、has key、SWRL、annotation 等 visitor 路径不是完整推理能力 | 以“转换后的 Ontop 内部模型及其告警/失败”对照验收，不宣称完整 OWL 2 DL 或全 OWL 2 QL 以外推理 |
| 一致性 | 发现不一致时记录并以运行时异常中止转换 | Rust 需定义对应的 ontology inconsistency 错误类别 |

源码：`ontology/owlapi/.../OWLAPITranslatorOWL2QL.java`（imports closure 与 visitor）和
`mapping/owlapi/.../OntopMappingOntologyConfigurationImpl.java`（文档加载）。

## Rust 验收切片

1. `.obda`：合法 prefix/mapping、废弃 source declaration、未知标签和带行号的 target
   错误。
2. R2RML：Turtle 合法 mapping、Turtle 语法错误、无效 TriplesMap、file 与 reader 的
   相对 IRI。
3. facts：Turtle（及每种后来有事实对照的已选格式）的 triple、quad、bnode、language、
   datatype、显式/缺失格式及 base IRI。
4. OWL：imports closure、支持的 subclass/property/assertion、公认不支持的 complex
   expression 和不一致本体。

这些 fixture 先在 Ontop 的 Docker JDK 11/17 基线中得到结果，再在 Rust adapter
测试中比较内部模型或对外查询结果。解析库的能力绝不能自动扩大 `rtop` 的承诺。
