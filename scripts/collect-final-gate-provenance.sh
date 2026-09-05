#!/usr/bin/env sh
set -eu

# 生成 CI artifact 的机器可读 provenance。它故意在最终 gate 后执行：只有完整
# gate 成功且本地 Docker 中仍有实际使用的 postgres:17 镜像时，才产生发布证据。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
baseline="$root/../ontop"
expected_commit=5ec07573b18513f33dfcd59ac45fe26a81f9cdbd
ledger="$root/docs/research/postgres-only-coverage-ledger.json"

[ "$(git -C "$baseline" rev-parse HEAD)" = "$expected_commit" ]

# 所有 PostgreSQL-backed passed asset 必须指向同一个已认证 digest。不能从 tag
# 推断结果，以免 registry 更新后 artifact 仍看似可信。
expected_digest=$(jq -r '
  [.entries[]
   | select(.in_scope and .status == "passed")
   | .environment.database_image_digest]
  | unique
  | if length == 1 then .[0] else error("expected exactly one PostgreSQL digest") end
' "$ledger")
actual_digests=$(docker image inspect postgres:17 \
  --format '{{range .RepoDigests}}{{println .}}{{end}}' | sort -u)
expected_content_digest=${expected_digest#*@}
printf '%s\n' "$actual_digests" | awk -F@ -v digest="$expected_content_digest" '$2 == digest { found = 1 } END { exit !found }'

hash() {
  sha256sum "$root/$1" | awk '{print $1}'
}

jq -n \
  --arg generated_by "scripts/collect-final-gate-provenance.sh" \
  --arg ontop_commit "$expected_commit" \
  --arg rtop_commit "$(git -C "$root" rev-parse HEAD)" \
  --arg postgres_17_digest "$expected_digest" \
  --argjson observed_image_digests "$(printf '%s\n' "$actual_digests" | jq -Rsc 'split("\n") | map(select(length > 0))')" \
  --arg cargo_lock_sha256 "$(hash Cargo.lock)" \
  --arg dockerfile_sha256 "$(hash Dockerfile)" \
  --arg compatibility_report_sha256 "$(hash compatibility-report.json)" \
  --arg coverage_ledger_sha256 "$(hash docs/research/postgres-only-coverage-ledger.json)" \
  '{schema_version: 1, generated_by: $generated_by, ontop_commit: $ontop_commit,
    rtop_commit: $rtop_commit, postgres_17_digest: $postgres_17_digest,
    observed_image_digests: $observed_image_digests,
    evidence_sha256: {Cargo_lock: $cargo_lock_sha256, Dockerfile: $dockerfile_sha256,
      compatibility_report: $compatibility_report_sha256,
      coverage_ledger: $coverage_ledger_sha256}}'
