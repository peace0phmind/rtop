# PostgreSQL Cast 最终覆盖审计

基线固定为 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。

审计对象：

- `test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/AbstractCastFunctionsTest.java`
- `test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/postgresql/CastPostgreSQLTest.java`

父类共有 86 个 `testCast*` 方法。PostgreSQL 子类禁用 `testCastDateTimeFromDate1`、`testCastDateTimeFromDate2`、`testCastDateTimeFromDate3`、`testCastDateTimeFromString`；父类禁用 `testCastDateTimeFromDateTime1`。这 5 项均不计入可执行 PostgreSQL 语料。因此，本审计要求并证明其余 **81** 项均由 `scripts/test-postgres-compat.sh` 中的 Rust CLI fixture 执行。

| 基线方法 | Rust fixture / 证据 |
| --- | --- |
| `testCastFloatFromFloat`, `testCastFloatFromDouble`, `testCastFloatFromDecimal2`, `testCastFloatFromInteger`, `testCastFloatFromBoolean`, `testCastFloatFromString2`, `testCastFloatFromString3`, `testCastFloatFromString4` | `cast-float-from-float`、`cast-float-from-double`、`cast-float-from-decimal`、`cast-float-from-integer`、`cast-float-from-boolean`、`cast-float-from-string`、`cast-float-and-double-from-string`、`cast-float-from-invalid-string` |
| `testCastFloatFromDecimal1` | `cast-float-from-mapped-decimal`，固定 `books.obda` + PostgreSQL `numeric(38,2)` |
| `testCastFloatFromDateTime`, `testCastFloatFromDate`, `testCastFloatFromString1` | `cast-invalid-direct`、`cast-mapped-title-invalid` |
| `testCastDoubleFromFloat`, `testCastDoubleFromDouble`, `testCastDoubleFromInteger`, `testCastDoubleFromBoolean`, `testCastDoubleFromString1`, `testCastDoubleFromString2`, `testCastDoubleFromString3` | `cast-double-from-float`、`cast-double-from-double`、`cast-double-from-integer`、`cast-double-from-boolean`、`cast-float-and-double-from-string`、`cast-double-from-string-integer`、`cast-invalid-direct` |
| `testCastDoubleFromDecimal` | `cast-double-from-mapped-decimal` |
| `testCastDoubleFromDateTime`, `testCastDoubleFromDate` | `cast-invalid-direct` |
| `testCastDecimalFromFloat`, `testCastDecimalFromDouble`, `testCastDecimalFromDecimal2`, `testCastDecimalFromInteger`, `testCastDecimalFromBoolean`, `testCastDecimalFromString2`, `testCastDecimalFromString3`, `testCastDecimalFromString4` | `cast-decimal-direct`、`cast-decimal-from-decimal`、`cast-decimal-from-boolean`、`cast-decimal-from-string`、`cast-invalid-direct` |
| `testCastDecimalFromDecimal1` | `cast-decimal-from-mapped-decimal` |
| `testCastDecimalFromDateTime`, `testCastDecimalFromDate`, `testCastDecimalFromString1` | `cast-invalid-direct`、`cast-mapped-title-invalid` |
| `testCastIntegerFromFloat1`, `testCastIntegerFromFloat2`, `testCastIntegerFromFloat3`, `testCastIntegerFromDouble`, `testCastIntegerFromDecimal2`, `testCastIntegerFromInteger`, `testCastIntegerFromBoolean`, `testCastIntegerFromString2`, `testCastIntegerFromString3`, `testCastIntegerFromString4` | `cast-integer-direct`、`cast-integer-from-float-negative`、`cast-integer-from-boolean`、`cast-integer-from-string`、`cast-invalid-direct` |
| `testCastIntegerFromDecimal1` | `cast-integer-from-mapped-decimal` |
| `testCastIntegerFromDateTime`, `testCastIntegerFromDate`, `testCastIntegerFromString1` | `cast-invalid-direct`、`cast-mapped-title-invalid` |
| `testCastBooleanFromFloat`, `testCastBooleanFromDouble`, `testCastBooleanFromInteger`, `testCastBooleanFromBoolean`, `testCastBooleanFromString2`, `testCastBooleanFromString3`, `testCastBooleanFromString4` | `cast-boolean-direct`、`cast-boolean-from-float`、`cast-boolean-from-invalid-string` |
| `testCastBooleanFromDecimal` | `cast-boolean-from-mapped-decimal` |
| `testCastBooleanFromDateTime`, `testCastBooleanFromDate`, `testCastBooleanFromString1` | `cast-invalid-direct`、`cast-mapped-title-invalid` |
| `testCastStringFromFloat`, `testCastStringFromDouble`, `testCastStringFromDecimal2`, `testCastStringFromInteger`, `testCastStringFromDateTime`, `testCastStringFromDate`, `testCastStringFromBoolean`, `testCastStringFromString1`, `testCastStringFromString2` | `cast-string-direct`、`cast-string-from-datetime` |
| `testCastStringFromDecimal1` | `cast-string-from-mapped-decimal` |
| `testCastStringFromIRI`, `testCastStringFromLiteral` | `cast-string-iri-and-literal` |
| `testCastDateFromDateTime1` | `cast-date-from-mapped-datetime`；在其它 expression 断言完成后恢复固定 `books-potgresql.sql` 时间戳 |
| `testCastDateFromDateTime2`, `testCastDateFromDate`, `testCastDateFromString1`, `testCastDateFromString2`, `testCastDateFromInteger`, `testCastDateFromDouble` | `cast-date-from-datetime`、`cast-date-direct`、`cast-date-from-invalid-string`、`cast-date-from-integer`、`cast-invalid-direct` |
| `testCastDateTimeFromDateTime2`, `testCastDateTimeFromInteger`, `testCastDateTimeFromDouble` | `cast-datetime-from-datetime`、`cast-invalid-direct` |

## 执行证据

```sh
./scripts/test-postgres-compat.sh
cargo test --all --locked
cargo fmt --check
jq empty compatibility-report.json
git diff --check
```

以上命令均通过。`compatibility-report.json` 的 `postgres-cast-float-from-double` 条目记录基线来源、Docker 环境、books 映射输入与可观察结果。
