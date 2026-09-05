# PostgreSQL 限定的可观察等价范围

状态：accepted。`rtop` 以固定 Ontop 基线的跨语言可观察 VKG 行为为迁移单位，并只以 PostgreSQL 服务端证明关系数据源行为；不复制 Java/JDBC/RDF4J/OWLAPI/Protégé 宿主类型，也不把其他方言计入完成率。此边界在“复刻全部 Java 技术栈”和“只做少量 PostgreSQL smoke test”之间取舍：前者违背 Rust 原生产品边界，后者不能证明功能等价，因此每项保留行为必须进入逐条覆盖账本和可重复的 PostgreSQL gate。
