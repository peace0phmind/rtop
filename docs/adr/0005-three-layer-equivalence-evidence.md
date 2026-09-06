# 三层功能等价的闭合证据门槛

状态：accepted。

## 背景

rtop 的替换内核只迁移固定 Ontop 基线中可由 PostgreSQL 虚拟知识图谱外部观察的
本体（OWL 2 QL）、原生 OBDA 映射和 SPARQL 层功能，不迁移 JVM 宿主 API。现有
PostgreSQL coverage ledger 能证明一组已发现资产通过，但不能单独证明三个层的功能
分母完整，也不能证明测试进入了每条实现路径。

## 决策

以固定 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 的源码入口、启用测试、
manifest、fixture 和明确 ignore 行为共同导出三个层的功能原子分母。每个原子必须在
闭合矩阵中具备：

1. Ontop 来源与行为定义；
2. rtop 对应模块/函数；
3. 同一固定 PostgreSQL 夹具上的 Ontop-vs-rtop 差分用例；
4. 规范化结果与环境 provenance；
5. 对应 Issue；
6. 行/分支覆盖证据。

差分用例必须比较成功、失败、拒绝、ignore、边界及必要的资源行为；tuple 比较保留
RDF term、bag、多重性和显式顺序。硬编码预期或独立 SQL oracle 可以作为辅助检查，
但不能代替 Ontop-vs-rtop 差分证据。

对 `src/ontology.rs`、`src/mapping.rs`、`src/sparql.rs` 和 `src/lib.rs` 中纳入三个层的
代码，关闭门槛为 `cargo llvm-cov` 行覆盖率至少 90%、分支覆盖率至少 80%，且所有新增或
修改行由矩阵用例覆盖。代码覆盖率不能单独证明语义等价。

任何矩阵空字段、未映射原子或未满足门槛均阻止 #35 与 #53 关闭；#1 与 #8 作为父级
路线图/规格也保持开放。

## 后果

旧的静态缺口审计只能作为历史输入。后续状态必须由闭合矩阵生成或与其逐项一致，
不得与 coverage ledger 并列给出矛盾的完成结论。
