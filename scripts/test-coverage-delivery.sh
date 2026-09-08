#!/usr/bin/env sh
set -eu

# #118 的真实交付插桩切片。它复用 delivery gate 的固定 PostgreSQL 输入与断言，
# 但直接运行 caller 提供的已插桩 rtop 二进制，以使 CLI、配置和 HTTP server profile
# 与 Rust tests / PostgreSQL CLI 位于同一 LLVM collection。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
: "${RTOP_COVERAGE_BINARY:?需要已插桩的 rtop 二进制}"
: "${RTOP_COVERAGE_PROFILE_DIR:?需要 LLVM profile 目录}"
[ -x "$RTOP_COVERAGE_BINARY" ] || {
  echo "coverage delivery gate: rtop binary is not executable" >&2
  exit 1
}

database=rtop-coverage-delivery-$$
tmp=$(mktemp -d)
server_pid=
port=${RTOP_COVERAGE_DELIVERY_PORT:-18081}
cleanup() {
  [ -z "$server_pid" ] || kill "$server_pid" >/dev/null 2>&1 || true
  [ -z "$server_pid" ] || wait "$server_pid" >/dev/null 2>&1 || true
  docker rm -f "$database" >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

profile_count() {
  find "$RTOP_COVERAGE_PROFILE_DIR" -name '*.profraw' -type f -print | wc -l | tr -d ' '
}

before=$(profile_count)
docker run -d --name "$database" \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test \
  -p 127.0.0.1::5432 postgres:17 >/dev/null
postgres_port=$(docker port "$database" 5432/tcp | sed 's/.*://')
until docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do
  sleep 1
done
docker exec -i "$database" psql -U rtop -d rtop_test \
  < "$root/tests/compat/postgres-query-kinds/init.sql"
docker exec -i "$database" psql -U rtop -d rtop_test \
  < "$root/tests/compat/postgres-http/algebra-init.sql"

config="$tmp/query.toml"
sed \
  -e "s|mapping = \"mapping.obda\"|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$config"
algebra_config="$tmp/algebra.toml"
sed \
  -e "s|mapping = \"mapping.obda\"|mapping = \"$root/tests/compat/postgres-http/mapping-algebra-bag.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" >"$algebra_config"

if invalid_port=$(RTOP_POSTGRES_PORT=invalid "$RTOP_COVERAGE_BINARY" validate "$config" 2>&1); then
  echo "coverage delivery gate: invalid PostgreSQL port must be rejected" >&2
  exit 1
fi
printf '%s\n' "$invalid_port" | grep -F 'RTOP_POSTGRES_PORT 必须是 1 到 65535 的端口号' >/dev/null
"$RTOP_COVERAGE_BINARY" validate "$config"
"$RTOP_COVERAGE_BINARY" compile "$config"
direct_config="$tmp/direct.toml"
printf '%s\n' \
  '[direct_mapping]' \
  'base_iri = "https://direct.example.test/"' \
  'relations = ["query_people"]' \
  '' \
  '[datasource]' \
  'kind = "postgres"' \
  'host = "127.0.0.1"' \
  "port = $postgres_port" \
  'database = "rtop_test"' \
  'user = "rtop"' \
  'password = "rtop"' >"$direct_config"
[ "$("$RTOP_COVERAGE_BINARY" validate "$direct_config")" = 'Validation completed' ]

# 以 Ontop RDB2RDF D011 的原始 create.sql / 完整 N-Triples 作为 Direct Mapping
# 交付闭环：三个 relation、复合 PK 和两条 FK 必须经 PostgreSQL catalog 规划为
# 可执行 mapping，而不是只验证 TOML 配置能够解析。
docker exec -i "$database" psql -U rtop -d rtop_test \
  < "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D011/create.sql"
direct_d011_config="$tmp/direct-d011.toml"
sed "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-direct-d011/rtop.toml" >"$direct_d011_config"
direct_d011_actual=$("$RTOP_COVERAGE_BINARY" query "$direct_d011_config" \
  "$root/tests/compat/postgres-direct-d011/construct.rq" | sort)
direct_d011_expected=$(tr -d '\r' \
  < "$root/tests/compat/postgres-direct-d011/construct.expected" | sort)
[ "$direct_d011_actual" = "$direct_d011_expected" ]

[ "$("$RTOP_COVERAGE_BINARY" query "$config" "$root/tests/compat/postgres-query-kinds/ask.rq")" = true ]
select_output=$(printf '%s\n' 'SELECT ?person WHERE { ?person <https://example.test/type> <https://example.test/Person> }' \
  | "$RTOP_COVERAGE_BINARY" query "$config" -)
[ "$select_output" = '?person=<https://example.test/person/1>' ]
default_select_output=$(printf '%s\n' 'SELECT ?person WHERE { ?person <https://example.test/type> <https://example.test/Person> }' \
  | "$RTOP_COVERAGE_BINARY" query "$config")
[ "$default_select_output" = '?person=<https://example.test/person/1>' ]
[ "$("$RTOP_COVERAGE_BINARY" query "$algebra_config" "$root/tests/compat/postgres-http/sql-seam.rq")" = '?name="Ada" ?person=<https://example.test/person/1>' ]
algebra_bag_output=$("$RTOP_COVERAGE_BINARY" query "$algebra_config" "$root/tests/compat/postgres-http/algebra-bag.rq")
[ "$(printf '%s\n' "$algebra_bag_output" | wc -l | tr -d ' ')" = 4 ]
printf '%s\n' "$algebra_bag_output" | grep -F '?name="Ada" ?person=<https://example.test/person/1> ?tag="left"' >/dev/null
printf '%s\n' "$algebra_bag_output" | grep -F '?name="Ada" ?person=<https://example.test/person/1> ?tag="right"' >/dev/null
aggregate_output=$("$RTOP_COVERAGE_BINARY" query "$algebra_config" "$root/tests/compat/postgres-http/aggregate-having-order.rq")
[ "$(printf '%s\n' "$aggregate_output" | wc -l | tr -d ' ')" = 2 ]
printf '%s\n' "$aggregate_output" | grep -F '?category="profit" ?total="2.60"^^<http://www.w3.org/2001/XMLSchema#decimal>' >/dev/null
printf '%s\n' "$aggregate_output" | grep -F '?category="loss" ?total="-1.50"^^<http://www.w3.org/2001/XMLSchema#decimal>' >/dev/null
decimal_round_aggregate_output=$("$RTOP_COVERAGE_BINARY" query "$algebra_config" "$root/tests/compat/postgres-http/decimal-round-aggregate.rq")
[ "$decimal_round_aggregate_output" = '?roundAfterSum="1"^^<http://www.w3.org/2001/XMLSchema#decimal> ?sumAfterRound="1"^^<http://www.w3.org/2001/XMLSchema#decimal> ?total="1.10"^^<http://www.w3.org/2001/XMLSchema#decimal>' ]
decimal_round_output=$("$RTOP_COVERAGE_BINARY" query "$algebra_config" "$root/tests/compat/postgres-http/decimal-round.rq")
[ "$(printf '%s\n' "$decimal_round_output" | wc -l | tr -d ' ')" = 3 ]
printf '%s\n' "$decimal_round_output" | grep -F '?amount="-1.50"^^<http://www.w3.org/2001/XMLSchema#decimal> ?negativeHalf="-3"^^<http://www.w3.org/2001/XMLSchema#decimal> ?rounded="-2"^^<http://www.w3.org/2001/XMLSchema#decimal> ?sum="-1.30"^^<http://www.w3.org/2001/XMLSchema#decimal>' >/dev/null
printf '%s\n' "$decimal_round_output" | grep -F '?amount="0.10"^^<http://www.w3.org/2001/XMLSchema#decimal> ?negativeHalf="-3"^^<http://www.w3.org/2001/XMLSchema#decimal> ?rounded="0"^^<http://www.w3.org/2001/XMLSchema#decimal> ?sum="0.30"^^<http://www.w3.org/2001/XMLSchema#decimal>' >/dev/null
printf '%s\n' "$decimal_round_output" | grep -F '?amount="2.50"^^<http://www.w3.org/2001/XMLSchema#decimal> ?negativeHalf="-3"^^<http://www.w3.org/2001/XMLSchema#decimal> ?rounded="3"^^<http://www.w3.org/2001/XMLSchema#decimal> ?sum="2.70"^^<http://www.w3.org/2001/XMLSchema#decimal>' >/dev/null
native_obda_terms_null_output=$("$RTOP_COVERAGE_BINARY" query "$algebra_config" "$root/tests/compat/postgres-http/native-obda-terms-null.rq")
[ "$(printf '%s\n' "$native_obda_terms_null_output" | wc -l | tr -d ' ')" = 2 ]
printf '%s\n' "$native_obda_terms_null_output" | grep -Fx '?label="visible" ?person=<https://example.test/nullable/1>' >/dev/null
printf '%s\n' "$native_obda_terms_null_output" | grep -Fx '?label="Ada"@en ?person=<https://example.test/person/1>' >/dev/null
graph_output=$(printf '%s\n' 'CONSTRUCT { ?person <https://example.test/type> <https://example.test/Person> } WHERE { ?person <https://example.test/type> <https://example.test/Person> }' \
  | "$RTOP_COVERAGE_BINARY" query "$config" -)
[ "$graph_output" = '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' ]
"$RTOP_COVERAGE_BINARY" materialize "$config" "$tmp/materialized.ttl" >"$tmp/materialize.stdout"
grep -q '^NR of TRIPLES: 1$' "$tmp/materialize.stdout"
grep -q '^<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> \.$' "$tmp/materialized.ttl"
materialized_nquads=$("$RTOP_COVERAGE_BINARY" materialize "$config" - nquads)
[ "$materialized_nquads" = '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' ]
materialized_default=$("$RTOP_COVERAGE_BINARY" materialize "$config")
[ "$materialized_default" = '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' ]
if materialize_write_failure=$("$RTOP_COVERAGE_BINARY" materialize \
  "$config" "$tmp/missing/materialized.ttl" 2>&1); then
  echo "coverage delivery gate: unwritable materialize output must fail" >&2
  exit 1
fi
printf '%s\n' "$materialize_write_failure" | grep -F '无法写入 materialize 输出' >/dev/null
"$RTOP_COVERAGE_BINARY" extract-db-metadata "$config" "$tmp/metadata.json"
jq -e '.relations[] | select(.name == ["\"query_people\""])' "$tmp/metadata.json" >/dev/null
metadata_stdout=$("$RTOP_COVERAGE_BINARY" extract-db-metadata "$config" -)
printf '%s\n' "$metadata_stdout" | jq -e '.relations[] | select(.name == ["\"query_people\""])' >/dev/null
metadata_default=$("$RTOP_COVERAGE_BINARY" extract-db-metadata "$config")
printf '%s\n' "$metadata_default" | jq -e '.relations[] | select(.name == ["\"query_people\""])' >/dev/null
if metadata_write_failure=$("$RTOP_COVERAGE_BINARY" extract-db-metadata \
  "$config" "$tmp/missing/metadata.json" 2>&1); then
  echo "coverage delivery gate: unwritable metadata output must fail" >&2
  exit 1
fi
printf '%s\n' "$metadata_write_failure" | grep -F '无法写入 metadata 输出' >/dev/null
"$RTOP_COVERAGE_BINARY" bootstrap "$config" https://bootstrap.example "$tmp/bootstrap.obda" "$tmp/bootstrap.ttl"
grep -q '^mappingId bootstrap-query_people$' "$tmp/bootstrap.obda"
grep -q '^<https://bootstrap.example/query_people> a owl:Class \.$' "$tmp/bootstrap.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/bootstrap.obda\"|" \
  "$config" >"$tmp/bootstrap.toml"
[ "$("$RTOP_COVERAGE_BINARY" query "$tmp/bootstrap.toml" "$root/tests/compat/postgres-query-kinds/bootstrap.rq")" = true ]
if bootstrap_write_failure=$("$RTOP_COVERAGE_BINARY" bootstrap "$config" https://bootstrap.example \
  "$tmp/missing/bootstrap.obda" "$tmp/missing/bootstrap.ttl" 2>&1); then
  echo "coverage delivery gate: unwritable bootstrap output must fail" >&2
  exit 1
fi
printf '%s\n' "$bootstrap_write_failure" | grep -F '无法写入 bootstrap mapping' >/dev/null
"$RTOP_COVERAGE_BINARY" mapping to-r2rml \
  "$root/tests/compat/postgres-query-kinds/mapping.obda" "$tmp/mapping.ttl" --force
grep -q 'rr:predicate <https://example.test/type>' "$tmp/mapping.ttl"
"$RTOP_COVERAGE_BINARY" mapping to-r2rml \
  "$root/tests/compat/postgres-query-kinds/named-graph.obda" "$tmp/named-graph.ttl" --force
grep -q 'rr:graphMap \[ rr:constant <https://example.test/graph/people> \]' "$tmp/named-graph.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/named-graph.ttl\"|" \
  "$config" >"$tmp/named-graph.toml"
[ "$("$RTOP_COVERAGE_BINARY" query "$tmp/named-graph.toml" "$root/tests/compat/postgres-query-kinds/named-graph.rq")" = true ]
"$RTOP_COVERAGE_BINARY" mapping to-r2rml \
  "$root/tests/compat/postgres-query-kinds/template-graph.obda" "$tmp/template-graph.ttl" --force
grep -q 'rr:graphMap \[ rr:template "https://example.test/graph/people/{id}" \]' "$tmp/template-graph.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/template-graph.ttl\"|" \
  "$config" >"$tmp/template-graph.toml"
[ "$("$RTOP_COVERAGE_BINARY" query "$tmp/template-graph.toml" "$root/tests/compat/postgres-query-kinds/template-graph.rq")" = true ]
"$RTOP_COVERAGE_BINARY" mapping to-r2rml \
  "$root/tests/compat/postgres-query-kinds/typed-blank-node.obda" "$tmp/typed-blank-node.ttl" --force
grep -q 'rr:termType rr:BlankNode' "$tmp/typed-blank-node.ttl"
grep -q 'rr:datatype <http://www.w3.org/2001/XMLSchema#integer>' "$tmp/typed-blank-node.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/typed-blank-node.ttl\"|" \
  "$config" >"$tmp/typed-blank-node.toml"
[ "$("$RTOP_COVERAGE_BINARY" query "$tmp/typed-blank-node.toml" "$root/tests/compat/postgres-query-kinds/typed-blank-node.rq")" = true ]
if to_r2rml_without_force=$("$RTOP_COVERAGE_BINARY" mapping to-r2rml \
  "$root/tests/compat/postgres-query-kinds/mapping.obda" "$tmp/missing-force.ttl" 2>&1); then
  echo "coverage delivery gate: to-r2rml without --force must fail" >&2
  exit 1
fi
printf '%s\n' "$to_r2rml_without_force" | grep -F '请显式传入 --force' >/dev/null
"$RTOP_COVERAGE_BINARY" mapping to-obda \
  "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D001/r2rmla.ttl" "$tmp/d001.obda"
grep -q '^mappingId[[:space:]]*rtop-r2rml-1$' "$tmp/d001.obda"
"$RTOP_COVERAGE_BINARY" mapping to-obda \
  "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D006/r2rmla.ttl" "$tmp/d006.obda"
grep -q 'GRAPH <http://example.com/graph/student>' "$tmp/d006.obda"
grep -q '<http://example.com/BadStudent> <http://example.com/description> "Bad Student"' "$tmp/d006.obda"
"$RTOP_COVERAGE_BINARY" mapping to-obda \
  "$root/tests/compat/postgres-query-kinds/relative-r2rml.ttl" "$tmp/relative-r2rml.obda"
grep -q '<http://example.com/base/person/{id}> <https://example.test/base/kind> <https://example.test/base/Person>' "$tmp/relative-r2rml.obda"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/relative-r2rml.obda\"|" \
  "$config" >"$tmp/relative-r2rml.toml"
[ "$("$RTOP_COVERAGE_BINARY" query "$tmp/relative-r2rml.toml" "$root/tests/compat/postgres-query-kinds/relative-r2rml.rq")" = true ]
"$RTOP_COVERAGE_BINARY" mapping pretty-r2rml \
  "$root/tests/compat/postgres-query-kinds/relative-r2rml.ttl" "$tmp/pretty-r2rml.ttl"
grep -q '<http://www.w3.org/ns/r2rml#template> "person/{id}"' "$tmp/pretty-r2rml.ttl"
"$RTOP_COVERAGE_BINARY" mapping pretty-r2rml \
  "$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D000/r2rml.ttl" "$tmp/d000-pretty-r2rml.ttl"
grep -q 'http://www.w3.org/ns/r2rml#TriplesMap' "$tmp/d000-pretty-r2rml.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/d000-pretty-r2rml.ttl\"|" \
  "$config" >"$tmp/d000-pretty-r2rml.toml"
"$RTOP_COVERAGE_BINARY" validate "$tmp/d000-pretty-r2rml.toml"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$root/../ontop/test/rdb2rdf-compliance/src/test/resources/D014/r2rmlb.ttl\"|" \
  "$config" >"$tmp/d014-r2rml.toml"
"$RTOP_COVERAGE_BINARY" validate "$tmp/d014-r2rml.toml"
"$RTOP_COVERAGE_BINARY" mapping v1-to-v3 \
  "$root/tests/compat/postgres-query-kinds/v1-to-v3-simplify-projection.obda" "$tmp/v1.obda" --simplify-projection
grep -q 'source[[:space:]][[:space:]]*SELECT \* FROM query_people' "$tmp/v1.obda"
"$RTOP_COVERAGE_BINARY" mapping v1-to-v3 \
  "$root/tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.obda" "$tmp/v1-alias.obda"
grep -q 'left_person.id AS id1, right_person.id AS id2' "$tmp/v1-alias.obda"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/v1-alias.obda\"|" \
  "$config" >"$tmp/v1-alias.toml"
[ "$("$RTOP_COVERAGE_BINARY" query "$tmp/v1-alias.toml" "$root/tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.rq")" = true ]
"$RTOP_COVERAGE_BINARY" mapping v1-to-v3 \
  "$root/tests/compat/postgres-query-kinds/v1-to-v3-r2rml.ttl" "$tmp/v1-r2rml.ttl"
grep -q 'left_person.id AS id1, right_person.id AS id2' "$tmp/v1-r2rml.ttl"
sed "s|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|mapping = \"$tmp/v1-r2rml.ttl\"|" \
  "$config" >"$tmp/v1-r2rml.toml"
[ "$("$RTOP_COVERAGE_BINARY" query "$tmp/v1-r2rml.toml" "$root/tests/compat/postgres-query-kinds/v1-to-v3-r2rml.rq")" = true ]
if legacy_source_uri=$("$RTOP_COVERAGE_BINARY" mapping v1-to-v3 \
  "$root/tests/compat/postgres-query-kinds/v1-to-v3.obda" "$tmp/rejected-v1.obda" 2>&1); then
  echo "coverage delivery gate: legacy sourceUri must fail" >&2
  exit 1
fi
printf '%s\n' "$legacy_source_uri" | grep -F 'Unknown parameter name "sourceUri"' >/dev/null

# 同一公开 CLI seam 覆盖 conversion 输出无法创建时的稳定错误契约。它们必须由
# 实际二进制生成 profile，不能只在 binary 单元测试中调用 helper。
unwritable_conversion="$tmp/missing/conversion.out"
if pretty_write_failure=$("$RTOP_COVERAGE_BINARY" mapping pretty-r2rml \
  "$root/tests/compat/postgres-query-kinds/relative-r2rml.ttl" "$unwritable_conversion" 2>&1); then
  echo "coverage delivery gate: prettify unwritable output must fail" >&2
  exit 1
fi
printf '%s\n' "$pretty_write_failure" | grep -F '无法写入 prettify 输出' >/dev/null
if to_obda_write_failure=$("$RTOP_COVERAGE_BINARY" mapping to-obda \
  "$root/tests/compat/postgres-query-kinds/relative-r2rml.ttl" "$unwritable_conversion" 2>&1); then
  echo "coverage delivery gate: to-obda unwritable output must fail" >&2
  exit 1
fi
printf '%s\n' "$to_obda_write_failure" | grep -F '无法写入 native OBDA' >/dev/null
if to_r2rml_write_failure=$("$RTOP_COVERAGE_BINARY" mapping to-r2rml \
  "$root/tests/compat/postgres-query-kinds/mapping.obda" "$unwritable_conversion" --force 2>&1); then
  echo "coverage delivery gate: to-r2rml unwritable output must fail" >&2
  exit 1
fi
printf '%s\n' "$to_r2rml_write_failure" | grep -F '无法写入 R2RML' >/dev/null
if v1_native_write_failure=$("$RTOP_COVERAGE_BINARY" mapping v1-to-v3 \
  "$root/tests/compat/postgres-query-kinds/v1-to-v3-duplicate-alias.obda" "$unwritable_conversion" 2>&1); then
  echo "coverage delivery gate: native v1-to-v3 unwritable output must fail" >&2
  exit 1
fi
printf '%s\n' "$v1_native_write_failure" | grep -F '无法写入 v1-to-v3 mapping' >/dev/null
if v1_r2rml_write_failure=$("$RTOP_COVERAGE_BINARY" mapping v1-to-v3 \
  "$root/tests/compat/postgres-query-kinds/v1-to-v3-r2rml.ttl" "$unwritable_conversion" 2>&1); then
  echo "coverage delivery gate: R2RML v1-to-v3 unwritable output must fail" >&2
  exit 1
fi
printf '%s\n' "$v1_r2rml_write_failure" | grep -F '无法写入 v1-to-v3 R2RML' >/dev/null
if invalid_materialize=$("$RTOP_COVERAGE_BINARY" materialize "$config" "$tmp/invalid.out" json 2>&1); then
  echo "coverage delivery gate: unsupported materialize format must fail" >&2
  exit 1
fi
printf '%s\n' "$invalid_materialize" | grep -F 'materialize 不支持的输出格式：json' >/dev/null
if invalid_bootstrap=$("$RTOP_COVERAGE_BINARY" bootstrap "$config" 'not-an-iri' \
  "$tmp/invalid.obda" "$tmp/invalid.ttl" 2>&1); then
  echo "coverage delivery gate: invalid bootstrap IRI must fail" >&2
  exit 1
fi
printf '%s\n' "$invalid_bootstrap" | grep -F 'bootstrap base-iri 必须为不含 # 的绝对 IRI' >/dev/null
if unknown_command=$("$RTOP_COVERAGE_BINARY" unknown "$config" 2>&1); then
  echo "coverage delivery gate: unknown command must fail" >&2
  exit 1
fi
printf '%s\n' "$unknown_command" | grep -Fx '未知命令：unknown' >/dev/null
if missing_config=$("$RTOP_COVERAGE_BINARY" query 2>&1); then
  echo "coverage delivery gate: query without config must fail" >&2
  exit 1
fi
printf '%s\n' "$missing_config" | grep -F '用法：rtop <query|validate|compile|endpoint|materialize|extract-db-metadata|bootstrap>' >/dev/null
if missing_mapping_arguments=$("$RTOP_COVERAGE_BINARY" mapping pretty-r2rml 2>&1); then
  echo "coverage delivery gate: mapping without input/output must fail" >&2
  exit 1
fi
printf '%s\n' "$missing_mapping_arguments" | grep -F '用法：rtop mapping <pretty-r2rml|v1-to-v3> <input> <output>' >/dev/null
if missing_bootstrap_arguments=$("$RTOP_COVERAGE_BINARY" bootstrap "$config" 2>&1); then
  echo "coverage delivery gate: bootstrap without required arguments must fail" >&2
  exit 1
fi
printf '%s\n' "$missing_bootstrap_arguments" | grep -F '用法：rtop bootstrap <config.toml> <base-iri> <mapping.obda> <ontology.ttl>' >/dev/null

# 复用 Ontop CastPostgreSQLTest 的 books fixture，经公开 CLI seam 覆盖 PostgreSQL
# timestamp 到 SPARQL xsd:date 的 adapter 类型转换，而不是以内存 literal 代替。
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-cast/init-mapped-datetime.sql"
cast_config="$tmp/postgres-cast.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/lightweight-tests/src/test/resources/books/books.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-cast/books.toml" >"$cast_config"
cast_output=$("$RTOP_COVERAGE_BINARY" query "$cast_config" "$root/tests/compat/postgres-cast/cast-date-from-mapped-datetime.rq")
[ "$cast_output" = '?v="1970-11-05"^^<http://www.w3.org/2001/XMLSchema#date>
?v="2011-12-08"^^<http://www.w3.org/2001/XMLSchema#date>
?v="2014-06-05"^^<http://www.w3.org/2001/XMLSchema#date>
?v="2015-09-21"^^<http://www.w3.org/2001/XMLSchema#date>' ]

# 固定 Ontop datatype-manifest 覆盖 PostgreSQL scalar wire 值；CLI formatter 以空格
# 分隔 bindings，而原始 expected 是 TSV，故仅规范化分隔符，不改写 RDF term 词法。
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-datatype-manifest/init.sql"
datatype_config="$tmp/postgres-datatype-manifest.toml"
sed \
  -e "s|mapping = \"mapping.obda\"|mapping = \"$root/tests/compat/postgres-datatype-manifest/mapping.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-datatype-manifest/rtop.toml" >"$datatype_config"
datatype_output=$("$RTOP_COVERAGE_BINARY" query "$datatype_config" "$root/tests/compat/postgres-datatype-manifest/query.rq")
datatype_expected=$(tr -d '\r' < "$root/tests/compat/postgres-datatype-manifest/expected.txt" | tr '\t' ' ')
[ "$datatype_output" = "$datatype_expected" ]
for datatype_case in manifest-columns numeric-filter character-bgp; do
  datatype_output=$("$RTOP_COVERAGE_BINARY" query "$datatype_config" "$root/tests/compat/postgres-datatype-manifest/$datatype_case.rq")
  datatype_expected=$(tr -d '\r' < "$root/tests/compat/postgres-datatype-manifest/$datatype_case.expected" | tr '\t' ' ')
  [ "$datatype_output" = "$datatype_expected" ]
done

# 固定 Ontop AbstractNestedDataTest 的 JSON、JSONB 和 PostgreSQL array 映射均必须
# 经真实 PostgreSQL source SQL 计算同一 AVG/STR/CONCAT 结果。该资产已由完整兼容
# gate 与双边 endpoint artifact 取证；这里把它纳入统一 LLVM profile，不能只让
# 独立 compatibility runner 覆盖运行时代码。
docker exec -i "$database" psql -U rtop -d rtop_test \
  < "$root/tests/compat/postgres-nested/init.sql"
nested_expected=$(tr -d '\r' < "$root/tests/compat/postgres-nested/flatten-aggregate.expected" | sort)
for nested_kind in rtop json array; do
  case "$nested_kind" in
    rtop) nested_mapping=nested-jsonb.obda ;;
    json) nested_mapping=nested-json.obda ;;
    array) nested_mapping=nested-array.obda ;;
  esac
  nested_config="$tmp/nested-$nested_kind.toml"
  sed \
    -e "s|mapping = \"nested-[^\"]*\.obda\"|mapping = \"$root/tests/compat/postgres-nested/$nested_mapping\"|" \
    -e "s/port = 55432/port = $postgres_port/" \
    "$root/tests/compat/postgres-nested/$nested_kind.toml" >"$nested_config"
  nested_actual=$("$RTOP_COVERAGE_BINARY" query "$nested_config" \
    "$root/tests/compat/postgres-nested/flatten-aggregate.rq" | sort)
  [ "$nested_actual" = "$nested_expected" ]
