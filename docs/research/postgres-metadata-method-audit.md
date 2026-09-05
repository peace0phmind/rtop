# PostgreSQL JDBC metadata 继承方法审计

固定基线 `AbstractDbMetadataInfoTest` 被
`PgSqlMetadataInfoTest` 继承，声明且执行一个方法：`testPropertyInfo`。

| 基线方法 | 分类 | 原因与 Rust 证据 |
| --- | --- | --- |
| `testPropertyInfo` | excluded | 方法调用 JDBC `DriverManager.getDriver` 与 `Driver.getPropertyInfo`，并遍历 `DriverPropertyInfo[]`；这是 ADR-0001 排除的 Java/JDBC 宿主类型 API，不是 PostgreSQL VKG 的跨语言查询契约。对应配置读取、真实 PostgreSQL 连接与稳定 CLI 诊断由 `postgres-cli-query-validate-and-exit-categories` 在 `scripts/test-delivery-compat.sh` 中验证。 |

这不是未实现或基线忽略：仅排除 Java 对象模型本身。固定源方法集合由
`scripts/validate-postgres-method-audits.sh` 与本文件精确比较。
