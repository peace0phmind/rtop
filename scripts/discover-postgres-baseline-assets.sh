#!/usr/bin/env sh
set -eu

# 此脚本只读取固定 Ontop 工作树。它输出 #37 的可复跑发现摘要，而不是把
# 文件数误当作功能等价分母；后续账本会把每个 collection 展开到方法/实例。
# RDB2RDF manifest 可同时包含 R2RML 与 DirectMapping entry，故二者分别计数。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
baseline="$root/../ontop"
commit=5ec07573b18513f33dfcd59ac45fe26a81f9cdbd

[ "$(git -C "$baseline" rev-parse HEAD)" = "$commit" ]

direct=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/docker-tests/src/test/java/it/unibz/inf/ontop/docker \
  | awk '/^test\/docker-tests\/src\/test\/java\/it\/unibz\/inf\/ontop\/docker\/postgres\/[^/]+\.java$/ || /\/datatypes\/PgsqlDatatypeTest\.java$/ { count++ } END { print count + 0 }')
direct_test_class_assets=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/docker-tests/src/test/java/it/unibz/inf/ontop/docker \
  | awk '/^test\/docker-tests\/src\/test\/java\/it\/unibz\/inf\/ontop\/docker\/postgres\/[^/]+\.java$/ || /\/datatypes\/PgsqlDatatypeTest\.java$/' \
  | jq -Rsc 'split("\n") | map(select(length > 0) | {asset_id:("postgres-direct-class-" + (split("/") | last | sub("\\.java$"; ""))), source_path:., classification:"in-scope"})')

annotated=$(git -C "$baseline" grep -l '@PostgreSQLLightweightTest' -- \
  test/lightweight-tests/src/test/java \
  | wc -l | tr -d ' ')
annotated_test_class_assets=$(git -C "$baseline" grep -l '@PostgreSQLLightweightTest' -- \
  test/lightweight-tests/src/test/java \
  | jq -Rsc 'split("\n") | map(select(length > 0) | {asset_id:("postgres-lightweight-class-" + (split("/") | last | sub("\\.java$"; ""))), source_path:., classification:"in-scope"})')
declared_test_method_assets=$( {
    git -C "$baseline" ls-tree -r --name-only HEAD test/docker-tests/src/test/java/it/unibz/inf/ontop/docker \
      | awk '/^test\/docker-tests\/src\/test\/java\/it\/unibz\/inf\/ontop\/docker\/postgres\/[^/]+\.java$/ || /\/datatypes\/PgsqlDatatypeTest\.java$/'
    git -C "$baseline" grep -l '@PostgreSQLLightweightTest' -- test/lightweight-tests/src/test/java
  } | sort -u | while IFS= read -r source; do
    git -C "$baseline" show "$commit:$source" \
      | awk '
          /@Ignore|@Disabled/ { ignored = 1 }
          /void[[:space:]]+test[A-Za-z0-9_]*/ {
            method = $0
            sub(/^.*void[[:space:]]+/, "", method)
            sub(/[[:space:](].*$/, "", method)
            print method "\t" (ignored ? "baseline-ignored" : "in-scope")
            ignored = 0
          }
        ' \
      | while IFS="$(printf '\t')" read -r method classification; do printf '%s\t%s\t%s\n' "$source" "$method" "$classification"; done
  done | jq -Rn '[inputs | capture("^(?<source_path>.*)\\t(?<method>test[A-Za-z0-9_]*)\\t(?<classification>.*)$") | {asset_id:("postgres-declared-method-" + (.source_path | sub("\\.java$"; "") | gsub("/"; "-")) + "-" + .method), source_path:.source_path, method:.method, classification}]')

manifests=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/docker-tests/src/test/resources/testcases-docker \
  | awk '/manifest-pgsql\.ttl$/ { count++ } END { print count + 0 }')

manifest_entries=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/docker-tests/src/test/resources/testcases-docker \
  | rg 'manifest-pgsql\.ttl$' \
  | while IFS= read -r file; do
      count=$(git -C "$baseline" show "$commit:$file" | rg -c 'qt:query' || true)
      classification=in-scope
      # PgsqlDatatypeTest.parameters() 明确把 general-Type: all 放入 IGNORE；
      # 该 manifest 不能被误计为 Rust passed，也不能从发现分母中静默消失。
      case "$file" in
        test/docker-tests/src/test/resources/testcases-docker/general/manifest-pgsql.ttl)
          classification=baseline-ignored ;;
      esac
      printf '%s\t%s\t%s\n' "$count" "$file" "$classification"
    done \
  | jq -Rn '[inputs | capture("^(?<count>[0-9]+)\\t(?<source_path>.*)\\t(?<classification>.*)$") | {asset_id:("docker-postgresql-manifest-" + (.source_path | split("/") | .[-2])), source_path, entries:(.count | tonumber), classification}]')
