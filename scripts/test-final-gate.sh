#!/usr/bin/env sh
set -eu

# PostgreSQL 17 发布前的本地/CI 同一入口。各子 gate 自行创建隔离容器并清理。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

run_gate() {
  gate=$1
  printf '[final-gate] start: %s\n' "$gate" >&2
  shift
  "$@"
  printf '[final-gate] passed: %s\n' "$gate" >&2
}

run_gate fmt cargo fmt --check
run_gate rust-tests cargo test --all --locked
run_gate baseline-discovery ./scripts/validate-postgres-baseline-discovery.sh
run_gate method-audits ./scripts/validate-postgres-method-audits.sh
run_gate coverage-ledger ./scripts/validate-coverage-ledger.sh
run_gate report-json jq empty compatibility-report.json
run_gate diff-check git diff --check
run_gate postgres-compat ./scripts/test-postgres-compat.sh
run_gate postgres-suite ./scripts/test-postgres-suite-compat.sh
run_gate postgres-lubm ./scripts/test-postgres-lubm-compat.sh
run_gate delivery ./scripts/test-delivery-compat.sh
run_gate oci ./scripts/test-oci-compat.sh

# 将会影响可观察对照的输入指纹写到 stdout，供 CI artifact 保存和差异审计。
sha256sum Cargo.lock Dockerfile compatibility-report.json \
  docs/research/postgres-only-coverage-ledger.json
