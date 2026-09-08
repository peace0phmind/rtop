#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ledger="$root/docs/research/postgres-only-coverage-ledger.json"
report="$root/compatibility-report.json"

require_replay_field() {
  field=$1
  expected=$2
  predicate=$3
  invalid_assets=$(jq -r "$predicate" "$ledger")
  if [ -n "$invalid_assets" ]; then
    printf '%s\n' "$invalid_assets" | while IFS= read -r asset_id; do
      printf 'coverage ledger: invalid replay: asset_id=%s field=%s expected=%s\n' "$asset_id" "$field" "$expected" >&2
    done
    exit 1
  fi
}

jq -e '
  .schema_version == 1
  and (.ontop_baseline | type == "string" and length > 0)
  and (.replacement_kernel.consumer == "外部 endpoint 兼容 profile（无运行时或测试反向依赖）")
  and (.replacement_kernel.acceptance_seam == "原生 rtop PostgreSQL SPARQL HTTP endpoint")
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
    and (.reference_result.sha256 | test("^[0-9a-f]{64}(;[0-9a-f]{64})*$"))
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

# shell command 是人工审计记录；凡由 Ontop-vs-rtop 差分 runner 产生的证据，都
# 必须以不含运行时路径的结构化 replay 描述其稳定身份。
require_replay_field 'replay' 'runner、非空 cases 和 parameters 对象' '
  .entries[]
  | select(.command | contains("./scripts/test-ontop-rtop-differential.sh"))
  | select((.replay | type) != "object"
      or (.replay.runner != "./scripts/test-ontop-rtop-differential.sh")
      or ((.replay.cases | type) != "array")
      or ((.replay.cases | length) == 0)
      or any(.replay.cases[]; (type != "string") or length == 0)
      or ((.replay.parameters | type) != "object"))
  | .asset_id
'
require_replay_field 'replay' '不得包含 /tmp 运行时路径' '
  .entries[]
  | select(.replay? and (.replay | tostring | contains("/tmp/")))
  | .asset_id
'

# #54 建立替换内核账本入口。它必须固定 Ontop endpoint controller 源码、真实
# PostgreSQL endpoint case 与三种 HTTP 请求形状的可观察结果；不得把 Rust
# 内部结构、SQL 文本或普通 CLI 成功冒充为 endpoint 证据。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-replacement-http-protocol-anchor"
      and .issue == 54
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/http/query"
      and .assertion_strength == "boolean"
      and .rtop_case_id == "postgres-http-sparql-get-and-post-protocol"
      and .source_path == "client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/SparqlQueryController.java"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #86 使用 rtop 版本控制的固定 FIBO mapping、ontology、schema 和查询资产；余额、
# 放款、归属、货币反例与两种 ROUND 结果均在 HTTP JSON seam 比较。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-fibo-main-postgres-endpoint"
      and .issue == 86
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/integration/fibo-main"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "intent-engine-fibo-main-postgres-endpoint"
      and .reference_result.source == "tests/compat/postgres-fibo/fixtures/generated/query.sparql"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #87 必须保留 FIBO 本体和身份的正确结果及四个反例：删除 subclass、错误
# equivalentClass、删除 weak alignment 和 unsafe owl:sameAs 都经真实 HTTP endpoint
# 比较，不能以静态 ontology 审阅替代。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-fibo-ontology-identity-counterexamples-postgres-endpoint"
      and .issue == 87
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/integration/fibo-ontology-identity-counterexamples"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "intent-engine-fibo-ontology-identity-counterexamples-postgres-endpoint"
      and .reference_result.source == "tests/compat/postgres-fibo/fixtures/generated/fibo-loan-count-query.sparql"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #88 必须使用固定 scale facts 制品与原始三项 FIBO 查询，在隔离 PostgreSQL