manifest_entry_count=$(printf '%s\n' "$manifest_entries" | jq 'map(.entries) | add')

rdb2rdf_manifests=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/rdb2rdf-compliance/src/test/resources \
  | awk '/\/manifest\.ttl$/ { count++ } END { print count + 0 }')
rdb2rdf_assets=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/rdb2rdf-compliance/src/test/resources \
  | sed -n 's#.*resources/\(D[0-9][0-9][0-9]\)/manifest\.ttl#\1#p' \
  | while IFS= read -r id; do
      source="test/rdb2rdf-compliance/src/test/resources/$id/manifest.ttl"
      r2rml_entries=$(git -C "$baseline" show "$commit:$source" | rg -c 'a rdb2rdftest:R2RML' || echo 0)
      direct_mapping_entries=$(git -C "$baseline" show "$commit:$source" | rg -c 'a rdb2rdftest:DirectMapping' || echo 0)
      printf '%s\t%s\t%s\n' "$id" "$r2rml_entries" "$direct_mapping_entries"
    done \
  | jq -Rn '[inputs | capture("^(?<id>D[0-9]{3})\\t(?<r2rml_entries>[0-9]+)\\t(?<direct_mapping_entries>[0-9]+)$") | {asset_id:("rdb2rdf-manifest-" + .id), source_path:("test/rdb2rdf-compliance/src/test/resources/" + .id + "/manifest.ttl"), r2rml_entries:(.r2rml_entries | tonumber), direct_mapping_entries:(.direct_mapping_entries | tonumber), classification:"in-scope"}]')
r2rml_manifests=$(printf '%s\n' "$rdb2rdf_assets" | jq '[.[] | select(.r2rml_entries > 0)] | length')
direct_mapping_manifests=$(printf '%s\n' "$rdb2rdf_assets" | jq '[.[] | select(.direct_mapping_entries > 0)] | length')
direct_mapping_entries=$(printf '%s\n' "$rdb2rdf_assets" | jq 'map(.direct_mapping_entries) | add')

sparql_manifests=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/sparql-compliance/src/test/resources \
  | awk '/\/manifest\.ttl$/ { count++ } END { print count + 0 }')
sparql_manifest_assets=$(git -C "$baseline" ls-tree -r --name-only HEAD \
  test/sparql-compliance/src/test/resources \
  | awk '/\/manifest\.ttl$/' \
  | jq -Rsc 'split("\n") | map(select(length > 0) | {asset_id:("sparql-manifest-" + (gsub("/"; "-") | sub("\\.ttl$"; ""))), source_path:., classification:"discovery-only"})')

cli_sources=$(git -C "$baseline" ls-tree -r --name-only HEAD client/cli/src/main/java \
  | awk '/\/Ontop[A-Za-z]+\.java$/ { count++ } END { print count + 0 }')

http_controllers=$(git -C "$baseline" ls-tree -r --name-only HEAD client/endpoint/src/main/java \
  | awk '/controllers\/.*Controller\.java$/ { count++ } END { print count + 0 }')

method_count() {
  git -C "$baseline" show "$commit:$1" \
    | awk '/^[[:space:]]*(public|protected|private)?[[:space:]]*(static[[:space:]]+)?void[[:space:]]+test[A-Za-z0-9_]*[[:space:]]*\(/ { count++ } END { print count + 0 }'
}

bind_methods=$(method_count test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractBindTestWithFunctions.java)
left_join_methods=$(method_count test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractLeftJoinProfTest.java)
distinct_aggregate_methods=$(method_count test/docker-tests/src/test/java/it/unibz/inf/ontop/docker/AbstractDistinctInAggregateTest.java)
cast_methods=$(method_count test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/AbstractCastFunctionsTest.java)
nested_methods=$(method_count test/lightweight-tests/src/test/java/it/unibz/inf/ontop/docker/lightweight/AbstractNestedDataTest.java)

