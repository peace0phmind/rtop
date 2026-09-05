#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
database=rtop-delivery-compat-$$
port=${RTOP_DELIVERY_PORT:-18080}
tmp=$(mktemp -d)
server_pid=
cleanup() {
  [ -z "$server_pid" ] || kill "$server_pid" >/dev/null 2>&1 || true
  [ -z "$server_pid" ] || wait "$server_pid" >/dev/null 2>&1 || true
  docker rm -f "$database" >/dev/null 2>&1 || true
  rmdir "$tmp" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

docker run -d --name "$database" -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop \
  -e POSTGRES_DB=rtop_test -p 127.0.0.1::5432 postgres:17 >/dev/null
postgres_port=$(docker port "$database" 5432/tcp | sed 's/.*://')
until docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do sleep 1; done
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-query-kinds/init.sql"
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D001/create.sql"

query_config="$tmp/query.toml"
sed \
  -e "s|mapping = \"mapping.obda\"|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$query_config"
http_config="$tmp/http.toml"
sed \
  -e "s|mapping = \"../postgres-query-kinds/mapping.obda\"|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|" \
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
(cd "$root" && cargo run --quiet -- mapping to-r2rml "$root/tests/compat/postgres-query-kinds/duplicate-mapping-id.obda" "$tmp/duplicate-mapping-id.ttl" --force)
[ "$(grep -c '^<rtop-triples-map-' "$tmp/duplicate-mapping-id.ttl")" = 2 ]
grep -q 'rr:predicate <https://example.test/type>' "$tmp/duplicate-mapping-id.ttl"
grep -q 'rr:predicate <https://example.test/label>' "$tmp/duplicate-mapping-id.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/duplicate-mapping-id.ttl\"|" "$query_config" >"$tmp/duplicate-mapping-id.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/duplicate-mapping-id.toml" tests/compat/postgres-query-kinds/duplicate-mapping-id.rq)
[ "$actual" = true ]
(cd "$root" && cargo run --quiet -- mapping v1-to-v3 "$root/tests/compat/postgres-query-kinds/v1-to-v3.obda" "$tmp/v1-to-v3.obda")
! grep -q '^\[SourceDeclaration\]$' "$tmp/v1-to-v3.obda"
grep -q 'query_people.id AS id' "$tmp/v1-to-v3.obda"
grep -q 'jdbc.url=jdbc:postgresql://legacy.invalid/rtop_test' "$tmp/v1-to-v3.properties"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/v1-to-v3.obda\"|" "$query_config" >"$tmp/v1-to-v3.toml"
actual=$(cd "$root" && cargo run --quiet -- query "$tmp/v1-to-v3.toml" tests/compat/postgres-query-kinds/ask.rq)
[ "$actual" = true ]
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
grep -q 'source[[:space:]]*SELECT \* FROM query_people WHERE id = 1' "$tmp/v1-to-v3-simplify-projection.obda"
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
[ "$(request "http://127.0.0.1:$port/predefined/person?person=not-an-iri")" = 400 ]
grep -q '不是有效 IRI' "$tmp/body"

ask='ASK { ?person <https://example.test/type> <https://example.test/Person> }'
select='SELECT ?person { ?person <https://example.test/type> <https://example.test/Person> }'

[ "$(request -G --data-urlencode "query=$ask" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
grep -qi '^Content-Type: application/sparql-results+json; charset=utf-8' "$tmp/headers"
grep -qi '^Cache-Control: no-store' "$tmp/headers"
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":true}' ]

[ "$(request -X POST --data-urlencode "query=$ask" -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":true}' ]

[ "$(request -X POST --data-binary "$ask" -H 'Content-Type: application/sparql-query' -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")" = 200 ]
[ "$(cat "$tmp/body")" = '{"head":{},"boolean":true}' ]

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