# endpoint 比较完整 RDF term；质量门槛和 30 秒资源上限属于同一交付场景。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-fibo-scale-postgres-endpoint"
      and .issue == 88
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/integration/fibo-scale"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "intent-engine-fibo-scale-postgres-endpoint"
      and .reference_result.source == "tests/compat/postgres-fibo/fixtures/scale/generated/facts.sql"
      and .environment.timeout_seconds == 30
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #85 必须下载并 hash 校验固定 NPD 制品，在完整 PostgreSQL dataset/mapping/OWL
# 上以 HTTP JSON 和独立 SQL oracle 观察 BGP、VALUES+OPTIONAL 和 subclass 蕴含。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-npd-postgres-endpoint"
      and .issue == 85
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/integration/npd"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "intent-engine-npd-postgres-endpoint"
      and .reference_result.source == "tests/compat/postgres-npd-fixture.json"
      and .environment.timeout_seconds == 30
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #56 必须把关系图模式作为一个真实 PostgreSQL endpoint 情景验收：VALUES 的
# multiset、OPTIONAL 的未绑定变量、MINUS 的共享变量负匹配和 UNION 的分支重数
# 都必须以完整 RDF binding 与 ORDER BY 序列比较。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-postgres-relational-pattern-bag"
      and .issue == 56
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/query/relational-patterns"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "postgres-relational-pattern-bag-endpoint"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #55 的最小纵切必须由真实 PostgreSQL endpoint 返回完整 RDF binding。账本只接受
# 两个 native mapping source 的共享变量 BGP；SQL 文本、reformulate diagnostics 或
# 旧的逐三元组内存 join 均不能替代该证据。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-postgres-sql-reformulated-bgp"
      and .issue == 55
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/query/sql-reformulation"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "postgres-sql-reformulated-bgp-endpoint"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #57 必须以真实 PostgreSQL endpoint 验收 IN/NOT IN 的短路 error 规则，以及
# BOUND、BIND 和字符串函数的组合结果；单元测试或纯 VALUES 不能替代 mapping 输入。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-postgres-expression-in-bind-error"
      and .issue == 57
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/query/expressions"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "postgres-expression-in-bind-error-endpoint"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #58 必须在 PostgreSQL numeric mapping 输入上验收精确 decimal、正负 ROUND
# 边界和聚合前/后舍入；不能用 f64 单元测试、SQL 文本或纯 BIND 替代。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-postgres-decimal-round-exact"
      and .issue == 58
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/query/decimal"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "postgres-decimal-round-exact-endpoint"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #59 必须在真实 PostgreSQL mapping endpoint 中组合验收聚合子查询、HAVING
# 以及聚合/算术表达式 ORDER BY；不能只由 parser 或内存 VALUES 测试替代。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-postgres-aggregate-having-expression-order"
      and .issue == 59
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/query/aggregation"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "postgres-aggregate-having-expression-order-endpoint"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #60 必须以 PostgreSQL mapping endpoint 验收有界 path 的 bag 重数，以及与
# outer binding 关联的 EXISTS/NOT EXISTS；parser 成功、SQL 文本或无关子查询均不足。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-postgres-property-path-exists"
      and .issue == 60
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/query/property-path-and-exists"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "postgres-property-path-exists-endpoint"
      and .command == "./scripts/test-delivery-compat.sh"
    )
' "$ledger" >/dev/null

# #61 必须以真实 PostgreSQL endpoint 验收 FROM/FROM NAMED 对 named graph 的
# 可见性；SERVICE 的固定 Ontop baseline 未支持边界也必须是稳定请求错误，不能向
# 公网发起隐式请求或把 parser 成功冒充 federation 结果。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "intent-engine-postgres-dataset-graph-and-service-boundary"
      and .issue == 61
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/query/dataset-and-graph"
      and .assertion_strength == "full-term"
      and .rtop_case_id == "postgres-dataset-graph-service-boundary-endpoint"
      and .command == "./scripts/test-delivery-compat.sh"
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

# #83 的 endpoint 生命周期证据必须来自真实 PostgreSQL HTTP 请求：慢请求不能
# 阻塞成功请求，失败不能污染成功结果，断开必须取消 pg_sleep 并保留后续请求能力。
jq -e '
  .entries as $entries
  | any($entries[];
      .asset_id == "postgres-http-request-isolation-cancellation-and-disconnect"
      and .issue == 83
      and .in_scope == true
      and .status == "passed"
      and .scope == "replacement-kernel/http/request-lifecycle"
      and .assertion_strength == "boolean"
      and .rtop_case_id == "postgres-http-request-isolation-cancellation-and-disconnect"
      and (.command | endswith("./scripts/test-delivery-compat.sh"))
    )
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

# #62 的 URL input、XML Catalog 与 imports closure 必须由真实 PostgreSQL
# endpoint 证明；远程 IRI 经本地 catalog 映射，避免把公网可达性当成测试前提。
jq -e '.entries as $e | any($e[]; .asset_id == "postgres-ontology-imports-url-catalog-endpoint" and .issue == 62 and .status == "passed" and .assertion_strength == "full-term")' "$ledger" >/dev/null

