#!/usr/bin/env sh
set -eu

# PostgreSQL 17 发布前的本地/CI 同一入口。各子 gate 自行创建隔离容器并清理。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

cargo fmt --check
cargo test --all --locked
./scripts/validate-postgres-baseline-discovery.sh
./scripts/validate-postgres-method-audits.sh
./scripts/validate-coverage-ledger.sh
jq empty compatibility-report.json
git diff --check
./scripts/test-postgres-compat.sh
./scripts/test-postgres-suite-compat.sh
./scripts/test-postgres-lubm-compat.sh
./scripts/test-delivery-compat.sh
./scripts/test-oci-compat.sh

# 将会影响可观察对照的输入指纹写到 stdout，供 CI artifact 保存和差异审计。
sha256sum Cargo.lock Dockerfile compatibility-report.json \
  docs/research/postgres-only-coverage-ledger.json