done

# 固定 Ontop PostgreSQL aggregate 语料：DistinctInAggregate、GroupConcat 和
# AbstractLeftJoinProfTest 分别经过不同原生 mapping（后者还装载 OWL TBox）。完整
# compatibility gate 已有这些资产；统一 profile 必须同样经过真实 CLI/runtime 路径。
docker exec -i "$database" psql -U rtop -d rtop_test \
  < "$root/tests/compat/postgres-aggregates/init.sql"
aggregate_config="$tmp/aggregates.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/distinctInAggregates/university.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-aggregates/rtop.toml" >"$aggregate_config"
gconcat_config="$tmp/gconcat.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/pgsql/gconcat/vkg.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-aggregates/gconcat.toml" >"$gconcat_config"
left_join_config="$tmp/left-join.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/redundant_join/redundant_join_fk_test.obda\"|" \
  -e "s|^ontology = .*|ontology = \"$root/../ontop/test/docker-tests/src/test/resources/redundant_join/redundant_join_fk_test.owl\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-aggregates/redundant-join.toml" >"$left_join_config"
for aggregate_query in sum-distinct avg-distinct count-distinct group-concat-distinct; do
  aggregate_actual=$("$RTOP_COVERAGE_BINARY" query "$aggregate_config" \
    "$root/tests/compat/postgres-aggregates/$aggregate_query.rq")
  aggregate_expected=$(tr -d '\r' < "$root/tests/compat/postgres-aggregates/$aggregate_query.expected")
  [ "$aggregate_actual" = "$aggregate_expected" ]