jq -n \
  --arg ontop_commit "$commit" \
  --argjson direct "$direct" \
  --argjson direct_test_class_assets "$direct_test_class_assets" \
  --argjson annotated "$annotated" \
  --argjson annotated_test_class_assets "$annotated_test_class_assets" \
  --argjson declared_test_method_assets "$declared_test_method_assets" \
  --argjson manifests "$manifests" \
  --argjson manifest_entries "$manifest_entries" \
  --argjson manifest_entry_count "$manifest_entry_count" \
  --argjson rdb2rdf_manifests "$rdb2rdf_manifests" \
  --argjson r2rml_manifests "$r2rml_manifests" \
  --argjson direct_mapping_manifests "$direct_mapping_manifests" \
  --argjson direct_mapping_entries "$direct_mapping_entries" \
  --argjson rdb2rdf_assets "$rdb2rdf_assets" \
  --argjson sparql_manifests "$sparql_manifests" \
  --argjson sparql_manifest_assets "$sparql_manifest_assets" \
  --argjson cli_sources "$cli_sources" \
  --argjson http_controllers "$http_controllers" \
  --argjson bind_methods "$bind_methods" \
  --argjson left_join_methods "$left_join_methods" \
  --argjson distinct_aggregate_methods "$distinct_aggregate_methods" \
  --argjson cast_methods "$cast_methods" \
  --argjson nested_methods "$nested_methods" \
  '{ontop_commit:$ontop_commit,
  direct_test_class_assets:$direct_test_class_assets,
  annotated_test_class_assets:$annotated_test_class_assets,
  declared_test_method_assets:$declared_test_method_assets,
  collections:[
    {id:"docker-postgresql-direct-classes", count:$direct, unit:"Java test class; inherited methods must be expanded"},
    {id:"lightweight-postgresql-annotated-classes", count:$annotated, unit:"Java test class selected by meta-annotation; inherited methods must be expanded"},
    {id:"docker-postgresql-manifests", count:$manifests, unit:"manifest file; each query entry is an asset"},
    {id:"docker-postgresql-manifest-entries", count:$manifest_entry_count, unit:"qt:query instance; each must be classified independently"},
    {id:"rdb2rdf-compliance-manifests", count:$rdb2rdf_manifests, unit:"R2RML or Direct Mapping manifest; each entry must be classified"},
    {id:"r2rml-compliance-manifests", count:$r2rml_manifests, unit:"R2RML manifest file; PostgreSQL applicability must be classified per entry"},
    {id:"direct-mapping-compliance-manifests", count:$direct_mapping_manifests, unit:"RDB2RDF Direct Mapping manifest; PostgreSQL applicability must be classified per entry"},
    {id:"direct-mapping-compliance-entries", count:$direct_mapping_entries, unit:"DirectMapping test entry; each must have an independent asset ID"},
    {id:"sparql-compliance-manifests", count:$sparql_manifests, unit:"discovery input only; entries are not automatically PostgreSQL scope"},
    {id:"cli-command-sources", count:$cli_sources, unit:"CLI command source; external tasks must be classified"},
    {id:"http-controller-sources", count:$http_controllers, unit:"HTTP route source; external routes must be classified"}
  ], sparql_manifest_assets:$sparql_manifest_assets,
  rdb2rdf_manifest_assets:$rdb2rdf_assets,
  r2rml_manifest_assets:[$rdb2rdf_assets[] | select(.r2rml_entries > 0)],
  direct_mapping_manifest_assets:[$rdb2rdf_assets[] | select(.direct_mapping_entries > 0)],
  docker_postgresql_manifest_entries:$manifest_entries,
  delivery_assets:[
    {asset_id:"cli-query", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopQuery.java", classification:"in-scope"},
    {asset_id:"cli-materialize", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopMaterialize.java", classification:"in-scope"},
    {asset_id:"cli-bootstrap", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopBootstrap.java", classification:"in-scope"},
    {asset_id:"cli-validate", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopValidate.java", classification:"in-scope"},
    {asset_id:"cli-endpoint", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopEndpoint.java", classification:"in-scope"},
    {asset_id:"cli-extract-db-metadata", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopExtractDBMetadata.java", classification:"in-scope"},
    {asset_id:"cli-compile", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopCompile.java", classification:"in-scope"},
    {asset_id:"cli-mapping-interoperability", source_path:"client/cli/src/main/java/it/unibz/inf/ontop/cli/OntopMappingOntologyRelatedCommand.java", classification:"in-scope"},
    {asset_id:"http-sparql", source_path:"client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/SparqlQueryController.java", classification:"in-scope"},
    {asset_id:"http-reformulate", source_path:"client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/ReformulateController.java", classification:"in-scope"},
    {asset_id:"http-ontology", source_path:"client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/OntologyFetcherController.java", classification:"in-scope"},
    {asset_id:"http-predefined", source_path:"client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/PredefinedQueryController.java", classification:"in-scope"},
    {asset_id:"http-portal", source_path:"client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/PortalController.java", classification:"excluded"},
    {asset_id:"http-portal-config", source_path:"client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/PortalConfigController.java", classification:"excluded"},
    {asset_id:"http-auto-restart", source_path:"client/endpoint/src/main/java/it/unibz/inf/ontop/endpoint/controllers/AutoRestartController.java", classification:"excluded"}
  ], inherited_method_collections:[
    {id:"abstract-bind-test-with-functions", count:$bind_methods, unit:"inherited Java test method; ignore status must be recorded per method"},
    {id:"abstract-left-join-prof", count:$left_join_methods, unit:"inherited Java test method"},
    {id:"abstract-distinct-in-aggregate", count:$distinct_aggregate_methods, unit:"inherited Java test method"},
    {id:"abstract-cast-functions", count:$cast_methods, unit:"inherited Java test method"},
    {id:"abstract-nested-data", count:$nested_methods, unit:"inherited Java test method"}
  ]}'
