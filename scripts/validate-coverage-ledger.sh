#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ledger="$root/docs/research/postgres-only-coverage-ledger.json"
report="$root/compatibility-report.json"

jq -e '
  .schema_version == 1
  and (.ontop_baseline | type == "string" and length > 0)
  and (.entries | type == "array" and length > 0)
  and (([.entries[].asset_id] | length) == ([.entries[].asset_id] | unique | length))
  and all(.entries[]; (.in_scope | not) or .status == "passed")
  and all(.entries[];
    (.asset_id | type == "string" and length > 0)
    and (.source_path | type == "string" and length > 0)
    and (.baseline_asset | type == "string" and length > 0)
    and (.scope | type == "string" and length > 0)
    and (.in_scope | type == "boolean")
    and (.status | type == "string" and length > 0)
    and (.status_reason | type == "string" and length > 0)
    and (.assertion_strength | IN("full-term", "boolean", "graph", "cardinality-only", "error-category", "graph/error-category", "value-equivalent-graph"))
    and (.rtop_case_id | type == "string" and length > 0)
    and (.reference_result.source | type == "string" and length > 0)
    and (.reference_result.sha256 | test("^[0-9a-f]{64}$"))
    and (.ontop_commit == "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd")
    and (.environment.database_image_digest | startswith("postgres:17@sha256:"))
    and (.environment.initialization_sql_sha256 | test("^[0-9a-f]{64}$"))
    and (.environment.timeout_seconds | type == "number" and . > 0)
    and (.environment.cleanup_strategy | type == "string" and length > 0)
    and (.command | type == "string" and length > 0)
    and (.issue | type == "number")
    and (.reviewed_on | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
  )
' "$ledger" >/dev/null

# PostgreSQL 范围保留的 11 个非元 CLI 命令不能只凭文档叙述完成：每个
# 基线 command asset 必须以一个 passed 账本条目出现。四个 mapping 命令
# 分别以其独立的 PostgreSQL reload case 作为代表资产。
jq -e '
  .entries as $entries
  | ["cli-query", "cli-materialize", "cli-bootstrap", "cli-validate", "cli-endpoint", "cli-extract-db-metadata", "cli-compile", "cli-pretty-r2rml", "cli-v1-to-v3-native-obda", "cli-r2rml-to-obda-d001-reload", "cli-obda-to-r2rml-force-reload"]
  | all(.[]; . as $asset_id | any($entries[]; .asset_id == $asset_id and .status == "passed"))
' "$ledger" >/dev/null

# 映射互操作的四个命令还需要保留其不能由一个 happy path 替代的原子：
# 格式化、native/R2RML alias、相对 IRI、具名图、BlankNode/datatype、旧格式
# 失败和重复 mappingId。每项都必须有可重新加载的 PostgreSQL 对照证据。
jq -e '
  .entries as $entries
  | ["cli-pretty-r2rml", "cli-v1-to-v3-native-obda", "cli-r2rml-to-obda-d001-reload", "cli-obda-to-r2rml-force-reload", "cli-obda-to-r2rml-force-named-graph", "cli-obda-to-r2rml-force-template-graph", "cli-v1-to-v3-duplicate-qualified-alias", "cli-v1-to-v3-simplify-projection", "cli-v1-to-v3-r2rml-qualified-alias", "cli-obda-to-r2rml-typed-blank-node", "cli-obda-to-r2rml-legacy-source-declaration-error", "cli-obda-to-r2rml-duplicate-mapping-id", "cli-r2rml-to-obda-relative-iri"]
  | all(.[]; . as $asset_id | any($entries[]; .asset_id == $asset_id and .status == "passed"))
' "$ledger" >/dev/null

# adapter 生命周期必须在真正的 PostgreSQL 容器而非跳过环境变量的单元测试中
# 验证：提前停止流和取消均要证明同一连接仍可继续请求。
jq -e '
  .entries as $entries
  | ["postgres-adapter-stream-early-stop-resource-release", "postgres-adapter-cancellation-stable-diagnostic-and-reuse"]
  | all(.[]; . as $asset_id | any($entries[]; .asset_id == $asset_id and .status == "passed"))
' "$ledger" >/dev/null

# #44 的 PostgreSQL 类型、标识符和复杂值范围不能只由 compatibility report
# 摘要声明：这些原子分别钉住 scalar datatype、identifier folding/quoted alias、
# 正负 cast、服务器 regex、JSON/JSONB/array、动态 mapping expansion，以及
# Direct Mapping 的 identifier/datatype RDF 图。
jq -e '
  .entries as $entries
  | ([.entries[] | select(.issue == 44 and (.asset_id | startswith("postgres-datatype-manifest-")))] | length == 34)
  and (["postgres-unquoted-identifier-folding", "postgres-quoted-identifier-alias", "postgres-cast-lexical-and-invalid-input", "postgres-source-sql-regex-negation", "postgres-nested-json-jsonb-array-aggregate", "postgres-epnet-dynamic-meta-mapping-expansion", "postgres-direct-d010-identifier-percent-encoding", "postgres-direct-d016-sql-datatypes-binary"]
  | all(.[]; . as $asset_id | any($entries[]; .asset_id == $asset_id and .status == "passed" and .issue == 44))
  )
' "$ledger" >/dev/null

# #43 必须有真实 PostgreSQL adapter 的本体资产：imports closure 的 subclass
# 改写、domain/range fact 推理、mapping subProperty/inverse，以及 disjoint 的
# 稳定输入拒绝；不能退回只有内存 FakeSource 的单元测试证据。
jq -e '
  .entries as $entries
  | ["postgres-ontology-imported-subclass-rewrite", "postgres-ontology-domain-range-facts", "postgres-ontology-subproperty-inverse-rewrite", "postgres-ontology-disjoint-inconsistency"]
  | all(.[]; . as $asset_id | any($entries[]; .asset_id == $asset_id and .status == "passed" and .issue == 43))
' "$ledger" >/dev/null

jq -e '.entries as $e | ["postgres-facts-turtle-mapping-union","postgres-facts-nquads-named-graph","postgres-facts-rdfxml-explicit-base","postgres-facts-missing-malformed-error"] | all(.[]; . as $id | any($e[]; .asset_id == $id and .issue == 42 and .status == "passed"))' "$ledger" >/dev/null

jq -e '.entries as $e | any($e[]; .asset_id == "postgres-oci-no-jvm-default-endpoint-cli-secret-healthcheck" and .issue == 49 and .status == "passed")' "$ledger" >/dev/null

# 发现器选中的 18 个直接 Docker PostgreSQL class 与 10 个 lightweight
# PostgreSQL annotation class 都是严格继承分母。每个 class source 必须由至少一个
# passed report case 精确引用；只在自由文本或父类方法审计中出现不足以证明该类仍被验收。
"$root/scripts/discover-postgres-baseline-assets.sh" \
  | jq -r '(.direct_test_class_assets + .annotated_test_class_assets)[] | select(.classification == "in-scope") | [.asset_id, .source_path] | @tsv' \
  | while IFS="$(printf '\t')" read -r asset_id source_path; do
      jq -e --arg source_path "$source_path" '
        any(.cases[]; .status == "passed" and any(.source_paths[]?; . == $source_path))
      ' "$report" >/dev/null || {
        printf 'coverage ledger: missing PostgreSQL test-class report evidence: %s (%s)\n' "$asset_id" "$source_path" >&2
        exit 1
      }
    done

