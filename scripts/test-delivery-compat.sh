#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
database=rtop-delivery-compat-$$
postgis_database=
port=${RTOP_DELIVERY_PORT:-18080}
tmp=$(mktemp -d)
server_pid=
cleanup() {
  [ -z "$server_pid" ] || kill "$server_pid" >/dev/null 2>&1 || true
  [ -z "$server_pid" ] || wait "$server_pid" >/dev/null 2>&1 || true
  docker rm -f "$database" >/dev/null 2>&1 || true
  [ -z "$postgis_database" ] || docker rm -f "$postgis_database" >/dev/null 2>&1 || true
  rmdir "$tmp" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker run -d --name "$database" -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop \
  -e POSTGRES_DB=rtop_test -p 127.0.0.1::5432 postgres:17 >/dev/null
postgres_port=$(docker port "$database" 5432/tcp | sed 's/.*://')
until docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do sleep 1; done
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-query-kinds/init.sql"
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-http/algebra-init.sql"
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D001/create.sql"

query_config="$tmp/query.toml"
sed \
  -e "s|mapping = \"mapping.obda\"|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$query_config"
http_config="$tmp/http.toml"
sed \
  -e "s|mapping = \"mapping-algebra-bag.obda\"|mapping = \"$root/tests/compat/postgres-http/mapping-algebra-bag.obda\"|" \
  -e "s|ontology = \"ontology.ttl\"|ontology = \"$root/tests/compat/postgres-http/ontology.ttl\"|" \
  -e "s|predefined_config = \"predefined.json\"|predefined_config = \"$root/tests/compat/postgres-http/predefined.json\"|" \
  -e "s|predefined_queries = \"predefined.toml\"|predefined_queries = \"$root/tests/compat/postgres-http/predefined.toml\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-http/rtop.toml" >"$http_config"

(cd "$root" && cargo run --quiet -- validate "$query_config")
(cd "$root" && cargo run --quiet -- compile "$query_config")
actual=$(cd "$root" && cargo run --quiet -- query "$query_config" tests/compat/postgres-query-kinds/ask.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- materialize "$query_config" "$tmp/materialized.ttl") >"$tmp/materialize.stdout"
grep -q '^NR of TRIPLES: 1$' "$tmp/materialize.stdout"
grep -q '^<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> \.$' "$tmp/materialized.ttl"
(cd "$root" && cargo run --quiet -- extract-db-metadata "$query_config" "$tmp/metadata.json")
jq -e '.relations[] | select(.name == ["\"query_people\""]) | .columns == [{name:"\"id\"", isNullable:false, datatype:"INTEGER"}]' "$tmp/metadata.json" >/dev/null
(cd "$root" && cargo run --quiet -- bootstrap "$query_config" https://bootstrap.example "$tmp/bootstrap.obda" "$tmp/bootstrap.ttl")
grep -q '^mappingId bootstrap-query_people$' "$tmp/bootstrap.obda"
grep -q '^<https://bootstrap.example/query_people> a owl:Class \.$' "$tmp/bootstrap.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/bootstrap.obda\"|" "$query_config" >"$tmp/bootstrap.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/bootstrap.toml" tests/compat/postgres-query-kinds/bootstrap.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping pretty-r2rml "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D000/r2rml.ttl" "$tmp/pretty-r2rml.ttl")
grep -q 'http://www.w3.org/ns/r2rml#TriplesMap' "$tmp/pretty-r2rml.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/pretty-r2rml.ttl\"|" "$query_config" >"$tmp/pretty-r2rml.toml"
(cd "$root" && cargo run --quiet -- validate "$tmp/pretty-r2rml.toml")
(cd "$root" && cargo run --quiet -- mapping to-obda "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D001/r2rmla.ttl" "$tmp/to-obda.obda")
grep -q '^mappingId[[:space:]]*rtop-r2rml-1$' "$tmp/to-obda.obda"
grep -q '<http://xmlns.com/foaf/0.1/name>' "$tmp/to-obda.obda"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/to-obda.obda\"|" "$query_config" >"$tmp/to-obda.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/to-obda.toml" tests/compat/postgres-query-kinds/to-obda.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping to-obda "$root/tests/compat/postgres-query-kinds/relative-r2rml.ttl" "$tmp/relative-r2rml.obda")
grep -q '<https://example.test/base/person/{id}> <https://example.test/base/kind> <https://example.test/base/Person>' "$tmp/relative-r2rml.obda"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/relative-r2rml.obda\"|" "$query_config" >"$tmp/relative-r2rml.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/relative-r2rml.toml" tests/compat/postgres-query-kinds/relative-r2rml.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/mapping.obda" "$tmp/to-r2rml.ttl" --force)
grep -q '^@prefix rr: <http://www.w3.org/ns/r2rml#> \.$' "$tmp/to-r2rml.ttl"
grep -q 'rr:predicate <https://example.test/type>' "$tmp/to-r2rml.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/to-r2rml.ttl\"|" "$query_config" >"$tmp/to-r2rml.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/to-r2rml.toml" tests/compat/postgres-query-kinds/ask.rq)
[ "$actual" = true ]
set +e
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/mapping.obda" "$tmp/no-force.ttl") >"$tmp/to-r2rml.stdout" 2>"$tmp/to-r2rml.stderr"
to_r2rml_status=$?
set -e
[ "$to_r2rml_status" = 2 ]
grep -q -- '--force' "$tmp/to-r2rml.stderr"
set +e
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/v1-to-v3.obda" "$tmp/legacy-to-r2rml.ttl" --force) >"$tmp/legacy-to-r2rml.stdout" 2>"$tmp/legacy-to-r2rml.stderr"
legacy_to_r2rml_status=$?
set -e
[ "$legacy_to_r2rml_status" = 2 ]
grep -q 'SourceDeclaration' "$tmp/legacy-to-r2rml.stderr"
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/named-graph.obda" "$tmp/named-graph.ttl" --force)
grep -q 'rr:graphMap \[ rr:constant <https://example.test/graph/people> \]' "$tmp/named-graph.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/named-graph.ttl\"|" "$query_config" >"$tmp/named-graph.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/named-graph.toml" tests/compat/postgres-query-kinds/named-graph.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/template-graph.obda" "$tmp/template-graph.ttl" --force)
grep -q 'rr:graphMap \[ rr:template "https://example.test/graph/people/{id}" \]' "$tmp/template-graph.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/template-graph.ttl\"|" "$query_config" >"$tmp/template-graph.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/template-graph.toml" tests/compat/postgres-query-kinds/template-graph.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/typed-blank-node.obda" "$tmp/typed-blank-node.ttl" --force)
grep -q 'rr:termType rr:BlankNode' "$tmp/typed-blank-node.ttl"
grep -q 'rr:datatype <http://www.w3.org/2001/XMLSchema#integer>' "$tmp/typed-blank-node.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/typed-blank-node.ttl\"|" "$query_config" >"$tmp/typed-blank-node.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/typed-blank-node.toml" tests/compat/postgres-query-kinds/typed-blank-node.rq)
[ "$actual" = true ]
set +e
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/duplicate-mapping-id.obda" "$tmp/duplicate-mapping-id.ttl" --force) >"$tmp/duplicate-mapping-id.stdout" 2>"$tmp/duplicate-mapping-id.stderr"
duplicate_mapping_id_status=$?
set -e
[ "$duplicate_mapping_id_status" = 2 ]
grep -q 'Duplicate mapping IDs found in obda file' "$tmp/duplicate-mapping-id.stderr"
[ ! -e "$tmp/duplicate-mapping-id.ttl" ]
set +e
(cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$root/tests/compat/postgres-query-kinds/v1-to-v3.obda" "$tmp/v1-to-v3.obda") >"$tmp/v1-to-v3.stdout" 2>"$tmp/v1-to-v3.stderr"
v1_to_v3_status=$?
set -e
[ "$v1_to_v3_status" = 2 ]
grep -q 'Unknown parameter name "sourceUri"' "$tmp/v1-to-v3.stderr"
[ ! -e "$tmp/v1-to-v3.obda" ]
(cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$root/tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.obda" "$tmp/v1-to-v3-duplicate-alias.obda")
grep -q ':person/{id1} :related :person/{id2}' "$tmp/v1-to-v3-duplicate-alias.obda"
grep -q 'left_person.id AS id1, right_person.id AS id2' "$tmp/v1-to-v3-duplicate-alias.obda"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/v1-to-v3-duplicate-alias.obda\"|" "$query_config" >"$tmp/v1-to-v3-duplicate-alias.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/v1-to-v3-duplicate-alias.toml" tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$root/tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.obda" "$tmp/v1-to-v3-duplicate-alias-simplified.obda" --simplify-projection)
grep -q 'left_person.id AS id1, right_person.id AS id2' "$tmp/v1-to-v3-duplicate-alias-simplified.obda"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/v1-to-v3-duplicate-alias-simplified.obda\"|" "$query_config" >"$tmp/v1-to-v3-duplicate-alias-simplified.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/v1-to-v3-duplicate-alias-simplified.toml" tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$root/tests/compat/postgres-query-kinds/v1-to-v3-simplify-projection.obda" "$tmp/v1-to-v3-simplify-projection.obda" --simplify-projection)
grep -q 'source[[:space:]]*SELECT \* FROM query_people' "$tmp/v1-to-v3-simplify-projection.obda"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/v1-to-v3-simplify-projection.obda\"|" "$query_config" >"$tmp/v1-to-v3-simplify-projection.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/v1-to-v3-simplify-projection.toml" tests/compat/postgres-query-kinds/v1-to-v3-simplify-projection.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$root/tests/compat/postgres-query-kinds/v1-to-v3-r2rml.ttl" "$tmp/v1-to-v3-r2rml.ttl")
grep -q 'rr:template "https://example.test/person/{id1}"' "$tmp/v1-to-v3-r2rml.ttl"
grep -q 'rr:template "https://example.test/person/{id2}"' "$tmp/v1-to-v3-r2rml.ttl"
grep -q 'left_person.id AS id1, right_person.id AS id2' "$tmp/v1-to-v3-r2rml.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/v1-to-v3-r2rml.ttl\"|" "$query_config" >"$tmp/v1-to-v3-r2rml.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/v1-to-v3-r2rml.toml" tests/compat/postgres-query-kinds/v1-to-v3-r2rml.rq)
[ "$actual" = true ]
set +e
(cd "$root" && cargo run --quiet -- unknown "$query_config") >"$tmp/unknown.stdout" 2>"$tmp/unknown.stderr"
unknown_status=$?
set -e
[ "$unknown_status" = 64 ]
grep -q '^未知命令：unknown$' "$tmp/unknown.stderr"

RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$http_config" "127.0.0.1:$port" >"$tmp/server.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
[ "$(cat "$tmp/health")" = ok ]

request() {
  curl -sS -D "$tmp/headers" -o "$tmp/body" -w '%{http_code}' "$@"
}

[ "$(request "http://127.0.0.1:$port/ontology")" = 200 ]
grep -qi '^Content-Type: text/plain; charset=utf-8' "$tmp/headers"
# `/ontology` 下载的是固定 ontology 文件本身；该资产用 ex: 前缀表达 Person，
# 因而不能把 serializer 未承诺的 IRI 展开当作 HTTP 契约。
grep -q '^ex:Person a <http://www.w3.org/2002/07/owl#Class> \.$' "$tmp/body"
[ "$(request -X POST "http://127.0.0.1:$port/ontology")" = 200 ]

[ "$(request -H 'Accept: text/turtle' "http://127.0.0.1:$port/predefined/person?person=https%3A%2F%2Fexample.test%2Fperson%2F1")" = 200 ]
grep -qi '^Content-Type: text/turtle; charset=utf-8' "$tmp/headers"
grep -q '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' "$tmp/body"
[ "$(request -X POST --data-urlencode 'person=https://example.test/person/1' -H 'Accept: text/turtle' "http://127.0.0.1:$port/predefined/person")" = 200 ]
grep -qi '^Content-Type: text/turtle; charset=utf-8' "$tmp/headers"
grep -qi '^Cache-Control: no-store' "$tmp/headers"
grep -q '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' "$tmp/body"
[ "$(request "http://127.0.0.1:$port/predefined/missing")" = 404 ]
[ "$(request "http://127.0.0.1:$port/predefined/person")" = 400 ]
[ "$(request "http://127.0.0.1:$port/predefined/person?person=not-an-iri")" = 500 ]
grep -q 'Unexpected exception: Not a valid (absolute) IRI: not-an-iri' "$tmp/body"

ask='ASK { ?person <https://example.test/type> <https://example.test/Person> }'
select='SELECT ?person { ?person <https://example.test/type> <https://example.test/Person> }'
sql_seam_query=$(cat "$root/tests/compat/postgres-http/sql-seam.rq")
algebra_bag_query=$(cat "$root/tests/compat/postgres-http/algebra-bag.rq")
expression_in_bind_query=$(cat "$root/tests/compat/postgres-http/expression-in-bind.rq")
decimal_round_query=$(cat "$root/tests/compat/postgres-http/decimal-round.rq")
decimal_round_aggregate_query=$(cat "$root/tests/compat/postgres-http/decimal-round-aggregate.rq")
aggregate_having_order_query=$(cat "$root/tests/compat/postgres-http/aggregate-having-order.rq")
property_path_exists_query=$(cat "$root/tests/compat/postgres-http/property-path-exists.rq")
not_exists_correlation_query=$(cat "$root/tests/compat/postgres-http/not-exists-correlation.rq")
dataset_graph_query=$(cat "$root/tests/compat/postgres-http/dataset-graph.rq")
dataset_named_graph_query=$(cat "$root/tests/compat/postgres-http/dataset-named-graph.rq")
native_obda_terms_null_query=$(cat "$root/tests/compat/postgres-http/native-obda-terms-null.rq")

[ "$(request -G --data-urlencode "query=$ask" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
grep -qi '^Content-Type: application/sparql-results+json; charset=utf-8' "$tmp/headers"
grep -qi '^Cache-Control: no-store' "$tmp/headers"
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":true}' ]

[ "$(request -X POST --data-urlencode "query=$ask" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":true}' ]

[ "$(request -X POST --data-binary "$ask" -H 'Content-Type: application/sparql-query' -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":true}' ]

[ "$(request -X POST --data-urlencode "query=$sql_seam_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
grep -qi '^Content-Type: application/sparql-results+json; charset=utf-8' "$tmp/headers"
jq -e '(.head.vars | sort) == ["name", "person"] and .results.bindings == [{name:{type:"literal",value:"Ada"},person:{type:"uri",value:"https://example.test/person/1"}}]' "$tmp/body" >/dev/null

# SPARQL protocol 的 SELECT/ASK 结果格式与 CONSTRUCT 图格式均在同一 PostgreSQL
# endpoint 协商：JSON 之外必须保留 XML、CSV、TSV 和 N-Triples 的可观察载荷。
[ "$(request -X POST --data-urlencode "query=$sql_seam_query" -H 'Accept: application/sparql-results+xml' "http://127.0.0.1:$port/sparql")" = 200 ]
grep -qi '^Content-Type: application/sparql-results+xml; charset=utf-8' "$tmp/headers"
grep -F '<variable name="name"/><variable name="person"/>' "$tmp/body" >/dev/null
grep -F '<binding name="name"><literal>Ada</literal></binding><binding name="person"><uri>https://example.test/person/1</uri></binding>' "$tmp/body" >/dev/null
[ "$(request -X POST --data-urlencode "query=$sql_seam_query" -H 'Accept: text/csv' "http://127.0.0.1:$port/sparql")" = 200 ]
grep -qi '^Content-Type: text/csv; charset=utf-8' "$tmp/headers"
[ "$(cat "$tmp/body")" = 'name,person
Ada,https://example.test/person/1' ]
[ "$(request -X POST --data-urlencode "query=$sql_seam_query" -H 'Accept: text/tab-separated-values' "http://127.0.0.1:$port/sparql")" = 200 ]
grep -qi '^Content-Type: text/tab-separated-values; charset=utf-8' "$tmp/headers"
protocol_tab=$(printf '\t')
grep -F "?name${protocol_tab}?person" "$tmp/body" >/dev/null
grep -F "\"Ada\"${protocol_tab}<https://example.test/person/1>" "$tmp/body" >/dev/null
graph_protocol_query='CONSTRUCT { ?person <https://example.test/type> <https://example.test/Person> } WHERE { ?person <https://example.test/type> <https://example.test/Person> }'
[ "$(request -X POST --data-urlencode "query=$graph_protocol_query" -H 'Accept: application/n-triples' "http://127.0.0.1:$port/sparql")" = 200 ]
grep -qi '^Content-Type: application/n-triples; charset=utf-8' "$tmp/headers"
grep -F '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' "$tmp/body" >/dev/null
[ "$(request -X POST --data-urlencode "query=$sql_seam_query" -H 'Accept: application/unsupported' "http://127.0.0.1:$port/sparql")" = 406 ]
grep -Fx '请求的 Accept 不支持该结果格式' "$tmp/body"

[ "$(request -X POST --data-urlencode "query=$algebra_bag_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["name", "person", "tag"]
  and [.results.bindings[] | {person:.person.value, name:(.name.value // null), tag:.tag.value}]
      == [
        {person:"https://example.test/person/1", name:"Ada", tag:"left"},
        {person:"https://example.test/person/1", name:"Ada", tag:"right"},
        {person:"https://example.test/person/3", name:null, tag:"left"},
        {person:"https://example.test/person/3", name:null, tag:"right"}
      ]
' "$tmp/body" >/dev/null

[ "$(request -X POST --data-urlencode "query=$aggregate_having_order_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["category", "total"]
  and .results.bindings == [
    {category:{type:"literal",value:"profit"},total:{type:"literal",value:"2.6",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}},
    {category:{type:"literal",value:"loss"},total:{type:"literal",value:"-1.5",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}
  ]
' "$tmp/body" >/dev/null

[ "$(request -X POST --data-urlencode "query=$property_path_exists_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["friend", "person"]
  and .results.bindings == [
    {person:{type:"uri",value:"https://example.test/person/1"},friend:{type:"uri",value:"https://example.test/person/3"}},
    {person:{type:"uri",value:"https://example.test/person/1"},friend:{type:"uri",value:"https://example.test/person/3"}}
  ]
' "$tmp/body" >/dev/null

[ "$(request -X POST --data-urlencode "query=$not_exists_correlation_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["person", "value"]
  and .results.bindings == [{
    person:{type:"uri",value:"https://example.test/person/3"},
    value:{type:"literal",value:"3.0",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}
  }]
' "$tmp/body" >/dev/null

[ "$(request -X POST --data-urlencode "query=$native_obda_terms_null_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["label", "person"]
  and .results.bindings == [
    {person:{type:"uri",value:"https://example.test/nullable/1"},label:{type:"literal",value:"visible"}},
    {person:{type:"uri",value:"https://example.test/person/1"},label:{type:"literal",value:"Ada","xml:lang":"en"}}
  ]
' "$tmp/body" >/dev/null

for dataset_query in "$dataset_graph_query" "$dataset_named_graph_query"; do
  [ "$(request -X POST --data-urlencode "query=$dataset_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
  jq -e '
    (.head.vars | sort) == ["name", "person"]
    and .results.bindings == [{
      person:{type:"uri",value:"https://example.test/person/1"},
      name:{type:"literal",value:"Ada"}
    }]
  ' "$tmp/body" >/dev/null
done

# 固定 Ontop 源码基线对 SERVICE 返回 500；不能把网络失败静默降级或伪称
# federation 成功。旧部署锚曾验收 400，此处以固定提交的双边差分结果为准。
[ "$(request -X POST --data-urlencode 'query=SELECT ?person { SERVICE <http://example.invalid/sparql> { ?person ?p ?o } }' "http://127.0.0.1:$port/sparql")" = 500 ]
grep -q '^not-fully-translatable: SERVICE .*未支持' "$tmp/body"

[ "$(request -X POST --data-urlencode "query=$decimal_round_aggregate_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["roundAfterSum", "sumAfterRound", "total"]
  and .results.bindings == [{
    total:{type:"literal",value:"1.1",datatype:"http://www.w3.org/2001/XMLSchema#decimal"},
    roundAfterSum:{type:"literal",value:"1",datatype:"http://www.w3.org/2001/XMLSchema#decimal"},
    sumAfterRound:{type:"literal",value:"2",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}
  }]
' "$tmp/body" >/dev/null

[ "$(request -X POST --data-urlencode "query=$expression_in_bind_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["person", "role"]
  and .results.bindings == [{
    person:{type:"uri",value:"https://example.test/person/1"},
    role:{type:"literal",value:"ADA-3",datatype:"http://www.w3.org/2001/XMLSchema#string"}
  }]
' "$tmp/body" >/dev/null

[ "$(request -X POST --data-urlencode "query=$decimal_round_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["amount", "negativeHalf", "rounded", "sum"]
  and [.results.bindings[] | {
    amount:.amount.value, sum:.sum.value, rounded:.rounded.value, negativeHalf:.negativeHalf.value,
    amountDatatype:.amount.datatype, sumDatatype:.sum.datatype, roundedDatatype:.rounded.datatype, negativeHalfDatatype:.negativeHalf.datatype
  }] == [
    {amount:"-1.50",sum:"-1.3",rounded:"-1",negativeHalf:"-2",amountDatatype:"http://www.w3.org/2001/XMLSchema#decimal",sumDatatype:"http://www.w3.org/2001/XMLSchema#decimal",roundedDatatype:"http://www.w3.org/2001/XMLSchema#decimal",negativeHalfDatatype:"http://www.w3.org/2001/XMLSchema#decimal"},
    {amount:"0.10",sum:"0.3",rounded:"0",negativeHalf:"-2",amountDatatype:"http://www.w3.org/2001/XMLSchema#decimal",sumDatatype:"http://www.w3.org/2001/XMLSchema#decimal",roundedDatatype:"http://www.w3.org/2001/XMLSchema#decimal",negativeHalfDatatype:"http://www.w3.org/2001/XMLSchema#decimal"},
    {amount:"2.50",sum:"2.7",rounded:"3",negativeHalf:"-2",amountDatatype:"http://www.w3.org/2001/XMLSchema#decimal",sumDatatype:"http://www.w3.org/2001/XMLSchema#decimal",roundedDatatype:"http://www.w3.org/2001/XMLSchema#decimal",negativeHalfDatatype:"http://www.w3.org/2001/XMLSchema#decimal"}
  ]
' "$tmp/body" >/dev/null

[ "$(request -G --data-urlencode 'query=SELECT' "http://127.0.0.1:$port/sparql")" = 400 ]
grep -qi '^Cache-Control: no-store' "$tmp/headers"

[ "$(request -G --data-urlencode "query=$select" "http://127.0.0.1:$port/ontop/reformulate")" = 200 ]
grep -Eqi '^X-Query-ID: [0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}' "$tmp/headers"
grep -q '^SELECT ' "$tmp/body"
[ "$(request -G --data-urlencode "query=$select" --data-urlencode 'forNativeConsumption=true' "http://127.0.0.1:$port/ontop/reformulate")" = 200 ]
grep -Eqi '^X-Query-ID: [0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}' "$tmp/headers"
grep -q '^SELECT ' "$tmp/body"

kill "$server_pid"
wait "$server_pid" || true
server_pid=
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D014/create.sql"
r2rml_d014_config="$tmp/r2rml-d014-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D014/r2rmlb.ttl\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$r2rml_d014_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$r2rml_d014_config" "127.0.0.1:$port" >"$tmp/r2rml-d014-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
r2rml_d014_query=$(cat "$root/tests/compat/postgres-http/r2rml-d014-ref-object-join.rq")
[ "$(request -X POST --data-urlencode "query=$r2rml_d014_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["department", "departmentNumber", "employee"]
  and .results.bindings == [{
    employee:{type:"uri",value:"http://example.com/emp/7369"},
    department:{type:"bnode",value:(.results.bindings[0].department.value)},
    departmentNumber:{type:"literal",value:"10",datatype:"http://www.w3.org/2001/XMLSchema#integer"}
  }]
' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
# D001 在交付 gate 起始阶段已创建同名 quoted Student；后续 fixture 不再依赖它，
# 因此明确移除后加载 D008 的复合主键/graphMap schema。
docker exec "$database" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"'
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D008/create.sql"
r2rml_d008_config="$tmp/r2rml-d008-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D008/r2rmla.ttl\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$r2rml_d008_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$r2rml_d008_config" "127.0.0.1:$port" >"$tmp/r2rml-d008-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
r2rml_d008_query=$(cat "$root/tests/compat/postgres-http/r2rml-d008-template-graph.rq")
[ "$(request -X POST --data-urlencode "query=$r2rml_d008_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  (.head.vars | sort) == ["name", "student"]
  and .results.bindings == [{
    student:{type:"uri",value:"http://example.com/Student/10/Venus%20Williams"},
    name:{type:"literal",value:"Venus Williams"}
  }]
' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
r2rml_invalid_graph_config="$tmp/r2rml-invalid-graph-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D007/r2rmlh.ttl\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$r2rml_invalid_graph_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$r2rml_invalid_graph_config" "127.0.0.1:$port" >"$tmp/r2rml-invalid-graph-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
[ "$(request -X POST --data-urlencode 'query=SELECT * {}' "http://127.0.0.1:$port/sparql")" = 500 ]
grep -q '^invalid-mapping: R2RML 结构错误：graphMap 必须是 IRI' "$tmp/body"
kill "$server_pid"
wait "$server_pid" || true
server_pid=
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-expressions/init.sql"
regex_config="$tmp/postgres-regex-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/pgsql/regex/stockexchangeRegex.obda\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-expressions/regex.toml" >"$regex_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint "$regex_config" "127.0.0.1:$port" >"$tmp/regex-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
regex_query=$(cat "$root/tests/compat/postgres-expressions/regex-expression.rq")
[ "$(request -X POST --data-urlencode "query=$regex_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '.results.bindings == [{person:{type:"uri",value:"http://www.owl-ontologies.com/Ontology1207768242.owl#Person-2"}}]' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
docker exec "$database" psql -U rtop -d rtop_test -c "UPDATE books SET publication_date = CASE id WHEN 1 THEN TIMESTAMP '2014-06-05 16:47:52' WHEN 2 THEN TIMESTAMP '2011-12-08 11:30:00' WHEN 3 THEN TIMESTAMP '2015-09-21 09:23:06' WHEN 4 THEN TIMESTAMP '1970-11-05 07:50:00' END WHERE id IN (1, 2, 3, 4)" >/dev/null
cast_config="$tmp/postgres-cast-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/lightweight-tests/src/test/resources/books/books.obda\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-cast/books.toml" >"$cast_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint "$cast_config" "127.0.0.1:$port" >"$tmp/cast-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
cast_query=$(cat "$root/tests/compat/postgres-cast/cast-date-from-mapped-datetime.rq")
[ "$(request -X POST --data-urlencode "query=$cast_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '[.results.bindings[].v.value] == ["1970-11-05","2011-12-08","2014-06-05","2015-09-21"] and all(.results.bindings[].v.datatype; . == "http://www.w3.org/2001/XMLSchema#date")' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
# PostgresIdentifierTest 使用同一份固定 datatype fixture："Characters" 的带引号表名
# 与 Type_Char 的未引号折叠、以及显式 "LETTER" alias 都必须经由 HTTP RDF bindings
# 可观察，不能仅以 CLI 的 SQL 结果替代。
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-datatype-manifest/init.sql"
identifiers_config="$tmp/postgres-identifiers-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/pgsql/identifiers/identifiers-postgres.obda\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-identifiers/baseline-identifiers.toml" >"$identifiers_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint "$identifiers_config" "127.0.0.1:$port" >"$tmp/identifiers-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
for identifier_query in country country4; do
  [ "$(request -X POST --data-urlencode "query=$(cat "$root/tests/compat/postgres-identifiers/$identifier_query.rq")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
  jq -e --arg suffix "$identifier_query" '
    .head.vars == ["x"]
    and .results.bindings == [{
      x:{type:"uri",value:("http://www.semanticweb.org/ontologies/2013/7/untitled-ontology-150#Country" + (if $suffix == "country" then "" else "4" end) + "-a")}
    }]
  ' "$tmp/body" >/dev/null
done
kill "$server_pid"
wait "$server_pid" || true
server_pid=
# AbstractNestedDataTest 的 JSON、JSONB 和 array source 必须在 endpoint 而非 CLI
# 层返回同一组完整 RDF literal；id=2 的空内层 array 是不可产生额外 binding 的反事实。
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-nested/init.sql"
for nested_case in 'jsonb rtop' 'json json' 'array array'; do
  set -- $nested_case
  nested_format=$1
  nested_base_config=$2
  nested_config="$tmp/postgres-nested-$nested_format-endpoint.toml"
  sed \
    -e "s|^mapping = .*|mapping = \"$root/tests/compat/postgres-nested/nested-$nested_format.obda\"|" \
    -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
    "$root/tests/compat/postgres-nested/$nested_base_config.toml" >"$nested_config"
  RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint "$nested_config" "127.0.0.1:$port" >"$tmp/nested-$nested_format-endpoint.log" 2>&1 &
  server_pid=$!
  until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
  [ "$(request -X POST --data-urlencode "query=$(cat "$root/tests/compat/postgres-nested/flatten-workers.rq")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
  jq -e '
    .head.vars == ["v"]
    and ([.results.bindings[] | {type:.v.type,value:.v.value}] | sort_by(.value)) == [
      {type:"literal",value:"Bob"}, {type:"literal",value:"Bob"}, {type:"literal",value:"Bob"},
      {type:"literal",value:"Carl"}, {type:"literal",value:"Cynthia"}, {type:"literal",value:"Cynthia"},
      {type:"literal",value:"Cynthia"}, {type:"literal",value:"Jim"}, {type:"literal",value:"Jim"},
      {type:"literal",value:"Jim"}, {type:"literal",value:"Sam"}
    ]
  ' "$tmp/body" >/dev/null
  [ "$(request -X POST --data-urlencode "query=$(cat "$root/tests/compat/postgres-nested/flatten-empty.rq")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
  jq -e '
    .head.vars == ["v"]
    and ([.results.bindings[] | {type:.v.type,value:.v.value}] | sort_by(.value)) == [
      {type:"literal",value:"Cynthia"}, {type:"literal",value:"Jim"}
    ]
  ' "$tmp/body" >/dev/null
  kill "$server_pid"
  wait "$server_pid" || true
  server_pid=
done
# AbstractLeftJoinProfTest 的 PostgreSQL university fixture 以主键和两个外键保存；
# endpoint 只比较约束保持后的 OPTIONAL、join bag 与聚合 RDF terms，绝不把 SQL
# 计划形状当作优化成功的替代证据。
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-aggregates/init.sql"
constraints_semantics_config="$tmp/postgres-constraints-semantics-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/redundant_join/redundant_join_fk_test.obda\"|" \
  -e "s|^ontology = .*|ontology = \"$root/../ontop/test/docker-tests/src/test/resources/redundant_join/redundant_join_fk_test.owl\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-aggregates/redundant-join.toml" >"$constraints_semantics_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint "$constraints_semantics_config" "127.0.0.1:$port" >"$tmp/constraints-semantics-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
[ "$(request -X POST --data-urlencode "query=$(cat "$root/tests/compat/postgres-aggregates/optional-unbound-nickname.rq")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  .head.vars == ["v"]
  and .results.bindings == [
    {v:{type:"literal",value:"Barbara"}}, {v:{type:"literal",value:"Diego"}},
    {v:{type:"literal",value:"Johann"}}, {v:{type:"literal",value:"Mary"}}
  ]
' "$tmp/body" >/dev/null
[ "$(request -X POST --data-urlencode "query=$(cat "$root/tests/compat/postgres-aggregates/optional-nickname-course.rq")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  .head.vars == ["v","f"]
  and .results.bindings == [
    {v:{type:"literal",value:"Rog"},f:{type:"literal",value:"Roger"}},
    {v:{type:"literal",value:"Rog"},f:{type:"literal",value:"Roger"}},
    {v:{type:"literal",value:"Rog"},f:{type:"literal",value:"Roger"}},
    {v:{type:"literal",value:"Johnny"},f:{type:"literal",value:"John"}}
  ]
' "$tmp/body" >/dev/null
[ "$(request -X POST --data-urlencode "query=$(cat "$root/tests/compat/postgres-aggregates/aggregation-mapping-prof-student-count-property.rq")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  .head.vars == ["v"]
  and .results.bindings == [
    {v:{type:"literal",value:"12",datatype:"http://www.w3.org/2001/XMLSchema#integer"}},
    {v:{type:"literal",value:"13",datatype:"http://www.w3.org/2001/XMLSchema#integer"}},
    {v:{type:"literal",value:"31",datatype:"http://www.w3.org/2001/XMLSchema#integer"}}
  ]
' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
# Direct Mapping 从 PostgreSQL catalog 规划：D009 同时覆盖 primary key subject、
# foreign-key ref IRI 和 NULL foreign key 的反事实。D008 已占用同名 Student，故先
# 删除再加载固定 D009 schema；该 endpoint 后续不再依赖 D008 relation。
docker exec "$database" psql -U rtop -d rtop_test -c 'DROP TABLE "Student"'
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D009/create.sql"
direct_d009_config="$tmp/direct-d009-endpoint.toml"
sed "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-direct-d009/rtop.toml" >"$direct_d009_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$direct_d009_config" "127.0.0.1:$port" >"$tmp/direct-d009-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
direct_d009_query=$(cat "$root/tests/compat/postgres-http/direct-d009-foreign-key.rq")
[ "$(request -X POST --data-urlencode "query=$direct_d009_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  .head.vars == ["student", "sport"]
  and .results.bindings == [{
    student:{type:"uri",value:"http://example.com/base/Student/ID=10"},
    sport:{type:"uri",value:"http://example.com/base/Sport/ID=100"}
  }]
' "$tmp/body" >/dev/null
[ "$(request -X POST --data-urlencode 'query=ASK { <http://example.com/base/Student/ID=20> <http://example.com/base/Student#ref-Sport> ?sport }' -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":false}' ]
kill "$server_pid"
wait "$server_pid" || true
server_pid=
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D010/create.sql"
direct_d010_config="$tmp/direct-d010-endpoint.toml"
sed "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-direct-d010/rtop.toml" >"$direct_d010_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$direct_d010_config" "127.0.0.1:$port" >"$tmp/direct-d010-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
direct_d010_query=$(cat "$root/tests/compat/postgres-http/direct-d010-percent-encoding.rq")
[ "$(request -X POST --data-urlencode "query=$direct_d010_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '.head.vars == ["name"] and .results.bindings == [{name:{type:"literal",value:"Bolivia, Plurinational State of"}}]' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-metamapping/epnet-init.sql"
epnet_config="$tmp/epnet-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/pgsql/EPNet.obda\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-metamapping/epnet.toml" >"$epnet_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$epnet_config" "127.0.0.1:$port" >"$tmp/epnet-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
epnet_query=$(cat "$root/tests/compat/postgres-metamapping/query.rq")
[ "$(request -X POST --data-urlencode "query=$epnet_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  .head.vars == ["x"]
  and .results.bindings == [{
    x:{type:"uri",value:"http://www.semanticweb.org/ontologies/2015/1/EPNet-ONTOP_Ontology#Inscription-101"}
  }]
' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
for native_case in invalid-sql1 missing-target; do
  native_config="$tmp/$native_case-endpoint.toml"
  sed \
    -e "s|^mapping = .*|mapping = \"$root/../ontop/mapping/sql/all/src/test/resources/mistake/$native_case.obda\"|" \
    -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
    "$root/tests/compat/postgres-native-obda/$native_case.toml" >"$native_config"
  RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
    "$native_config" "127.0.0.1:$port" >"$tmp/$native_case-endpoint.log" 2>&1 &
  server_pid=$!
  until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
  [ "$(request -X POST --data-urlencode 'query=SELECT * {}' "http://127.0.0.1:$port/sparql")" = 500 ]
  grep -q '^invalid-mapping:' "$tmp/body"
  kill "$server_pid"
  wait "$server_pid" || true
  server_pid=
done
native_runtime_config="$tmp/native-runtime-source-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/mapping/sql/all/src/test/resources/mistake/correct.obda\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-native-obda/valid-source-missing-relation.toml" >"$native_runtime_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$native_runtime_config" "127.0.0.1:$port" >"$tmp/native-runtime-source-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
native_runtime_query=$(cat "$root/tests/compat/postgres-native-obda/runtime-source.rq")
[ "$(request -X POST --data-urlencode "query=$native_runtime_query" "http://127.0.0.1:$port/sparql")" = 500 ]
grep -Fx 'datasource-failure: db error' "$tmp/body"
kill "$server_pid"
wait "$server_pid" || true
server_pid=
imports_config="$tmp/imports-catalog.toml"
sed \
  -e 's|^ontology = .*|ontology = "https://example.test/imports/root"|' \
  -e "/^ontology = /a\\xml_catalog = \"$root/tests/compat/postgres-http/imports-catalog.xml\"" \
  "$http_config" >"$imports_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$imports_config" "127.0.0.1:$port" >"$tmp/imports-catalog-server.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
imports_catalog_query=$(cat "$root/tests/compat/postgres-http/imports-catalog.rq")
[ "$(request -X POST --data-urlencode "query=$imports_catalog_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '
  .results.bindings == [
    {person:{type:"uri",value:"https://example.test/person/1"}},
    {person:{type:"uri",value:"https://example.test/person/2"}},
    {person:{type:"uri",value:"https://example.test/person/3"}}
  ]
' "$tmp/body" >/dev/null
for tbox_query in tbox-equivalent-class tbox-equivalent-property tbox-existential-domain; do
  query=$(cat "$root/tests/compat/postgres-http/$tbox_query.rq")
  [ "$(request -X POST --data-urlencode "query=$query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
  jq -e '
    .results.bindings == [
      {person:{type:"uri",value:"https://example.test/person/1"}},
      {person:{type:"uri",value:"https://example.test/person/2"}},
      {person:{type:"uri",value:"https://example.test/person/3"}}
    ]
  ' "$tmp/body" >/dev/null
done
qualified_existence_query=$(cat "$root/tests/compat/postgres-http/tbox-qualified-existential-limit.rq")
[ "$(request -X POST --data-urlencode "query=$qualified_existence_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '.results.bindings == []' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=
sed '/^ontology = /d' "$http_config" >"$tmp/no-ontology.toml"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$tmp/no-ontology.toml" "127.0.0.1:$port" >"$tmp/no-ontology-server.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
[ "$(request "http://127.0.0.1:$port/ontology")" = 404 ]
grep -q '^No ontology found$' "$tmp/body"
[ "$(request -X POST "http://127.0.0.1:$port/ontology")" = 404 ]
kill "$server_pid"
wait "$server_pid" || true
server_pid=
sed 's/enable_download_ontology = true/enable_download_ontology = false/' "$http_config" >"$tmp/ontology-download-disabled.toml"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$tmp/ontology-download-disabled.toml" "127.0.0.1:$port" >"$tmp/ontology-download-disabled-server.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
[ "$(request "http://127.0.0.1:$port/ontology")" = 404 ]
grep -q '^not found$' "$tmp/body"
[ "$(request -X POST "http://127.0.0.1:$port/ontology")" = 404 ]
kill "$server_pid"
wait "$server_pid" || true
server_pid=
invalid_port=$((port + 1))
sed 's/port = [0-9][0-9]*/port = 1/' "$query_config" > "$tmp/unreachable-postgres.toml"
cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$tmp/unreachable-postgres.toml" "127.0.0.1:$invalid_port" >"$tmp/error-server.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$invalid_port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
[ "$(request -G --data-urlencode "query=$ask" "http://127.0.0.1:$invalid_port/sparql")" = 500 ]
grep -q '^datasource-failure:' "$tmp/body"
grep -qi '^Cache-Control: no-store' "$tmp/headers"
kill "$server_pid"
wait "$server_pid" || true
server_pid=

# QuestParallelScenario 的可观察契约是同一 endpoint 上慢、失败和断开请求不妨碍
# 成功请求；这里使用独立 PostgreSQL 连接而不把内部线程或 SQL 计划当作结果。
concurrency_config="$tmp/postgres-concurrency-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/tests/compat/postgres-concurrency/mapping.obda\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$concurrency_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$concurrency_config" "127.0.0.1:$port" >"$tmp/concurrency-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
delayed_query=$(cat "$root/tests/compat/postgres-concurrency/delayed.rq")
curl -sS -G --data-urlencode "query=$delayed_query" \
  "http://127.0.0.1:$port/sparql" >"$tmp/delayed-response" &
delayed_pid=$!
sleep 1
kill -0 "$delayed_pid"
[ "$(request -G --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")" = 200 ]
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":true}' ]
wait "$delayed_pid"
grep -q '"boolean":true' "$tmp/delayed-response"
# 数据源失败和成功请求各自拥有连接；失败响应不得污染紧随其后的健康 SPARQL 请求。
failed_query='ASK { ?person <https://example.test/failed> <https://example.test/Failed> }'
curl -sS -G --data-urlencode "query=$failed_query" -o "$tmp/failed-response" \
  -w '%{http_code}' "http://127.0.0.1:$port/sparql" >"$tmp/failed-status" &
failed_pid=$!
[ "$(request -G --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")" = 200 ]
wait "$failed_pid"
[ "$(cat "$tmp/failed-status")" = 500 ]
grep -q '^datasource-failure:' "$tmp/failed-response"
# curl 主动断开长查询；一秒后 PostgreSQL 不得再保留该 pg_sleep 执行单元，随后
# endpoint 仍立即接受正常请求。这同时验证取消传播与断开资源清理。
set +e
curl -sS --max-time 1 -G --data-urlencode "query=$delayed_query" \
  "http://127.0.0.1:$port/sparql" >"$tmp/disconnected-response"
disconnect_status=$?
set -e
[ "$disconnect_status" = 28 ]
sleep 1
active_delays=$(docker exec "$database" psql -U rtop -d rtop_test -Atc \
  "SELECT count(*) FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND query LIKE 'SELECT 1 AS id FROM (SELECT pg_sleep%'")
[ "$active_delays" = 0 ]
[ "$(request -G --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")" = 200 ]
kill "$server_pid"
wait "$server_pid" || true
server_pid=

# NPD fixture 直接从公开的固定上游提交下载，并按 git blob 校验。rtop 不读取或
# 依赖相邻的 intent-engine-ontop 工作树；下载物始终位于本次 gate 的临时目录。
npd_fixture="$tmp/npd"
npd_checkout="$tmp/npd-checkout"
git init -q "$npd_checkout"
git -C "$npd_checkout" remote add origin https://github.com/ontop/npd-benchmark.git
git -C "$npd_checkout" sparse-checkout init --no-cone
git -C "$npd_checkout" sparse-checkout set --no-cone \
  /data/postgres/npd.psql \
  /mappings/postgres/npd-v2-ql.obda \
  /ontology/npd-v2-ql.owl
git -c http.version=HTTP/1.1 -C "$npd_checkout" fetch --depth 1 --filter=blob:none origin \
  5b1eeb39c36c0dd5c69c835fd93d204d19be30dd
git -C "$npd_checkout" checkout -q --detach FETCH_HEAD
mkdir -p "$npd_fixture/data/postgres" "$npd_fixture/mappings/postgres" "$npd_fixture/ontology"
mv "$npd_checkout/data/postgres/npd.psql" "$npd_fixture/data/postgres/"
mv "$npd_checkout/mappings/postgres/npd-v2-ql.obda" "$npd_fixture/mappings/postgres/"
mv "$npd_checkout/ontology/npd-v2-ql.owl" "$npd_fixture/ontology/"
[ "$(git hash-object "$npd_fixture/data/postgres/npd.psql")" = "f9fd2cf2f8fcd635cf77ba678d67c971adc339e2" ]
[ "$(git hash-object "$npd_fixture/mappings/postgres/npd-v2-ql.obda")" = "0661b1930238fd2c78241af3a83702090626b3da" ]
[ "$(git hash-object "$npd_fixture/ontology/npd-v2-ql.owl")" = "8940b9109245b95204ef4ee89d15ce2a2b2dc47c" ]
docker exec "$database" psql -U rtop -d rtop_test -c 'CREATE DATABASE npd' >/dev/null
docker exec -i "$database" psql -U rtop -d npd < "$npd_fixture/data/postgres/npd.psql" >/dev/null
npd_config="$tmp/npd-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$npd_fixture/mappings/postgres/npd-v2-ql.obda\"|" \
  -e "/^mapping = /a\\ontology = \"$npd_fixture/ontology/npd-v2-ql.owl\"" \
  -e 's/^database = .*/database = "npd"/' \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$npd_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$npd_config" "127.0.0.1:$port" >"$tmp/npd-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
npd_query() {
  request --max-time 30 -X POST --data-urlencode "query=$1" \
    -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql"
}
npd_oracle() {
  docker exec "$database" psql -U rtop -d npd -At -c "$1"
}
npd_bgp_query='PREFIX npdv: <http://sws.ifi.uio.no/vocab/npd-v2#>
SELECT ?licence ?id WHERE { ?licence a npdv:ProductionLicence ; npdv:idNPD ?id . } ORDER BY ?id LIMIT 3'
[ "$(npd_query "$npd_bgp_query")" = 200 ]
jq -e '.head.vars == ["licence","id"] and .results.bindings == [{licence:{type:"uri",value:"http://sws.ifi.uio.no/data/npd-v2/licence/20756"},id:{type:"literal",value:"20756",datatype:"http://www.w3.org/2001/XMLSchema#integer"}},{licence:{type:"uri",value:"http://sws.ifi.uio.no/data/npd-v2/licence/20764"},id:{type:"literal",value:"20764",datatype:"http://www.w3.org/2001/XMLSchema#integer"}},{licence:{type:"uri",value:"http://sws.ifi.uio.no/data/npd-v2/licence/20772"},id:{type:"literal",value:"20772",datatype:"http://www.w3.org/2001/XMLSchema#integer"}}]' "$tmp/body" >/dev/null
[ "$(jq -r '.results.bindings[].id.value' "$tmp/body")" = "$(npd_oracle 'SELECT "prlNpdidLicence" FROM licence ORDER BY "prlNpdidLicence" LIMIT 3')" ]
npd_optional_query='PREFIX npdv: <http://sws.ifi.uio.no/vocab/npd-v2#>
SELECT ?licence ?id ?updated WHERE {
  VALUES ?licence { <http://sws.ifi.uio.no/data/npd-v2/licence/20756> <http://sws.ifi.uio.no/data/npd-v2/licence/20980> }
  ?licence a npdv:ProductionLicence ; npdv:idNPD ?id .
  OPTIONAL { ?licence npdv:dateUpdated ?updated . }
} ORDER BY ?id'
[ "$(npd_query "$npd_optional_query")" = 200 ]
jq -e '.head.vars == ["licence","id","updated"] and .results.bindings == [{licence:{type:"uri",value:"http://sws.ifi.uio.no/data/npd-v2/licence/20756"},id:{type:"literal",value:"20756",datatype:"http://www.w3.org/2001/XMLSchema#integer"}},{licence:{type:"uri",value:"http://sws.ifi.uio.no/data/npd-v2/licence/20980"},id:{type:"literal",value:"20980",datatype:"http://www.w3.org/2001/XMLSchema#integer"},updated:{type:"literal",value:"2012-07-04",datatype:"http://www.w3.org/2001/XMLSchema#date"}}]' "$tmp/body" >/dev/null
[ "$(jq -r '.results.bindings[] | [.id.value, (.updated.value // "")] | join("|")' "$tmp/body")" = "$(npd_oracle 'SELECT "prlNpdidLicence" || $$|$$ || COALESCE(NULLIF("prlDateUpdated", DATE $$9999-12-31$$)::text, $$$$) FROM licence WHERE "prlNpdidLicence" IN (20756, 20980) ORDER BY "prlNpdidLicence"')" ]
npd_entailment_query='PREFIX npdv: <http://sws.ifi.uio.no/vocab/npd-v2#>
SELECT ?licence ?id WHERE {
  VALUES ?licence { <http://sws.ifi.uio.no/data/npd-v2/licence/20756> }
  ?licence a npdv:Agent ; npdv:idNPD ?id .
}'
[ "$(npd_query "$npd_entailment_query")" = 200 ]
jq -e '.head.vars == ["licence","id"] and .results.bindings == [{licence:{type:"uri",value:"http://sws.ifi.uio.no/data/npd-v2/licence/20756"},id:{type:"literal",value:"20756",datatype:"http://www.w3.org/2001/XMLSchema#integer"}}]' "$tmp/body" >/dev/null
[ "$(jq -r '.results.bindings[].id.value' "$tmp/body")" = "$(npd_oracle 'SELECT "prlNpdidLicence" FROM licence WHERE "prlNpdidLicence" = 20756')" ]
kill "$server_pid"
wait "$server_pid" || true
server_pid=

fibo_fixtures="$root/tests/compat/postgres-fibo/fixtures"

# 主 FIBO 服务使用 rtop 版本控制的固定 application ontology、mapping、schema
# 和 SPARQL query；rtop 只替换部署端点，POST/form JSON 契约及 Python 消费的 binding
# 形状保持不变。金额反例明确比较聚合后与逐事实 ROUND，不能用相同数值掩盖差异。
docker exec "$database" psql -U rtop -d rtop_test -c 'CREATE DATABASE fibo' >/dev/null
docker exec -i "$database" psql -U rtop -d rtop_test < "$fibo_fixtures/generated/schema.sql"
fibo_config="$tmp/fibo-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$fibo_fixtures/generated/mapping.obda\"|" \
  -e "/^mapping = /a\\ontology = \"$fibo_fixtures/application-ontology.ttl\"" \
  -e 's/^database = .*/database = "fibo"/' \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$fibo_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$fibo_config" "127.0.0.1:$port" >"$tmp/fibo-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
fibo_query() {
  request -X POST --data-urlencode "query=$(cat "$fibo_fixtures/generated/$1.sparql")" \
    -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql"
}
[ "$(fibo_query query)" = 200 ]
jq -e '.head.vars == ["region","label","value"] and .results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/region.east"},label:{type:"literal",value:"华东"},value:{type:"literal",value:"130",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}]' "$tmp/body" >/dev/null
[ "$(fibo_query disbursement-query)" = 200 ]
jq -e '.results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/region.east"},label:{type:"literal",value:"华东"},value:{type:"literal",value:"100",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}]' "$tmp/body" >/dev/null
[ "$(fibo_query primary-attribution-query)" = 200 ]
jq -e '.results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/region.east"},label:{type:"literal",value:"华东"},value:{type:"literal",value:"150",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}]' "$tmp/body" >/dev/null
[ "$(fibo_query allocated-attribution-query)" = 200 ]
jq -e '.results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/region.east"},label:{type:"literal",value:"华东"},value:{type:"literal",value:"110",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}},{region:{type:"uri",value:"https://example.org/intent-engine/data/region/region.south"},label:{type:"literal",value:"华南"},value:{type:"literal",value:"40",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}]' "$tmp/body" >/dev/null
[ "$(fibo_query boundary-rounding-query)" = 200 ]
jq -e '.results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/region.east"},label:{type:"literal",value:"华东"},value:{type:"literal",value:"0.01",datatype:"http://www.w3.org/2001/XMLSchema#decimal"},factCount:{type:"literal",value:"3",datatype:"http://www.w3.org/2001/XMLSchema#integer"}}]' "$tmp/body" >/dev/null
[ "$(fibo_query per-fact-rounding-query)" = 200 ]
jq -e '.results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/region.east"},label:{type:"literal",value:"华东"},value:{type:"literal",value:"0.02",datatype:"http://www.w3.org/2001/XMLSchema#decimal"},factCount:{type:"literal",value:"3",datatype:"http://www.w3.org/2001/XMLSchema#integer"}}]' "$tmp/body" >/dev/null
[ "$(request -X POST --data-urlencode 'query=ASK { ?event <https://example.org/intent-engine/ontology/currency> "USD" }' -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":false}' ]
kill "$server_pid"
wait "$server_pid" || true
server_pid=

# FIBO 本体反例必须只替换 Ontop 部署的 ontology/mapping 输入，保持同一原始
# PostgreSQL schema 与 endpoint 协议。正确 subclass 为 2；删除它为 0；过强的
# equivalentClass 错将 BookingInstitution 纳入而为 3；删除 weak closeMatch 不影响
# 蕴含而仍为 2。每项都比较 HTTP JSON 的完整 RDF term。
fibo_ontology_count() {
  ontology_file=$1
  mapping_file=$2
  expected_count=$3
  fibo_variant_config="$tmp/fibo-variant-endpoint.toml"
  sed \
    -e "s|^mapping = .*|mapping = \"$mapping_file\"|" \
    -e "/^mapping = /a\\ontology = \"$ontology_file\"" \
    -e 's/^database = .*/database = "fibo"/' \
    -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
    "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$fibo_variant_config"
  RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
    "$fibo_variant_config" "127.0.0.1:$port" >"$tmp/fibo-variant-endpoint.log" 2>&1 &
  server_pid=$!
  until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
  [ "$(request -X POST --data-urlencode "query=$(cat "$fibo_fixtures/generated/fibo-loan-count-query.sparql")" \
    -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
  jq -e ".head.vars == [\"value\"] and .results.bindings == [{value:{type:\"literal\",value:\"$expected_count\",datatype:\"http://www.w3.org/2001/XMLSchema#integer\"}}]" "$tmp/body" >/dev/null
  kill "$server_pid"
  wait "$server_pid" || true
  server_pid=
}
fibo_ontology_count "$fibo_fixtures/application-ontology.ttl" \
  "$fibo_fixtures/generated/mapping.obda" 2
fibo_ontology_count "$fibo_fixtures/generated/no-loan-entailment-ontology.ttl" \
  "$fibo_fixtures/generated/mapping.obda" 0
fibo_ontology_count "$fibo_fixtures/generated/wrong-equivalence-ontology.ttl" \
  "$fibo_fixtures/generated/mapping.obda" 3
fibo_ontology_count "$fibo_fixtures/generated/no-weak-alignment-ontology.ttl" \
  "$fibo_fixtures/generated/mapping.obda" 2

# unsafe identity 是显式的 owl:sameAs 反例：只在专用 ontology/mapping 中存在，
# 查询 alpha 时会把 beta 的贷款合并，因而全 RDF term 结果从 1 变为 2。
fibo_variant_config="$tmp/fibo-unsafe-identity-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$fibo_fixtures/generated/unsafe-identity-mapping.obda\"|" \
  -e "/^mapping = /a\\ontology = \"$fibo_fixtures/generated/unsafe-identity-ontology.ttl\"" \
  -e 's/^database = .*/database = "fibo"/' \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$fibo_variant_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$fibo_variant_config" "127.0.0.1:$port" >"$tmp/fibo-unsafe-identity-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
unsafe_identity_query='PREFIX app: <https://example.org/intent-engine/ontology/>
PREFIX owl: <http://www.w3.org/2002/07/owl#>
SELECT ?canonical ?label (COUNT(DISTINCT ?loan) AS ?value) WHERE {
  VALUES ?canonical { <https://example.org/intent-engine/data/lender/lender.alpha> }
  { VALUES ?equivalent { <https://example.org/intent-engine/data/lender/lender.alpha> } }
  UNION { ?canonical owl:sameAs ?equivalent }
  UNION { ?equivalent owl:sameAs ?canonical }
  ?loan app:hasLender ?equivalent .
  ?canonical app:label ?label .
} GROUP BY ?canonical ?label'
[ "$(request -X POST --data-urlencode "query=$unsafe_identity_query" \
  -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '.head.vars == ["canonical","label","value"] and .results.bindings == [{canonical:{type:"uri",value:"https://example.org/intent-engine/data/lender/lender.alpha"},label:{type:"literal",value:"同名贷款人"},value:{type:"literal",value:"2",datatype:"http://www.w3.org/2001/XMLSchema#integer"}}]' "$tmp/body" >/dev/null
kill "$server_pid"
wait "$server_pid" || true
server_pid=

# 固定 scale 制品是与主 FIBO 隔离的 fibo_scale 数据库。先验证生成器 manifest、
# 590 行关系数据和四个质量约束，再以 30 秒 HTTP 资源门槛比较三项原始 SPARQL
# 聚合与独立 PostgreSQL SQL oracle 的完整 RDF term。
docker exec "$database" psql -U rtop -d rtop_test -c 'CREATE DATABASE fibo_scale' >/dev/null
docker exec -i "$database" psql -U rtop -d rtop_test < "$fibo_fixtures/scale/generated/facts.sql" >/dev/null
jq -e '
  .version == "fibo-scale-facts-v1"
  and .fictional == true
  and .total_rows == 590
  and .financial_fact_count == 220
  and .facts_sql_sha256 == "68d8a7c2b4a26886caee2586dfb2a662d0ba247dd115670abba80cc380fd75a6"
  and .row_counts == {balance_observation:120,booking_institution:2,disbursement_event:100,identity_dataset_version:1,identity_decision:10,lender:5,loan_contract:100,loan_participation:120,party:120,region:2,source_lender_description:10}
' "$fibo_fixtures/scale/generated/manifest.json" >/dev/null
[ "$(docker exec "$database" psql -U rtop -d fibo_scale -Atc 'SELECT (SELECT COUNT(*) FROM region)+(SELECT COUNT(*) FROM booking_institution)+(SELECT COUNT(*) FROM lender)+(SELECT COUNT(*) FROM loan_contract)+(SELECT COUNT(*) FROM balance_observation)+(SELECT COUNT(*) FROM disbursement_event)+(SELECT COUNT(*) FROM source_lender_description)+(SELECT COUNT(*) FROM identity_dataset_version)+(SELECT COUNT(*) FROM identity_decision)+(SELECT COUNT(*) FROM party)+(SELECT COUNT(*) FROM loan_participation)')" = 590 ]
[ "$(docker exec "$database" psql -U rtop -d fibo_scale -Atc "SELECT COUNT(*) FROM balance_observation a JOIN balance_observation b ON a.loan_id=b.loan_id AND a.fact_id<b.fact_id WHERE daterange(a.valid_from,a.valid_to,'[)') && daterange(b.valid_from,b.valid_to,'[)')")" = 0 ]
[ "$(docker exec "$database" psql -U rtop -d fibo_scale -Atc "SELECT COUNT(*) FROM (SELECT loan_id FROM balance_observation WHERE valid_from <= DATE '2025-12-31' AND (valid_to IS NULL OR DATE '2025-12-31' < valid_to) GROUP BY loan_id HAVING COUNT(*)<>1) invalid")" = 0 ]
fibo_scale_config="$tmp/fibo-scale-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$fibo_fixtures/generated/mapping.obda\"|" \
  -e "/^mapping = /a\\ontology = \"$fibo_fixtures/application-ontology.ttl\"" \
  -e 's/^database = .*/database = "fibo_scale"/' \
  -e "s/port = [0-9][0-9]*/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$fibo_scale_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint \
  "$fibo_scale_config" "127.0.0.1:$port" >"$tmp/fibo-scale-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
fibo_scale_query() {
  request --max-time 30 -X POST --data-urlencode "query=$(cat "$fibo_fixtures/generated/$1.sparql")" \
    -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql"
}
fibo_scale_oracle() {
  docker exec "$database" psql -U rtop -d fibo_scale -At -c "$1"
}
[ "$(fibo_scale_query query)" = 200 ]
jq -e '.head.vars == ["region","label","value"] and .results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/scale.fictional.region.east"},label:{type:"literal",value:"虚构华东"},value:{type:"literal",value:"6542397.01",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}},{region:{type:"uri",value:"https://example.org/intent-engine/data/region/scale.fictional.region.south"},label:{type:"literal",value:"虚构华南"},value:{type:"literal",value:"11766095.40",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}]' "$tmp/body" >/dev/null
[ "$(jq -r '.results.bindings[] | [.region.value | split("/")[-1], .label.value, .value.value] | join("|")' "$tmp/body")" = "$(fibo_scale_oracle "SELECT r.region_id || '|' || r.label || '|' || ROUND(SUM(b.amount), 2) FROM balance_observation b JOIN loan_contract l USING (loan_id) JOIN booking_institution i ON i.institution_id=l.booking_institution_id JOIN region r USING (region_id) WHERE b.valid_from <= DATE '2025-12-31' AND (b.valid_to IS NULL OR DATE '2025-12-31' < b.valid_to) GROUP BY r.region_id,r.label ORDER BY r.region_id")" ]
[ "$(fibo_scale_query disbursement-query)" = 200 ]
jq -e '.results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/scale.fictional.region.east"},label:{type:"literal",value:"虚构华东"},value:{type:"literal",value:"23777.00",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}},{region:{type:"uri",value:"https://example.org/intent-engine/data/region/scale.fictional.region.south"},label:{type:"literal",value:"虚构华南"},value:{type:"literal",value:"17860.00",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}]' "$tmp/body" >/dev/null
[ "$(jq -r '.results.bindings[] | [.region.value | split("/")[-1], .label.value, .value.value] | join("|")' "$tmp/body")" = "$(fibo_scale_oracle "SELECT r.region_id || '|' || r.label || '|' || ROUND(SUM(e.amount), 2) FROM disbursement_event e JOIN loan_contract l USING (loan_id) JOIN booking_institution i ON i.institution_id=l.booking_institution_id JOIN region r USING (region_id) WHERE e.occurred_at >= DATE '2025-01-01' AND e.occurred_at < DATE '2026-01-01' GROUP BY r.region_id,r.label ORDER BY r.region_id")" ]
[ "$(fibo_scale_query allocated-attribution-query)" = 200 ]
jq -e '.results.bindings == [{region:{type:"uri",value:"https://example.org/intent-engine/data/region/scale.fictional.region.east"},label:{type:"literal",value:"虚构华东"},value:{type:"literal",value:"8837752.83",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}},{region:{type:"uri",value:"https://example.org/intent-engine/data/region/scale.fictional.region.south"},label:{type:"literal",value:"虚构华南"},value:{type:"literal",value:"9470749.58",datatype:"http://www.w3.org/2001/XMLSchema#decimal"}}]' "$tmp/body" >/dev/null
[ "$(jq -r '.results.bindings[] | [.region.value | split("/")[-1], .label.value, .value.value] | join("|")' "$tmp/body")" = "$(fibo_scale_oracle "SELECT r.region_id || '|' || r.label || '|' || ROUND(SUM(b.amount*p.allocation_weight), 2) FROM balance_observation b JOIN loan_participation p USING (loan_id) JOIN party party USING (party_id) JOIN region r USING (region_id) WHERE b.valid_from <= DATE '2025-06-30' AND (b.valid_to IS NULL OR DATE '2025-06-30' < b.valid_to) AND p.valid_from <= DATE '2025-06-30' AND (p.valid_to IS NULL OR DATE '2025-06-30' < p.valid_to) AND p.role IN ('primary-borrower','co-borrower') AND p.allocation_policy='loan-balance-approved-v1' GROUP BY r.region_id,r.label ORDER BY r.region_id")" ]
kill "$server_pid"
wait "$server_pid" || true
server_pid=

# GeoSPARQLPostGISTest 的 geometry/geography 组合需要真正的 PostGIS，而不是
# postgres:17 容器中的模拟函数。固定镜像可用时，以 endpoint RDF term 验收成功
# intersection 和两个混合类型空结果；若 registry 不可用，脚本会失败，不能误报通过。
postgis_database="${database}-postgis"
docker run -d --name "$postgis_database" -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop \
  -e POSTGRES_DB=rtop_test -p 127.0.0.1::5432 docker.m.daocloud.io/postgis/postgis:17-3.5 >/dev/null
postgis_port=$(docker port "$postgis_database" 5432/tcp | sed 's/.*://')
postgis_ready=0
while [ "$postgis_ready" -lt 2 ]; do
  if docker exec "$postgis_database" psql -U rtop -d rtop_test -c 'SELECT PostGIS_Full_Version()' >/dev/null 2>&1; then
    postgis_ready=$((postgis_ready + 1))
  else
    postgis_ready=0
  fi
  sleep 1
done
docker exec -i "$postgis_database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-geospatial/init.sql"
postgis_config="$tmp/postgis-endpoint.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/tests/compat/postgres-geospatial/geospatial.obda\"|" \
  -e "s/port = [0-9][0-9]*/port = $postgis_port/" \
  "$root/tests/compat/postgres-geospatial/rtop.toml" >"$postgis_config"
RTOP_DEVELOPMENT=1 cargo run --quiet --manifest-path "$root/Cargo.toml" -- endpoint "$postgis_config" "127.0.0.1:$port" >"$tmp/postgis-endpoint.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do sleep 1; done
postgis_intersection_query=$(cat "$root/tests/compat/postgres-geospatial/intersection-2.rq")
[ "$(request -X POST --data-urlencode "query=$postgis_intersection_query" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
jq -e '.head.vars == ["v"] and .results.bindings == [{v:{type:"literal",value:"POLYGON((2 5,7 5,7 2,2 2,2 5))",datatype:"http://www.opengis.net/ont/geosparql#wktLiteral"}}]' "$tmp/body" >/dev/null
for postgis_empty_query in intersection-1 intersection-3; do
  [ "$(request -X POST --data-urlencode "query=$(cat "$root/tests/compat/postgres-geospatial/$postgis_empty_query.rq")" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
  jq -e '.head.vars == ["v"] and .results.bindings == []' "$tmp/body" >/dev/null
done
kill "$server_pid"
wait "$server_pid" || true
server_pid=
