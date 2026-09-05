# LeftJoinProf PostgreSQL 最终审计

固定来源：`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 的
`AbstractLeftJoinProfTest` 与 `postgres/LeftJoinProfPgSQLTest`。父类共有 52 个
`test*` 方法；下列组均由 `tests/compat/postgres-aggregates/` 的 PostgreSQL Docker
gate 直接执行，使用 `redundant_join_fk_test.obda` 与其 RDF/XML OWL。

| 基线方法 | fixture 组 |
| --- | --- |
| `testMinusNickname`、`testMinus2`、`testMinusLastname` | `optional-unbound-*` |
| `testSimpleFirstName`、`testRequiredTeacherNickname`、`testFullName1`、`testFullName2`、`testFirstNameNickname`、`testSimpleNickname`、`testNicknameAndCourse` | `simple-first-name`、`optional-*` |
| `testCourseTeacherName`、`testCourseJoinOnLeft1`、`testCourseJoinOnLeft2`、`testNotEqOrUnboundCondition`、`testPreferences`、`testUselessRightPart2`、`testOptionalTeachesAt`、`testOptionalTeacherID` | `course-*`、`optional-*` |
| `testSumStudents1`、`testSumStudents2`、`testSumStudents3`、`testSumStudents4`、`testSumStudents5`、`testAvgStudents1`、`testAvgStudents2`、`testAvgStudents3`、`testMinStudents1`、`testMinStudents2`、`testMaxStudents1`、`testMaxStudents2`、`testDuration1`、`testMultitypedSum1`、`testMultitypedAvg1` | `sum-students-*`、`avg-students-*`、`min-*`、`max-*`、`duration-1`、`multityped-*` |
| `testMinusMultitypedSum`、`testMinusMultitypedAvg`、`testLimitSubQuery1` | `minus-multityped-*`、`limit-subquery-1` |
| `testSumOverNull1`、`testAvgOverNull1`、`testCountOverNull1`、`testMinOverNull1`、`testMaxOverNull1` | `empty-numeric-aggregates`、`min-max` |
| `testGroupConcat1`、`testGroupConcat2`、`testGroupConcat3`、`testGroupConcat4`、`testGroupConcat5`、`testGroupConcat6`、`testDistinctAsGroupBy1` | `group-concat-*`、`distinct-as-group-by` |
| `testProperties`、`testNonOptimizableLJAndJoinMix` | `properties`、`non-optimizable-left-join-mix` |
| `testValuesNodeOntologyProperty`、`testAggregationMappingProfStudentCountProperty` | `values-node-ontology-property`、`aggregation-mapping-prof-student-count-property` |

SQL 形状断言（如 Java 侧检查 SQL 是否出现 `LEFT`）不属于 Rust 运行时稳定契约；本迁移以
SPARQL 结果、RDF term、顺序及 PostgreSQL 实际 mapping/ontology 输入为验收依据。