# #52 的分母是固定 LUBM PostgreSQL manifest 声明的 14 个 query entry。每个
# 原始 .rq 都须有单独账本资产，避免把整组 cardinality gate 记成一个笼统的成功。
lubm_manifest=test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/manifest-pgsql.ttl
lubm_queries=$(git -C "$root/../ontop" show "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd:$lubm_manifest" \
  | awk '/mf:entries/ { entries = 1 } entries { print } entries && /\)[[:space:]]*\./ { exit }' \
  | rg -o ':query-[0-9]+' | sed 's/^://')
[ "$(printf '%s\n' "$lubm_queries" | sed '/^$/d' | wc -l | tr -d ' ')" = 14 ]
[ "$(printf '%s\n' "$lubm_queries" | sort -u | wc -l | tr -d ' ')" = 14 ]
printf '%s\n' "$lubm_queries" | while IFS= read -r query; do
  jq -e --arg source_path "test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm/$query.rq" '
    any(.entries[]; .issue == 52 and .status == "passed" and .source_path == $source_path)
  ' "$ledger" >/dev/null || {
    printf 'coverage ledger: #52 missing LUBM query evidence: %s\n' "$query" >&2
    exit 1
  }
done
[ "$(jq '[.entries[] | select(.issue == 52 and .status == "passed")] | length' "$ledger")" = 14 ]

