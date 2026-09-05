#!/usr/bin/env sh
set -eu

# 验证方法级审计没有因缩写、继承或 @Ignore 漏掉基线方法。该检查只读固定
# Ontop commit；每组的 Markdown 仍负责说明具体 PostgreSQL case 或 ignore 理由。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
baseline="$root/../ontop"
commit=5ec07573b18513f33dfcd59ac45fe26a81f9cdbd
[ "$(git -C "$baseline" rev-parse HEAD)" = "$commit" ]

check_audit() {
  id=$1
  source=$2
  audit=$3
  expected=$4
  source_names=$(mktemp)
  audit_names=$(mktemp)
  trap 'rm -f "$source_names" "$audit_names"' EXIT HUP INT TERM

  git -C "$baseline" show "$commit:$source" \
    | sed -n 's/.*void \(test[A-Za-z0-9_]*\)[[:space:]]*(.*/\1/p' \
    | sort -u > "$source_names"
  rg -o '`test[A-Za-z0-9_]*`' "$root/$audit" \
    | tr -d '`' | sort -u > "$audit_names"

  [ "$(wc -l < "$source_names" | tr -d ' ')" = "$expected" ]
  [ "$(wc -l < "$audit_names" | tr -d ' ')" = "$expected" ]
  diff -u "$source_names" "$audit_names"
  rm -f "$source_names" "$audit_names"
  trap - EXIT HUP INT TERM
  printf '%s\n' "method audit: $id ($expected methods)"
}

# 直接 PostgreSQL/annotation 选中的 class 自己声明的每个 test* 方法也必须
# 出现在方法级审计文档。它们有些是子类 Disabled override，不能因没有进入父类
# 审计而从发现分母消失；是否执行/忽略的理由由对应文档和兼容门禁分别承担。
catalog=$(mktemp)
trap 'rm -f "$catalog"' EXIT HUP INT TERM
"$root/scripts/discover-postgres-baseline-assets.sh" > "$catalog"
declared_method_count=$(jq '.declared_test_method_assets | length' "$catalog")
[ "$declared_method_count" = 86 ]
missing_declared_methods=$(jq -r '.declared_test_method_assets[] | [.source_path, .method] | @tsv' "$catalog" \
  | while IFS="$(printf '\t')" read -r source method; do
      if ! rg -F -- "\`$method\`" \
        "$root/docs/research/postgres-baseline-test-inventory.md" \
        "$root/docs/research/postgres-bind-functions-coverage.md" \
        "$root/docs/research/postgres-cast-final-audit.md" >/dev/null; then
        printf '%s\t%s\n' "$source" "$method"
      fi
    done)
if [ -n "$missing_declared_methods" ]; then
  printf 'method audit: declared methods missing from evidence documents:\n%s\n' "$missing_declared_methods" >&2
  exit 1
fi
rm -f "$catalog"
trap - EXIT HUP INT TERM
printf '%s\n' "method audit: direct/annotation declared methods ($declared_method_count methods)"

check_audit \
  bind \
  test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractBindTestWithFunctions.java \
  docs/research/postgres-bind-functions-coverage.md \
  96
check_audit \
  left-join \
  test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractLeftJoinProfTest.java \
  docs/research/left-join-prof-final-audit.md \
  52
check_audit \
  distinct-aggregate \
  test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractDistinctInAggregateTest.java \
  docs/research/postgres-distinct-aggregate-method-audit.md \
  4
check_audit \
  cast \
  test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/AbstractCastFunctionsTest.java \
  docs/research/postgres-cast-final-audit.md \
  86
check_audit \
  nested-data \
  test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/AbstractNestedDataTest.java \
  docs/research/postgres-nested-data-method-audit.md \
  10
check_audit \
  constraint \
  test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/AbstractConstraintTest.java \
  docs/research/postgres-constraint-method-audit.md \
  8
check_audit \
  jdbc-metadata \
  test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractDbMetadataInfoTest.java \
  docs/research/postgres-metadata-method-audit.md \
  1
