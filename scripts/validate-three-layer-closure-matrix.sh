#!/usr/bin/env sh
set -eu

# 该校验器故意把“矩阵结构有效”与“所有原子已经关闭”分开：前者可在
# #105 建立基础设施时验证；只有 --require-closed 才可成为最终发布 gate。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
matrix="$root/docs/research/three-layer-closure-matrix.json"
ledger="$root/docs/research/postgres-only-coverage-ledger.json"
report="$root/compatibility-report.json"
overrides="$root/docs/research/three-layer-differential-overrides.json"
normalizer="$root/scripts/normalize-ontop-rtop-differential.sh"
require_closed=${1:-}

jq -e '
  .schema_version == 1
  and .ontop_baseline == "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd"
  and (.generated_by == "scripts/generate-three-layer-closure-matrix.sh")
  and (.ledger_sha256 | test("^[0-9a-f]{64}$"))
  and (.coverage_evidence.line_and_function.status == "separate-gate-required")
  and (.coverage_evidence.line_and_function.provenance == "target/llvm-cov/minimal/provenance.json")
  and (.coverage_evidence.line_and_function.gate == "scripts/validate-module-coverage.sh")
  and (.coverage_evidence.branch.status == "external-tool-blocked")
  and (.coverage_evidence.branch.provenance == "target/llvm-cov/minimal/provenance.json")
  and (.coverage_evidence.branch.probe == "target/llvm-cov/minimal/branch-coverage-probe.json")
  and (.coverage_evidence.branch.scope == ["src/ontology.rs", "src/mapping.rs", "src/sparql.rs", "src/lib.rs"])
  and (.coverage_evidence.branch.required_gate_when_measurable == "ADR-0005: at least 80%")
  and (.atoms | type == "array" and length > 0)
  and (([.atoms[].id] | length) == ([.atoms[].id] | unique | length))
  and all(.atoms[];
    (.id | type == "string" and length > 0)
    and (.layer | IN("ontology", "mapping", "sparql"))
    and (.ontop_sources | type == "array" and length > 0 and all(.[]; type == "string" and length > 0))
    and (.behavior | type == "string" and length > 0)
    and (.rtop_implementation | type == "array" and length > 0)
    and (.differential_test_id | type == "string" and length > 0)
    and (.fixture | type == "string" and length > 0)
    and (.provenance.license | type == "string" and length > 0)
    and (.provenance.normalizer_sha256 | type == "string" and length > 0)
    and (.verdict | IN("pending", "passed", "failed", "baseline-ignored", "excluded", "environment-uncertain"))
    and (.coverage_test_ids | type == "array")
    and (.issue | type == "number")
    and (.ledger_case_id | type == "string" and length > 0)
  )
' "$matrix" >/dev/null

# 矩阵只能从严格账本的发现分母产生，并且必须在账本改变时重新生成；否则
# 删除某个难测原子或使用过期 discovery 输出都无法通过。
expected_ledger_sha256=$(sha256sum "$ledger" | awk '{print $1}')
[ -x "$normalizer" ]
expected_normalizer_sha256=$(sha256sum "$normalizer" | awk '{print $1}')
[ "$(jq -r '.ledger_sha256' "$matrix")" = "$expected_ledger_sha256" ]
jq -e --slurpfile ledger "$ledger" '
  ([.atoms[].id] | sort) == ([$ledger[0].entries[].asset_id] | sort)
  and all(.atoms[]; . as $atom | any($ledger[0].entries[]; .asset_id == $atom.id and .rtop_case_id == $atom.ledger_case_id and .issue == $atom.issue))
' "$matrix" >/dev/null
jq -e --slurpfile matrix "$matrix" '
  all($matrix[0].atoms[];
    if .verdict == "passed" then
      (.differential_artifacts | type == "string" and length > 0)
      and (.coverage_test_ids | length > 0)
      and (.provenance.license | startswith("Apache-2.0"))
      and (.provenance.normalizer_sha256 | test("^[0-9a-f]{64}$"))
    else true end)
' "$overrides" >/dev/null

# 通过的双边原子必须附有可复核的实际 artifact；不能以 overrides 中的文字
# 声明替代两个原始输出、规范化输出与 provenance 的 hash。
jq -r '.atoms[] | select(.verdict == "passed") | [.id, .differential_test_id, .differential_artifacts, .provenance.normalizer_sha256] | @tsv' "$matrix" \
  | while IFS="$(printf '\t')" read -r atom_id case_id artifact normalizer_hash; do
      artifact_path="$root/$artifact"
      artifact_dir=$(dirname "$artifact_path")
      [ "$normalizer_hash" = "$expected_normalizer_sha256" ]
      jq -e --arg case_id "$case_id" '
        .status == "passed"
        and .case_id == $case_id
        and .ontop_commit == "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd"
        and (.postgres_image_digest == .required_postgres_image_digest)
        and (.sha256.ontop_raw | test("^[0-9a-f]{64}$"))
        and (.sha256.rtop_raw | test("^[0-9a-f]{64}$"))
        and (.sha256.normalized | test("^[0-9a-f]{64}$"))
      ' "$artifact_path" >/dev/null
      jq -r '.sha256 | [.ontop_raw, .rtop_raw, .normalized] | @tsv' "$artifact_path" \
        | while IFS="$(printf '\t')" read -r ontop_hash rtop_hash normalized_hash; do
            [ "$(sha256sum "$artifact_dir/ontop.raw" | awk '{print $1}')" = "$ontop_hash" ]
            [ "$(sha256sum "$artifact_dir/rtop.raw" | awk '{print $1}')" = "$rtop_hash" ]
            [ "$(sha256sum "$artifact_dir/normalized.json" | awk '{print $1}')" = "$normalized_hash" ]
          done
    done
jq -e --slurpfile matrix "$matrix" '
  .cases as $cases
  | all($matrix[0].atoms[];
      . as $atom
      | any($cases[]; .id == $atom.ledger_case_id and .status == "passed"))
' "$report" >/dev/null

if [ "$require_closed" = "--require-closed" ]; then
  jq -e 'all(.atoms[]; .verdict == "passed" or .verdict == "baseline-ignored" or .verdict == "excluded")' "$matrix" >/dev/null
fi

printf '%s\n' 'three-layer closure matrix: structurally valid'
