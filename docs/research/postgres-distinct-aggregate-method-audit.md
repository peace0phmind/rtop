# PostgreSQL DISTINCT 聚合方法审计

固定基线 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 的
`AbstractDistinctInAggregateTest` 声明四个 `test*` 方法。它们由
`DistinctInAggregatePostgresTest` 继承，且 Rust PostgreSQL Docker case
`postgres-distinct-aggregates` 直接读取同一 `university.obda` 和四条 query。

| 基线方法 | Rust PostgreSQL 断言 |
| --- | --- |
| `testGroupConcatDistinct` | `GROUP_CONCAT(DISTINCT; separator=|)` 的完整词法及允许顺序 |
| `testSumDistinct` | `SUM(DISTINCT)=21` |
| `testAvgDistinct` | `AVG(DISTINCT)=10.5` 与单值组 |
| `testCountDistinct` | `COUNT(DISTINCT)=2` |

验证脚本：`scripts/test-postgres-compat.sh`；该文档由
`validate-postgres-method-audits.sh` 以方法名集合与固定基线精确比较。