# Docker PostgreSQL manifest 的严格继承单位是每个 qt:query，而不是 manifest
# 文件或某个聚合 smoke case。除 Java 显式 IGNORE 的 general-Type: all 之外，170
# 条固定基线 query 都必须有独立、通过的账本资产；其中 122 条 DockerPostgresTestSuite
# stockexchange/ASK 实例由动态 manifest gate 覆盖。
docker_manifest_queries=$(git -C "$root/../ontop" ls-tree -r --name-only 5ec07573b18513f33dfcd59ac45fe26a81f9cdbd -- test/docker-tests/src/test/resources/testcases-docker \
  | rg 'manifest-pgsql\.ttl$' \
  | while IFS= read -r manifest; do
      case "$manifest" in
        test/docker-tests/src/test/resources/testcases-docker/general/manifest-pgsql.ttl) continue ;;
      esac
      directory=$(dirname "$manifest")
      git -C "$root/../ontop" show "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd:$manifest" \
        | sed -n 's/.*qt:query <\([^>]*\)>.*/\1/p' \
        | while IFS= read -r query; do printf '%s/%s\n' "$directory" "$query"; done
    done)
[ "$(printf '%s\n' "$docker_manifest_queries" | sed '/^$/d' | wc -l | tr -d ' ')" = 170 ]
[ "$(printf '%s\n' "$docker_manifest_queries" | sort -u | wc -l | tr -d ' ')" = 170 ]
printf '%s\n' "$docker_manifest_queries" | while IFS= read -r source_file; do
  jq -e --arg source_file "$source_file" '
    any(.entries[]; .status == "passed" and .source_path == $source_file)
  ' "$ledger" >/dev/null || {
    printf 'coverage ledger: missing Docker PostgreSQL manifest query evidence: %s\n' "$source_file" >&2
    exit 1
  }
done
[ "$(jq '[.entries[] | select(.asset_id | startswith("postgres-suite-manifest-"))] | length' "$ledger")" = 122 ]

# 固定 PostgreSQL manifest 的每份 source 都必须留下可查询证据。执行型 manifest
# 需要 compatibility report 的 passed case；被 Java 参数化测试显式忽略的 general
# Type: all 则必须有 baseline-ignored case，不能伪装成通过或静默漏掉。
"$root/scripts/discover-postgres-baseline-assets.sh" \
  | jq -r '.docker_postgresql_manifest_entries[] | [.classification, .source_path] | @tsv' \
  | while IFS="$(printf '\t')" read -r classification source_path; do
      case "$classification" in
        in-scope) required_status=passed ;;
        baseline-ignored) required_status=baseline-ignored ;;
        *)
          printf 'coverage ledger: unknown PostgreSQL manifest classification: %s\n' "$classification" >&2
          exit 1 ;;
      esac
      jq -e --arg source_path "$source_path" --arg status "$required_status" '
        any(.cases[]; .status == $status and any(.source_paths[]?; . == $source_path))
      ' "$report" >/dev/null || {
        printf 'coverage ledger: missing manifest evidence: %s (%s)\n' "$source_path" "$required_status" >&2
        exit 1
      }
    done

# 发现器中的 in-scope 开发诊断控制器不能只在 report 中出现。它保留的
# Query-ID/header 是独立 HTTP 可观察契约，须有 PostgreSQL 服务端账本资产。
jq -e '.entries as $e | any($e[]; .asset_id == "http-development-reformulate-query-id" and .issue == 48 and .status == "passed" and .assertion_strength == "full-term")' "$ledger" >/dev/null

# 发现器中的每项 in-scope delivery source 都是独立外部契约锚点。具体 command
# 可以共享一个 PostgreSQL case，但不能只在自由文本或子类名称中出现。
"$root/scripts/discover-postgres-baseline-assets.sh" \
  | jq -r '.delivery_assets[] | select(.classification == "in-scope") | [.asset_id, .source_path] | @tsv' \
  | while IFS="$(printf '\t')" read -r asset_id source_file; do
      jq -e --arg source_file "$source_file" '
        any(.entries[]; .status == "passed" and .source_path == $source_file)
      ' "$ledger" >/dev/null || {
        printf 'coverage ledger: missing in-scope delivery source evidence: %s (%s)\n' "$asset_id" "$source_file" >&2
        exit 1
      }
    done

# #41 的 native OBDA 证据必须分别钉住 prefix/合法 PostgreSQL source 的 RDF term，
# parser 与 target reader 的加载期拒绝，以及合法 source 的 PostgreSQL 执行期失败。
jq -e '.entries as $e | ["postgres-native-obda-prefix-declaration-iri-term","postgres-native-obda-legal-source-lower-term","postgres-native-obda-invalid-source-sql","postgres-native-obda-reader-missing-target","postgres-native-obda-runtime-source-relation-error"] | all(.[]; . as $id | any($e[]; .asset_id == $id and .issue == 41 and .status == "passed"))' "$ledger" >/dev/null

