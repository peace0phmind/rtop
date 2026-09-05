# PostgreSQL BindWithFunctions 方法级覆盖审计

审计对象为固定只读基线
`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 的
`test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractBindTestWithFunctions.java`。

该类有 **91 个启用的 `@Test` 方法**。下表按方法逐项列出 rtop 的 PostgreSQL Docker
门禁；每个列出的 fixture 都由 `scripts/test-postgres-compat.sh` 通过 Rust CLI 对
`postgres:17` 服务端执行。`ofn-mapping.toml` 直接引用基线
`pgsql/bind/sparqlBindPostgreSQL.obda`。

| 基线方法 | rtop PostgreSQL 证据 |
| --- | --- |
| `testAndBind` | `logical-and-mapping-input.rq` |
| `testAndBindDistinct` | `logical-and-distinct-mapping-input.rq` |
| `testOrBind` | `logical-or-mapping-input.rq` |
| `testCeil` | `numeric-functions-mapping-input.rq` |
| `testFloor` | `numeric-functions-mapping-input.rq` |
| `testRound` | `numeric-functions-mapping-input.rq` |
| `testAbs` | `numeric-functions-mapping-input.rq` |
| `testHashSHA256` | `sha256-mapping-input.rq` |
| `testStrLen` | `strlen-mapping-input.rq` |
| `testSubstr2` | `substr-language.rq` |
| `testSubstr3` | `substr-language.rq` |
| `testURIEncoding` | `encode-for-uri-mapping-input.rq` |
| `testStrEnds` | `strends-mapping-input.rq` |
| `testStrStarts` | `strstarts-mapping-input.rq` |
| `testStrSubstring` | `substr-mapping-input.rq` |
| `testContainsBind` | `contains-bind-mapping-input.rq` |
| `testContainsFilter` | `contains-filter-mapping-input.rq` |
| `testBindWithUcase` | `case-concat-mapping-input.rq` |
| `testBindWithLcase` | `case-concat-mapping-input.rq` |
| `testBindWithBefore1` | `str-before-after-mapping-input.rq` |
| `testBindWithBefore2` | `str-before-after-mapping-input.rq` |
| `testBindWithAfter1` | `str-before-after-mapping-input.rq` |
| `testBindWithAfter2` | `str-before-after-mapping-input.rq` |
| `testMonth` | `datetime-extractors-mapping-input.rq` |
| `testYear` | `datetime-extractors-mapping-input.rq` |
| `testDay` | `datetime-extractors-mapping-input.rq` |
| `testMinutes` | `datetime-extractors-mapping-input.rq` |
| `testHours` | `datetime-extractors-mapping-input.rq` |
| `testSeconds` | `datetime-extractors-mapping-input.rq` |
| `testNow` | `now.rq` |
| `testUuid` | `random-identifiers.rq` |
| `testStrUuid` | `random-identifiers.rq` |
| `testRand` | `random-identifiers.rq` |
| `testDivide` | `divide-mapping-input.rq` |
| `testTZ` | `tz-timezone-mapping-input.rq` |
| `testBound` | `bound-optional-mapping-input.rq` |
| `testRDFTermEqual1` | `rdf-term-equal-mapping-input.rq` |
| `testRDFTermEqual2` | `rdf-term-str-not-equal-mapping-input.rq` |
| `testSameTerm` | `same-term-mapping-input.rq` |
| `testIsIRI` | `term-predicates-mapping-input.rq` |
| `testIsBlank` | `bnode.rq`（基线方法本身无断言） |
| `testIsLiteral` | `term-predicates-mapping-input.rq` |
| `testIsNumeric` | `term-predicates-mapping-input.rq` |
| `testStr` | `str-timezone-mapping-input.rq` |
| `testLang` | `lang-mapping-input.rq` |
| `testDatatype` | `datatype-mapping-input.rq` |
| `testConcat` | `concat-mapping-input.rq` |
| `testLangMatches` | `langmatches-optional-mapping-input.rq` |
| `testREGEX` | `regex-optional-mapping-input.rq` |
| `testREPLACE` | `replace-mapping-input.rq` |
| `testConstantFloatDivide` | `numeric-promotion.rq` |
| `testConstantFloatIntegerDivide` | `numeric-promotion.rq` |
| `testConstantFloatDecimalDivide` | `numeric-promotion.rq` |
| `testConstantFloatDoubleDivide` | `numeric-promotion.rq` |
| `testConstantDoubleDoubleDivide` | `numeric-promotion.rq` |
| `testConstantIntegerDivide` | `numeric-promotion.rq` |
| `testCoalesceDivideByZeroInt` | `coalesce-arithmetic-errors.rq` |
| `testCoalesceDivideByZeroDecimal` | `coalesce-arithmetic-errors.rq` |
| `testCoalesceInvalidDivide1` | `coalesce-arithmetic-errors.rq` |
| `testCoalesceInvalidDivide2` | `coalesce-arithmetic-errors.rq` |
| `testCoalesceInvalidSum` | `coalesce-arithmetic-errors.rq` |
| `testCoalesceInvalidSub` | `coalesce-arithmetic-errors.rq` |
| `testCoalesceInvalidTimes` | `coalesce-arithmetic-errors.rq` |
| `testBNODE0` | `bnode.rq` |
| `testBNODE1` | `bnode.rq` |
| `testIRI1` | `iri-uri.rq` |
| `testIRI1_2` | `iri-uri.rq` |
| `testIRI2` | `iri-uri.rq` |
| `testIRI3` | `iri-uri-relative.rq` |
| `testIRI4` | `iri-uri-relative.rq` |
| `testIRI5` | `iri-uri.rq` |
| `testIRI6` | `iri-uri.rq` |
| `testIRI7` | `iri-union-values.rq` |
| `testIRI8` | `iri-uri-relative.rq` |
| `testIF1` | `if-coalesce.rq` |
| `testIF2` | `if-coalesce.rq` |
| `testIF3` | `if-coalesce.rq` |
| `testIF4` | `if-coalesce.rq` |
| `testIF5` | `if-coalesce.rq` |
| `testIF6` | `if-coalesce.rq` |
| `testWeeksBetweenDate` | `ofn-date-between.rq` |
| `testDaysBetweenDate` | `ofn-date-between.rq` |
| `testWeeksBetweenDateTime` | `ofn-between.rq` |
| `testDaysBetweenDateTime` | `ofn-between.rq` |
| `testDaysBetweenDateTimeMappingInput` | `ofn-mapping-input.rq` |
| `testDaysBetweenDateMappingInput` | `ofn-mapping-input.rq` |
| `testHoursBetween` | `ofn-between.rq` |
| `testMinutesBetween` | `ofn-between.rq` |
| `testSecondsBetween` | `ofn-between.rq` |
| `testSecondsBetweenMappingInput` | `ofn-mapping-input.rq` |
| `testMilliSeconds` | `ofn-millis-between.rq` |

## 不在执行集的基线方法

`testHashMd5`、`testHashSHA1`、`testHashSHA384`、`testHashSHA512` 与
`testDivideByZeroFloat` 在固定基线均标注 `@Ignore`，不属于其执行测试集，因此本审计
不将它们记为 rtop 已通过测试。
