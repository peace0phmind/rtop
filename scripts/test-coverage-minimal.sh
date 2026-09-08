#!/usr/bin/env sh
set -eu

# #110 的最小统一插桩切片：同一 LLVM_PROFILE_FILE 先运行 Rust 回归，后运行
# 已插桩 rtop 二进制的真实 PostgreSQL CLI 查询。任何一个阶段没有 profile 都
# 失败关闭，报告只从这组 profile 生成。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

command -v cargo >/dev/null 2>&1 && cargo llvm-cov --version >/dev/null 2>&1 || {
  echo "coverage runner: cargo-llvm-cov is required" >&2
  exit 1
}
command -v docker >/dev/null 2>&1 || {
  echo "coverage runner: docker is required for the PostgreSQL CLI slice" >&2
  exit 1
}

cargo llvm-cov clean --workspace
# clean 会移除 target/llvm-cov；必须在它之后创建报告目录，否则 runner 虽执行成功，
# provenance/summary 会在生成前被删除，严格 gate 只能看到假性“报告缺失”。
output_dir=${RTOP_COVERAGE_OUTPUT_DIR:-"$root/target/llvm-cov/minimal"}
rm -rf "$output_dir"
mkdir -p "$output_dir"
# 两个 cargo llvm-cov 子命令共用它维护的 llvm-cov-target profile 目录；手工
# 拼接 RUSTFLAGS 曾导致普通 cargo test 未生成 profile，因而禁止这样做。
profile_dir="$root/target/llvm-cov-target"

profile_count() {
  find "$profile_dir" -name '*.profraw' -type f -print | wc -l | tr -d ' '
}

# `--all` 只选择 workspace package，未承诺执行 binary test target。门禁覆盖
# `src/main.rs`，故必须把 bin/integration target 同时纳入同一 profile collection。
cargo llvm-cov --no-report --all --all-targets --locked
RTOP_COVERAGE_PROFILE_DIR="$profile_dir" \
  "$root/scripts/test-coverage-postgres-adapter.sh"
test_profiles=$(profile_count)
[ "$test_profiles" -gt 0 ] || {
  echo "coverage runner: Rust tests did not produce LLVM profiles" >&2
  exit 1
}

RTOP_COVERAGE_USE_CARGO_LLVM_COV=1 \
RTOP_COVERAGE_PROFILE_DIR="$profile_dir" \
  "$root/scripts/test-coverage-postgres-cli.sh"

# `cargo llvm-cov run` 无法在一个常驻 HTTP server 的启动后保留调用控制；这里直接
# 运行同一 target 中的已插桩二进制，并显式沿用 cargo-llvm-cov 的 profile pattern。
# cargo-llvm-cov 0.9 uses a `%20m` merge pool.  The old `%m` pattern still
# produced raw files, but its report collector did not discover profiles from
# the manually started endpoint, silently omitting the real HTTP execution.
RTOP_COVERAGE_BINARY="$profile_dir/debug/rtop" \
RTOP_COVERAGE_PROFILE_DIR="$profile_dir" \
LLVM_PROFILE_FILE="$profile_dir/rtop-%p-%20m.profraw" \
  "$root/scripts/test-coverage-delivery.sh"

all_profiles=$(profile_count)
[ "$all_profiles" -gt "$test_profiles" ] || {
  echo "coverage runner: PostgreSQL CLI profiles were not merged into the collection" >&2
  exit 1
}

cargo llvm-cov report --json --summary-only \
  --output-path "$output_dir/coverage-summary.json"
cargo llvm-cov report --text --show-missing-lines \
  --output-path "$output_dir/coverage-missing-lines.txt"
jq -e '.data[0].files | length > 0' "$output_dir/coverage-summary.json" >/dev/null

# ADR-0005 的三层分支门槛不能由行/函数覆盖替代。当前 Rust/LLVM 工具链不导出
# 可统计的 branch denominator；将原始探针与该裁决一同保存。若将来工具开始导出
# 非零分母，本脚本刻意失败，要求改为真实的 80% branch gate，不能静默沿用阻断。
branch_probe="$output_dir/branch-coverage-probe.json"
cargo llvm-cov report --json --output-path "$branch_probe"
jq -e '
  [.data[0].files[]
   | select(.filename | test("/src/(ontology|mapping|sparql|lib)\\.rs$"))
   | .summary.branches.count]
  | length == 4 and all(.[]; . == 0)
' "$branch_probe" >/dev/null || {
  echo "coverage runner: Rust branch denominator changed; establish the ADR-0005 80% gate" >&2
  exit 1
}

cargo_llvm_cov_version=$(cargo llvm-cov --version)
rustc_version=$(rustc -Vv)
jq -n \
  --arg schema_version "1" \
  --arg cargo_llvm_cov "$cargo_llvm_cov_version" \
  --arg rustc "$rustc_version" \
  --arg profile_pattern "$profile_dir/rtop-%p-%20m.profraw" \
  --arg target_dir "$profile_dir" \
  --arg branch_probe "branch-coverage-probe.json" \
  --argjson rust_test_profiles "$test_profiles" \
  --argjson all_profiles "$all_profiles" \
  --arg rust_command "cargo test --all --all-targets --locked" \
  --arg cli_command "cargo llvm-cov run --no-clean --bin rtop (postgres:17, fixed postgres-minimal Ontop fixture)" \
  --arg delivery_command "instrumented rtop binary (postgres:17, fixed CLI/HTTP delivery fixture)" \
  '{schema_version: ($schema_version | tonumber), cargo_llvm_cov: $cargo_llvm_cov,
    rustc: $rustc, profile_pattern: $profile_pattern, target_dir: $target_dir,
    rust_test_profiles: $rust_test_profiles, all_profiles: $all_profiles,
    commands: [$rust_command, $cli_command, $delivery_command],
    branch_coverage: {
      status: "external-tool-blocked",
      reason: "cargo-llvm-cov exports a zero Rust branch denominator for ontology, mapping, sparql, and runtime",
      probe: $branch_probe,
      required_gate: "ADR-0005 requires at least 80% once a reliable denominator exists"
    }, complete: true}' \
  > "$output_dir/provenance.json"

printf '%s\n' "coverage runner: passed; reports in $output_dir"