# #40 的分母是固定基线所有 64 个 r2rml*.ttl mapping 文件，而不是 21 个
# manifest 目录或 59 个可合并的账本原子。每个文件必须至少被一个 issue 40
# 条目的 source_path 指向；花括号形式的 source_path 允许把同一验收原子中的
# 多个 mapping/expected 资产并列记录。
r2rml_mappings=$(git -C "$root/../ontop" ls-tree -r --name-only 5ec07573b18513f33dfcd59ac45fe26a81f9cdbd -- test/rdb2rdf-compliance/src/test/resources \
  | rg '/r2rml[a-z]*\.ttl$')
[ "$(printf '%s\n' "$r2rml_mappings" | sed '/^$/d' | wc -l | tr -d ' ')" = 64 ]
printf '%s\n' "$r2rml_mappings" | while IFS= read -r mapping; do
  directory=$(dirname "$mapping")/
  filename=$(basename "$mapping")
  jq -e --arg directory "$directory" --arg filename "$filename" '
    any(.entries[];
      .issue == 40 and .status == "passed"
      and (.source_path | contains($directory))
      and (.source_path | contains($filename)))
  ' "$ledger" >/dev/null || {
    printf 'coverage ledger: #40 missing R2RML mapping evidence: %s\n' "$mapping" >&2
    exit 1
  }
done

jq -e --slurpfile ledger "$ledger" '
  .cases as $report_cases
  | [ $ledger[0].entries[].rtop_case_id as $id
    | select(any($report_cases[]; .id == $id and .status == "passed"))
  ] | length == ($ledger[0].entries | length)
' "$report" >/dev/null

# `reference_result.sha256` is evidence only when it still describes the named
# asset.  Resolve local generated fixtures first and then the pinned, read-only
# Ontop checkout for baseline-relative paths.  This makes an edited expected
# result or a different baseline worktree fail the final gate instead of merely
# carrying a well-formed but stale digest.
jq -r '.entries[] | [.asset_id, .reference_result.source, .reference_result.sha256] | @tsv' "$ledger" \
  | while read -r asset_id source expected_hash; do
      candidate="$root/$source"
      if [ ! -f "$candidate" ]; then
        candidate="$root/../ontop/$source"
      fi
      if [ ! -f "$candidate" ]; then
        printf 'coverage ledger: missing reference asset for %s: %s\n' "$asset_id" "$source" >&2
        exit 1
      fi
      actual_hash=$(sha256sum "$candidate" | awk '{print $1}')
      if [ "$actual_hash" != "$expected_hash" ]; then
        printf 'coverage ledger: stale reference hash for %s: %s\n' "$asset_id" "$source" >&2
        exit 1
      fi
done

# #51 的 Direct Mapping 分母按 manifest entry 计，而不是目录：D005 与 D012
# 各有 standard/modified 两项，因此 26 个 entry 必须有 26 个独立 passed asset。
jq -e '[.entries[] | select(.issue == 51)] | length == 26 and all(.[]; .status == "passed")' "$ledger" >/dev/null
jq -e '.entries as $e | any($e[]; .asset_id == "direct-mapping-d012-modified-double-value-equivalent" and .issue == 51 and .assertion_strength == "value-equivalent-graph")' "$ledger" >/dev/null
direct_mapping_outputs=$(for manifest in $(git -C "$root/../ontop" ls-tree -r --name-only 5ec07573b18513f33dfcd59ac45fe26a81f9cdbd -- test/rdb2rdf-compliance/src/test/resources | rg '/manifest\.ttl$'); do
  directory=$(dirname "$manifest")
  git -C "$root/../ontop" show "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd:$manifest" \
    | awk -v directory="$directory" '
        /a rdb2rdftest:DirectMapping/ { direct = 1 }
        direct && /rdb2rdftest:output "directGraph/ {
          output = $0
          sub(/^.*rdb2rdftest:output "/, "", output)
          sub(/".*$/, "", output)
          print directory "/" output
        }
        direct && /^\.$/ { direct = 0 }
      '
done | sort)
[ "$(printf '%s\n' "$direct_mapping_outputs" | sed '/^$/d' | wc -l | tr -d ' ')" = 26 ]
printf '%s\n' "$direct_mapping_outputs" | while IFS= read -r output; do
  directory=$(dirname "$output")/
  filename=$(basename "$output")
  jq -e --arg directory "$directory" --arg filename "$filename" '
    any(.entries[];
      .issue == 51 and .status == "passed"
      and (.source_path | contains($directory))
      and (.source_path | contains($filename)))
  ' "$ledger" >/dev/null || {
    printf 'coverage ledger: #51 missing Direct Mapping evidence: %s\n' "$output" >&2
    exit 1
  }
done

printf '%s\n' "coverage ledger: valid ($(jq '.entries | length' "$ledger") entries; strict in-scope mode)"
