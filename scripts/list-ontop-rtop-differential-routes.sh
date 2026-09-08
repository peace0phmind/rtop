#!/usr/bin/env sh
set -eu

# 从 harness 的声明性 describe 模式导出差分 normalizer 路由。这个工具不启动
# PostgreSQL/Java/Rust，也不改写 artifact；动态 manifest 参数只从已保存的
# provenance case_id 提取，仍由 harness 自身白名单校验。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
harness="$root/scripts/test-ontop-rtop-differential.sh"
artifacts="$root/docs/research/differential-artifacts"

describe() {
  env DIFFERENTIAL_DESCRIBE_ONLY=true "$@" "$harness" 2>/dev/null || true
}

# 主 case 的顶级标签以 harness 的主 dispatch 为准。嵌套标签可能被一并捕获，
# 但 describe 会拒绝它们，因而不会产生错误路由。
awk '
  /^case "\$case_name" in$/ { active = 1 }
  active && /^  [a-z0-9][a-z0-9_-]*\)$/ {
    value = $0
    sub(/^  /, "", value)
    sub(/\)$/, "", value)
    print value
  }
  /^direct_mapping=\$\{direct_mapping/ { exit }
' "$harness" | sort -u | while IFS= read -r case_name; do
  describe DIFFERENTIAL_CASE="$case_name"
done

describe DIFFERENTIAL_CASE=d001 DIFFERENTIAL_D001_VARIANT=blank-node

route_dynamic_family() {
  family=$1
  prefix=$2
  env_name=$3
  find "$artifacts" -name provenance.json -print | while IFS= read -r provenance; do
    case_id=$(jq -r '.case_id' "$provenance")
    case "$case_id" in
      "$prefix"*)
        value=${case_id#"$prefix"}
        describe DIFFERENTIAL_CASE="$family" "$env_name=$value"
        ;;
    esac
  done
}

for query in $(seq 1 14); do
  describe DIFFERENTIAL_CASE=lubm "LUBM_QUERY=$query"
done
route_dynamic_family suitefilter 'differential-postgres-suite-manifest-filters-' SUITE_FILTER_QUERY
route_dynamic_family suitedatatype 'differential-postgres-suite-manifest-datatypes-' SUITE_DATATYPE_QUERY
route_dynamic_family suitemodifier 'differential-postgres-suite-manifest-modifiers-' SUITE_MODIFIER_QUERY
route_dynamic_family suitesimplecq 'differential-postgres-suite-manifest-simplecq-' SUITE_SIMPLECQ_QUERY
