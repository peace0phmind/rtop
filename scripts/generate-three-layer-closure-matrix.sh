#!/usr/bin/env sh
set -eu

# 闭合矩阵的发现分母与已严格校验的 PostgreSQL coverage ledger 共享同一
# asset_id 集合。生成阶段绝不把 ledger 的单边 passed 状态提升为差分 passed：
# 每个原子一律从 pending 开始，必须由 #106 及后续子票写入双边证据后才可关闭。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ledger="$root/docs/research/postgres-only-coverage-ledger.json"
matrix="$root/docs/research/three-layer-closure-matrix.json"
overrides="$root/docs/research/three-layer-differential-overrides.json"

jq -n \
  --arg baseline "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd" \
  --arg generated_by "scripts/generate-three-layer-closure-matrix.sh" \
  --arg ledger_sha256 "$(sha256sum "$ledger" | awk '{print $1}')" \
  --slurpfile ledger "$ledger" --slurpfile overrides "$overrides" '
  def layer_for($scope; $source):
    if (($scope + " " + $source) | test("ontology|owlapi|owl"; "i")) then "ontology"
    elif (($scope + " " + $source) | test("mapping|r2rml|direct-mapping|rdb2rdf"; "i")) then "mapping"
    else "sparql"
    end;
  def implementation_for($layer):
    if $layer == "ontology" then ["src/ontology.rs", "src/lib.rs:111"]
    elif $layer == "mapping" then ["src/mapping.rs", "src/lib.rs:111"]
    else ["src/sparql.rs:220", "src/lib.rs:111"]
    end;
  {
    schema_version: 1,
    ontop_baseline: $baseline,
    generated_by: $generated_by,
    ledger_sha256: $ledger_sha256,
    purpose: "#104 的唯一三层完成状态源。矩阵从已验证 PostgreSQL 发现集生成，但单边账本 passed 不等同于 Ontop-vs-rtop 差分 passed。",
    coverage_evidence: {
      line_and_function: {
        status: "separate-gate-required",
        provenance: "target/llvm-cov/minimal/provenance.json",
        gate: "scripts/validate-module-coverage.sh"
      },
      branch: {
        status: "external-tool-blocked",
        provenance: "target/llvm-cov/minimal/provenance.json",
        probe: "target/llvm-cov/minimal/branch-coverage-probe.json",
        scope: ["src/ontology.rs", "src/mapping.rs", "src/sparql.rs", "src/lib.rs"],
        required_gate_when_measurable: "ADR-0005: at least 80%"
      }
    },
    atoms: [
      $ledger[0].entries[]
      | . as $entry
      | (layer_for($entry.scope; $entry.source_path)) as $layer
      | ({
          id: $entry.asset_id,
          layer: $layer,
          ontop_sources: ($entry.source_path | split("; ")),
          behavior: $entry.baseline_asset,
          rtop_implementation: implementation_for($layer),
          differential_test_id: ("pending-differential-" + $entry.asset_id),
          fixture: ("pending: derive from " + $entry.rtop_case_id),
          provenance: {license: "pending: inherit fixed Ontop asset provenance", normalizer_sha256: "pending"},
          verdict: "pending",
          failure_fingerprint: null,
          coverage_test_ids: [],
          issue: $entry.issue,
          ledger_case_id: $entry.rtop_case_id
        } + ($overrides[0][$entry.asset_id] // {}))
    ]
  }
' > "$matrix"
