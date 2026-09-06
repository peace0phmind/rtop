# intent-engine-ontop 的 PostgreSQL 替换内核

状态：accepted。为在不修改 `intent-engine-ontop` 源代码的前提下以 rtop 替换其 Ontop 服务，rtop 迁移 PostgreSQL 上从 SPARQL HTTP 到 native mapping、OWL 2 QL 改写、查询改写/优化和结果协议的完整可观察内核；排除 Java 宿主 API、Protégé、Portal、CLI materialize 与其他数据库方言。固定 Ontop 源码提交限定功能上限，使用方锁定 Ontop 镜像在七个 endpoint 情景中的结果限定集成验收；两者差异须显式记录。每项迁移以 Ontop PostgreSQL 适用测试资产和使用方集成情景取证，复现 Ontop 的限制与错误行为而不擅自扩展能力，且不要求相同 SQL 文本或查询计划。
