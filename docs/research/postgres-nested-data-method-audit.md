# PostgreSQL nested data 方法审计

固定基线 `AbstractNestedDataTest` 的十个方法分别由 Array、JSON 与 JSONB
PostgreSQL 子类继承。`postgres-nested-json-jsonb-array-flatten` 在真实
PostgreSQL 17 中读取三种原始列；case 比较 position/date/integer/二维/空元素/
manager/SPO/aggregate 的结果与五个 AVG literal 词法。

| 基线方法 | Rust PostgreSQL 断言 |
| --- | --- |
| `testUniqueConstraintsPreserved` | schema metadata 的 unique/primary-key 约束保留 |
| `testFlattenWithPosition` | position 展开 7 行 |
| `testFlattenTimestamp` | timestamp 展开 7 行 |
| `testFlattenInteger` | integer 展开 7 行 |
| `testFlatten2DArray` | 二维 array 展开 11 行 |
| `testFlattenWithEmptyElement` | 空元素保留 2 行 |
| `testFlattenJson` | JSON 展开 7 行 |
| `testFlattenJsonPossiblyNull` | nullable JSON manager 结果 5 行 |
| `testFlattenWithAggregate` | aggregate 89 行及五个 AVG literal |
| `testSPO` | subject/predicate/object 展开语义 |

验证脚本：`scripts/test-postgres-compat.sh`；本文件是固定父类方法集合的机械
审计来源，不将三个子类的同一继承集合重复计数。