# #63 必须同时钉住 OWL 2 QL TBox 的等价/无 filler 存在限制改写，以及限定
# existential 的基线限制；不得用纯内存断言代替 PostgreSQL endpoint 观测。
jq -e '.entries as $e | any($e[]; .asset_id == "postgres-owl-ql-tbox-closure-and-limit-endpoint" and .issue == 63 and .status == "passed" and .assertion_strength == "full-term")' "$ledger" >/dev/null

# #64 的 native OBDA 不能只靠 runtime/CLI：template、NULL、named graph 及
# source SQL 的加载期/执行期诊断均须由 PostgreSQL HTTP endpoint 保留。
jq -e '.entries as $e | any($e[]; .asset_id == "postgres-native-obda-template-null-graph-source-endpoint" and .issue == 64 and .status == "passed" and .assertion_strength == "full-term")' "$ledger" >/dev/null

# #67 将固定 EPNet native OBDA 的动态 class IRI 放到真实 PostgreSQL HTTP
# endpoint 中验收；metadata 仍是 CLI schema catalog 契约，不能凭空扩展 endpoint
# route。成功结果必须保留完整 URI term，而非仅比较基线的 countResults(1)。
if ! jq -e '.entries as $e | any($e[]; .asset_id == "postgres-epnet-dynamic-meta-mapping-endpoint" and .issue == 67 and .status == "passed" and .assertion_strength == "full-term" and .rtop_case_id == "postgres-epnet-dynamic-meta-mapping-endpoint" and .replay.runner == "./scripts/test-ontop-rtop-differential.sh" and .replay.cases == ["httpepnet"] and .replay.parameters == {})' "$ledger" >/dev/null; then
  printf '%s\n' 'coverage ledger: invalid replay: asset_id=postgres-epnet-dynamic-meta-mapping-endpoint field=replay expected=runner=./scripts/test-ontop-rtop-differential.sh,cases=[httpepnet],parameters={}' >&2
  exit 1
fi

# #71 不能把 R2RML CLI 图对照冒充 endpoint 证据：原始 D014b 与 D008a
# 必须分别经 PostgreSQL HTTP 返回 join/blank node/typed literal 和 GRAPH
# template binding，同时 D007h 的 literal graphMap 必须保留 invalid-mapping 分类。
if ! jq -e '.entries as $e | any($e[]; .asset_id == "postgres-r2rml-term-map-join-graph-endpoint" and .issue == 71 and .status == "passed" and .assertion_strength == "full-term" and .rtop_case_id == "postgres-r2rml-term-map-join-graph-endpoint" and .replay.runner == "./scripts/test-ontop-rtop-differential.sh" and .replay.cases == ["d014b", "d008a", "d007h"] and .replay.parameters == {})' "$ledger" >/dev/null; then
  printf '%s\n' 'coverage ledger: invalid replay: asset_id=postgres-r2rml-term-map-join-graph-endpoint field=replay expected=runner=./scripts/test-ontop-rtop-differential.sh,cases=[d014b,d008a,d007h],parameters={}' >&2
  exit 1
fi

jq -e '.entries as $e | any($e[]; .asset_id == "postgres-direct-mapping-pk-fk-null-encoding-endpoint" and .issue == 74 and .status == "passed" and .assertion_strength == "full-term" and .rtop_case_id == "postgres-direct-mapping-pk-fk-null-encoding-endpoint")' "$ledger" >/dev/null

# #77 的 PostgreSQL 类型、cast、regex 和标识符必须在真实 endpoint 返回 RDF
# terms；metadata 是 Ontop CLI JSON 的可观察边界，不能被误报为虚构 HTTP route。
if ! jq -e '.entries as $e | any($e[]; .asset_id == "postgres-type-cast-identifier-regex-metadata-endpoint" and .issue == 77 and .status == "passed" and .assertion_strength == "full-term" and .rtop_case_id == "postgres-type-cast-identifier-regex-metadata-endpoint" and .replay.runner == "./scripts/test-ontop-rtop-differential.sh" and .replay.cases == ["httpcastmappeddate", "httpcast", "httpregex", "httpidentifierfolding", "httpquotedalias", "cliextractmetadata"] and .replay.parameters == {})' "$ledger" >/dev/null; then
  printf '%s\n' 'coverage ledger: invalid replay: asset_id=postgres-type-cast-identifier-regex-metadata-endpoint field=replay expected=runner=./scripts/test-ontop-rtop-differential.sh,cases=[httpcastmappeddate,httpcast,httpregex,httpidentifierfolding,httpquotedalias,cliextractmetadata],parameters={}' >&2
  exit 1