done
for aggregate_query in group-concat-language group-concat-all; do
  aggregate_actual=$("$RTOP_COVERAGE_BINARY" query "$gconcat_config" \
    "$root/tests/compat/postgres-aggregates/$aggregate_query.rq")
  aggregate_expected=$(tr -d '\r' < "$root/tests/compat/postgres-aggregates/$aggregate_query.expected")
  [ "$aggregate_actual" = "$aggregate_expected" ]
done
for aggregate_query in limit-subquery-1 empty-numeric-aggregates group-concat-optional-union; do
  aggregate_actual=$("$RTOP_COVERAGE_BINARY" query "$left_join_config" \
    "$root/tests/compat/postgres-aggregates/$aggregate_query.rq")
  aggregate_expected=$(tr -d '\r' < "$root/tests/compat/postgres-aggregates/$aggregate_query.expected")
  [ "$aggregate_actual" = "$aggregate_expected" ]
done

# 固定 Ontop PostgreSQL SPARQL bind/function 资产。mapping-input 查询使 RDF term
# 解码、BIND/FILTER/OPTIONAL 同时经过真实 source SQL；纯 VALUES 表达式仍经同一
# 发布 CLI 入口，保留除零和相对 IRI 的错误/BASE 契约。
docker exec "$database" psql -U rtop -d rtop_test \
  -c 'DROP TABLE IF EXISTS address, person, books CASCADE' >/dev/null
