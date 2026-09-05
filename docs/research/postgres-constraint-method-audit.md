# PostgreSQL constraint 方法审计

固定基线 `AbstractConstraintTest` 的八个方法由
`ConstraintPostgreSQLTest` 继承。Java 的 metadata 对象不进入 Rust 契约；
`postgres-schema-constraints` 在真实 PostgreSQL 17 通过 `pg_constraint` 查询并
比较同一主键、唯一键和外键可观察关系。

| 基线方法 | Rust PostgreSQL 断言 |
| --- | --- |
| `testPrimaryBook` | Book 的 primary/unique 约束 |
| `testPrimaryKeyBookWriter` | BookWriter 无 unique/primary key |
| `testPrimaryKeyEdition` | Edition 的 primary/unique 约束 |
| `testPrimaryKeyWriter` | Writer 的 primary/unique 约束 |
| `testForeignKeyBook` | Book 无 foreign key |
| `testForeignKeyBookWriter` | BookWriter 的两个 foreign key |
| `testForeignKeyEdition` | Edition 的一个 foreign key |
| `testForeignKeyWriter` | Writer 无 foreign key |

验证脚本：`scripts/test-postgres-compat.sh`；本文件与固定父类方法集合由
`validate-postgres-method-audits.sh` 精确比较。