fi

# #79 的 JSON、JSONB、array 和 PostGIS 均必须由真实 PostgreSQL endpoint 的
# RDF term/空结果验收；不能退回 source SQL、CLI 行数或模拟 PostGIS 函数。
if ! jq -e '.entries as $e | any($e[]; .asset_id == "postgres-nested-json-jsonb-array-postgis-endpoint" and .issue == 79 and .status == "passed" and .assertion_strength == "full-term" and .rtop_case_id == "postgres-nested-json-jsonb-array-postgis-endpoint" and .replay.runner == "./scripts/test-ontop-rtop-differential.sh" and .replay.cases == ["httpnestedaggregate", "httppostgisintersection"] and .replay.parameters == {})' "$ledger" >/dev/null; then
  printf '%s\n' 'coverage ledger: invalid replay: asset_id=postgres-nested-json-jsonb-array-postgis-endpoint field=replay expected=runner=./scripts/test-ontop-rtop-differential.sh,cases=[httpnestedaggregate,httppostgisintersection],parameters={}' >&2
  exit 1
fi

# #81 只把 PostgreSQL 约束保持的结果和请求可完成性视为 endpoint 契约；不得用
# Ontop 内部 SQL 计划文字替代 NULL、FK join bag 与 aggregate RDF term 证据。
if ! jq -e '.entries as $e | any($e[]; .asset_id == "postgres-constraint-left-join-aggregate-endpoint" and .issue == 81 and .status == "passed" and .assertion_strength == "full-term" and .rtop_case_id == "postgres-constraint-left-join-aggregate-endpoint" and .replay.runner == "./scripts/test-ontop-rtop-differential.sh" and .replay.cases == ["httpprofoptional", "httpproffkjoin", "httpprofaggregate"] and .replay.parameters == {})' "$ledger" >/dev/null; then
  printf '%s\n' 'coverage ledger: invalid replay: asset_id=postgres-constraint-left-join-aggregate-endpoint field=replay expected=runner=./scripts/test-ontop-rtop-differential.sh,cases=[httpprofoptional,httpproffkjoin,httpprofaggregate],parameters={}' >&2
  exit 1
fi

# #82 需要真实 PostgreSQL endpoint 的协议格式与 Accept 协商证据；JSON 的单一
# happy path 不能替代 XML/CSV/TSV、N-Triples、dataset/config 与 406 边界。
if ! jq -e '.entries as $e | any($e[]; .asset_id == "postgres-http-result-formats-dataset-config-endpoint" and .issue == 82 and .status == "passed" and .assertion_strength == "full-term" and .rtop_case_id == "postgres-http-result-formats-dataset-config-endpoint" and .replay.runner == "./scripts/test-ontop-rtop-differential.sh" and .replay.cases == ["httpformats"] and .replay.parameters == {})' "$ledger" >/dev/null; then
  printf '%s\n' 'coverage ledger: invalid replay: asset_id=postgres-http-result-formats-dataset-config-endpoint field=replay expected=runner=./scripts/test-ontop-rtop-differential.sh,cases=[httpformats],parameters={}' >&2
  exit 1
fi

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
jq -e '.entries as $e | ["postgres-native-obda-prefix-declaration-iri-term","postgres-native-obda-legal-source-lower-term","postgres-native-obda-reader-missing-target","postgres-native-obda-runtime-source-relation-error"] | all(.[]; . as $id | any($e[]; .asset_id == $id and .issue == 41 and .status == "passed")) and any($e[]; .asset_id == "postgres-native-obda-invalid-source-sql" and .issue == 41 and (.status == "pending" or .status == "passed"))' "$ledger" >/dev/null

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
reference_pairs=$(mktemp)
jq -r '
  .entries[]
  | . as $entry
  | ($entry.reference_result.source | split("; ")) as $sources
  | ($entry.reference_result.sha256 | split(";")) as $hashes
  | if ($sources | length) != ($hashes | length) then
      error("reference source/hash count mismatch for " + $entry.asset_id)
    else range(0; $sources | length) end
  | . as $index
  | [$entry.asset_id, $sources[$index], $hashes[$index]]
  | @tsv
' "$ledger" > "$reference_pairs"
while IFS="$(printf '\t')" read -r asset_id source expected_hash; do
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
done < "$reference_pairs"
rm -f "$reference_pairs"

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