docker exec -i "$database" psql -U rtop -d rtop_test \
  < "$root/tests/compat/postgres-expressions/init.sql"
expression_config="$tmp/expressions.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/pgsql/bind/sparqlBindPostgreSQL.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-expressions/ofn-mapping.toml" >"$expression_config"
regex_config="$tmp/regex.toml"
sed \
  -e "s|^mapping = .*|mapping = \"$root/../ontop/test/docker-tests/src/test/resources/pgsql/regex/stockexchangeRegex.obda\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  "$root/tests/compat/postgres-expressions/regex.toml" >"$regex_config"
for expression_query in numeric-functions-mapping-input sha256-mapping-input langmatches-optional-mapping-input \
  replace-mapping-input ofn-mapping-input ofn-millis-between ofn-date-between \
  datetime-extractors-mapping-input bound-optional-mapping-input \
  term-predicates-mapping-input datatype-mapping-input regex-optional-mapping-input \
  rdf-term-equal-mapping-input same-term-mapping-input strstarts-mapping-input \
  str-before-after-mapping-input contains-bind-mapping-input logical-and-mapping-input \
  logical-and-distinct-mapping-input logical-or-mapping-input divide-mapping-input \
  concat-mapping-input substr-mapping-input encode-for-uri-mapping-input \
  strlen-mapping-input contains-filter-mapping-input strends-mapping-input \
  case-concat-mapping-input; do
  expression_actual=$("$RTOP_COVERAGE_BINARY" query "$expression_config" \
    "$root/tests/compat/postgres-expressions/$expression_query.rq")
  expression_expected=$(tr -d '\r' < "$root/tests/compat/postgres-expressions/$expression_query.expected")
  [ "$expression_actual" = "$expression_expected" ]
