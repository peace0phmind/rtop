# PostgreSQL 替换内核的运行与验收契约

状态：accepted。rtop 的 PostgreSQL 替换内核同时兼容 Ontop endpoint 可加载的 native OBDA 与 R2RML 映射，并以可转换为 PostgreSQL 的 Ontop 语义测试资产建立覆盖账本，而不迁移 Java 宿主对象或其他方言环境。兼容验收比较 SPARQL bag/有序结果、RDF term、HTTP 结果和错误行为；请求取消、连接释放与并发隔离也是可观察契约。外部本体 imports 和 SPARQL `SERVICE` 以 Ontop 同等可观察行为实现，并用固定本地服务、XML Catalog 或受控夹具取证，避免测试依赖不稳定公网；不要求复制 Ontop 的 SQL 文本或查询计划。
