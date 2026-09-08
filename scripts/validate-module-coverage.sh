#!/usr/bin/env sh
set -eu

# #118：逐个已交付 Rust 生产模块强制行、函数覆盖率门槛。行覆盖率保持 90%，
# 函数覆盖率采用经批准的 85% 门槛。覆盖率 runner 负责
# 生成统一 profile；本脚本只裁决报告，因此报告缺失、结构改变、任何模块低于门槛
# 都失败关闭，不能由仓库总平均数掩盖。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
report=${1:-"$root/target/llvm-cov/minimal/coverage-summary.json"}
line_threshold=${RTOP_MODULE_LINE_COVERAGE_THRESHOLD:-90}
function_threshold=${RTOP_MODULE_FUNCTION_COVERAGE_THRESHOLD:-85}

[ -f "$report" ] || {
  echo "module coverage gate: report is missing: $report" >&2
  exit 1
}

jq -e --argjson line_threshold "$line_threshold" --argjson function_threshold "$function_threshold" '
  .data[0].files as $files
  | ($files | type == "array" and length > 0)
  and all($files[];
    (.filename | test("/src/[A-Za-z0-9_]+\\.rs$"))
    and (.summary.lines.percent >= $line_threshold)
    and (.summary.functions.percent >= $function_threshold)
  )
' "$report" >/dev/null || {
  echo "module coverage gate: each src/*.rs must meet ${line_threshold}% lines and ${function_threshold}% functions" >&2
  jq -r --argjson line_threshold "$line_threshold" --argjson function_threshold "$function_threshold" '
    .data[0].files[]
    | select(.filename | test("/src/[A-Za-z0-9_]+\\.rs$"))
    | select(.summary.lines.percent < $line_threshold or .summary.functions.percent < $function_threshold)
    | "\(.filename): lines=\(.summary.lines.percent)% functions=\(.summary.functions.percent)%"
  ' "$report" >&2
  exit 1
}

printf '%s\n' "module coverage gate: passed (${line_threshold}% lines / ${function_threshold}% functions per src module)"