done
for expression_query in coalesce-arithmetic-errors iri-uri-relative; do
  expression_actual=$("$RTOP_COVERAGE_BINARY" query "$regex_config" \
    "$root/tests/compat/postgres-expressions/$expression_query.rq")
  expression_expected=$(tr -d '\r' < "$root/tests/compat/postgres-expressions/$expression_query.expected")
  [ "$expression_actual" = "$expression_expected" ]
done

# 已插桩 server 在二十五个真实 HTTP 请求后正常退出，确保 LLVM profiler 落盘。除了
# SPARQL 的 GET/form/raw body 外，覆盖 ontology、predefined、开发态 reformulate 以及
# 绑定/图结果协商；该环境变量只由 cfg(coverage) 二进制识别，发布 endpoint 不受影响。
endpoint_config="$root/tests/compat/postgres-query-kinds/coverage-endpoint.toml"
RTOP_COVERAGE_SERVER_MAX_REQUESTS=25 RTOP_DEVELOPMENT=1 RTOP_POSTGRES_PORT="$postgres_port" \
  "$RTOP_COVERAGE_BINARY" endpoint "$endpoint_config" "127.0.0.1:$port" \
  >"$tmp/server.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/health" 2>/dev/null; do
  sleep 1
done
[ "$(cat "$tmp/health")" = ok ]
ask='ASK { ?person <https://example.test/type> <https://example.test/Person> }'
response=$(curl -fsS -G --data-urlencode "query=$ask" \
  -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")
[ "$response" = '{"head":{},"boolean":true}' ]
response=$(curl -fsS -X POST --data-urlencode "query=$ask" \
  -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")
[ "$response" = '{"head":{},"boolean":true}' ]
response=$(curl -fsS -X POST --data-binary "$ask" -H 'Content-Type: application/sparql-query' \
  -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")
[ "$response" = '{"head":{},"boolean":true}' ]
response=$(curl -fsS -G --data-urlencode "query=$ask" \
  -H 'Accept: application/sparql-results+xml' "http://127.0.0.1:$port/sparql")
printf '%s\n' "$response" | grep -F '<boolean>true</boolean>' >/dev/null
response=$(curl -fsS -G --data-urlencode "query=$ask" \
  -H 'Accept: text/csv' "http://127.0.0.1:$port/sparql")
[ "$response" = 'boolean
true' ]
response=$(curl -fsS -G --data-urlencode "query=$ask" \
  -H 'Accept: text/tab-separated-values' "http://127.0.0.1:$port/sparql")
[ "$response" = '?boolean
true' ]
select='SELECT ?person WHERE { ?person <https://example.test/type> <https://example.test/Person> }'
response=$(curl -fsS -G --data-urlencode "query=$select" \
  -H 'Accept: application/sparql-results+xml' "http://127.0.0.1:$port/sparql")
printf '%s\n' "$response" | grep -F '<variable name="person"/>' >/dev/null
response=$(curl -fsS -G --data-urlencode "query=$select" \
  -H 'Accept: application/sparql-results+json' "http://127.0.0.1:$port/sparql")
printf '%s\n' "$response" | grep -F '"vars":["person"]' >/dev/null
response=$(curl -fsS -G --data-urlencode "query=$select" \
  -H 'Accept: text/csv' "http://127.0.0.1:$port/sparql")
[ "$response" = 'person
https://example.test/person/1' ]
response=$(curl -fsS -G --data-urlencode "query=$select" \
  -H 'Accept: text/tab-separated-values' "http://127.0.0.1:$port/sparql")
[ "$response" = '?person
<https://example.test/person/1>' ]
construct='CONSTRUCT { ?person <https://example.test/type> <https://example.test/Person> } WHERE { ?person <https://example.test/type> <https://example.test/Person> }'
response=$(curl -fsS -G --data-urlencode "query=$construct" \
  -H 'Accept: application/n-triples' "http://127.0.0.1:$port/sparql")
printf '%s\n' "$response" | grep -F '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' >/dev/null
response=$(curl -fsS -G --data-urlencode "query=$construct" \
  -H 'Accept: text/turtle' "http://127.0.0.1:$port/sparql")
printf '%s\n' "$response" | grep -F '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' >/dev/null
response=$(curl -fsS -G --data-urlencode "query=$construct" "http://127.0.0.1:$port/sparql")
printf '%s\n' "$response" | grep -F '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' >/dev/null
response=$(curl -fsS -G --data-urlencode "query=$select" "http://127.0.0.1:$port/ontop/reformulate")
printf '%s\n' "$response" | grep -F 'SELECT' >/dev/null
response=$(curl -fsS "http://127.0.0.1:$port/ontology")
printf '%s\n' "$response" | grep -F '<https://example.test/Person> a owl:Class .' >/dev/null
response=$(curl -fsS -G --data-urlencode 'person=https://example.test/person/1' \
  -H 'Accept: text/turtle' "http://127.0.0.1:$port/predefined/person")
printf '%s\n' "$response" | grep -F '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' >/dev/null
response=$(curl -fsS -X POST 'http://127.0.0.1:'"$port"'/predefined/person?person=not-an-iri' \
  --data-urlencode 'person=https://example.test/person/1' \
  -H 'Accept: text/turtle')
printf '%s\n' "$response" | grep -F '<https://example.test/person/1> <https://example.test/type> <https://example.test/Person> .' >/dev/null
status=$(curl -sS -o "$tmp/missing-predefined" -w '%{http_code}' \
  "http://127.0.0.1:$port/predefined/person")
[ "$status" = 400 ]
grep -F '缺少必填预定义参数：person' "$tmp/missing-predefined" >/dev/null
status=$(curl -sS -o "$tmp/unknown-predefined" -w '%{http_code}' \
  "http://127.0.0.1:$port/predefined/unknown")
[ "$status" = 404 ]
[ "$(cat "$tmp/unknown-predefined")" = 'not found' ]
status=$(curl -sS -o "$tmp/missing-query" -w '%{http_code}' "http://127.0.0.1:$port/sparql")
[ "$status" = 400 ]
grep -F '请求缺少 query 参数' "$tmp/missing-query" >/dev/null
status=$(curl -sS -X PUT -o "$tmp/wrong-method" -w '%{http_code}' \
  -G --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")
[ "$status" = 405 ]
[ "$(cat "$tmp/wrong-method")" = 'method not allowed' ]
status=$(curl -sS -o "$tmp/not-found" -w '%{http_code}' "http://127.0.0.1:$port/not-found")
[ "$status" = 404 ]
[ "$(cat "$tmp/not-found")" = 'not found' ]
status=$(curl -sS -o "$tmp/not-acceptable" -w '%{http_code}' -G --data-urlencode "query=$ask" \
  -H 'Accept: application/json' "http://127.0.0.1:$port/sparql")
[ "$status" = 406 ]
grep -F '请求的 Accept 不支持该结果格式' "$tmp/not-acceptable" >/dev/null
status=$(curl -sS -o "$tmp/invalid-predefined" -w '%{http_code}' -G \
  --data-urlencode 'person=not-an-iri' "http://127.0.0.1:$port/predefined/person")
[ "$status" = 500 ]
grep -F 'Unexpected exception: Not a valid (absolute) IRI: not-an-iri' "$tmp/invalid-predefined" >/dev/null
wait "$server_pid"
server_pid=

# 非开发 endpoint 的三个请求覆盖默认禁用的 ontology、predefined 和 reformulate
# 路由；配置来自同一固定 Ontop query fixture，只移除可选 endpoint 块。
no_endpoint_config="$tmp/no-endpoint.toml"
sed \
  -e "s|mapping = \"mapping.obda\"|mapping = \"$root/tests/compat/postgres-query-kinds/mapping.obda\"|" \
  -e "s|ontology = \"coverage-ontology.ttl\"|ontology = \"$root/tests/compat/postgres-query-kinds/coverage-ontology.ttl\"|" \
  -e "s/port = 55432/port = $postgres_port/" \
  -e '/^\[endpoint\]/,/^$/d' \
  "$endpoint_config" >"$no_endpoint_config"
RTOP_COVERAGE_SERVER_MAX_REQUESTS=3 RTOP_POSTGRES_PORT="$postgres_port" \
  "$RTOP_COVERAGE_BINARY" endpoint "$no_endpoint_config" "127.0.0.1:$port" \
  >"$tmp/no-endpoint-server.log" 2>&1 &
server_pid=$!
until curl -fsS "http://127.0.0.1:$port/healthz" >"$tmp/no-endpoint-health" 2>/dev/null; do
  sleep 1
done
status=$(curl -sS -o "$tmp/disabled-ontology" -w '%{http_code}' "http://127.0.0.1:$port/ontology")
[ "$status" = 404 ]
[ "$(cat "$tmp/disabled-ontology")" = 'not found' ]
status=$(curl -sS -o "$tmp/disabled-reformulate" -w '%{http_code}' -G \
  --data-urlencode "query=$select" "http://127.0.0.1:$port/ontop/reformulate")
[ "$status" = 404 ]
[ "$(cat "$tmp/disabled-reformulate")" = 'not found' ]
wait "$server_pid"
server_pid=

after=$(profile_count)
[ "$after" -gt "$before" ] || {
  echo "coverage delivery gate: CLI/HTTP did not produce a new LLVM profile" >&2
  exit 1
}
printf '%s\n' "coverage delivery gate: passed (profiles $before -> $after)"
