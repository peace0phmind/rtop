#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
catalog=$(mktemp)
trap 'rm -f "$catalog"' EXIT HUP INT TERM
"$root/scripts/discover-postgres-baseline-assets.sh" > "$catalog"

jq -e '
  .ontop_commit == "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd"
  and (.direct_test_class_assets | length == 18)
  and (.annotated_test_class_assets | length == 10)
  and (.declared_test_method_assets | length > 0)
  and (.rdb2rdf_manifest_assets | length == 27)
  and (.r2rml_manifest_assets | length == 21)
  and (.direct_mapping_manifest_assets | length == 24)
  and (.docker_postgresql_manifest_entries | length == 11)
  and ([.docker_postgresql_manifest_entries[] | select(.classification == "in-scope") | .entries] | add == 170)
  and ([.docker_postgresql_manifest_entries[] | select(.classification == "baseline-ignored")] | length == 1)
  and any(.docker_postgresql_manifest_entries[]; .asset_id == "docker-postgresql-manifest-general" and .classification == "baseline-ignored" and .entries == 1)
  and all(.docker_postgresql_manifest_entries[]; (.entries | type == "number" and . > 0))
  and (.sparql_manifest_assets | length == 95)
  and all(.sparql_manifest_assets[]; .classification == "discovery-only")
  and (([.direct_test_class_assets[], .annotated_test_class_assets[], .declared_test_method_assets[], .docker_postgresql_manifest_entries[], .sparql_manifest_assets[], .rdb2rdf_manifest_assets[], .delivery_assets[] | .asset_id] | length) == ([.direct_test_class_assets[], .annotated_test_class_assets[], .declared_test_method_assets[], .docker_postgresql_manifest_entries[], .sparql_manifest_assets[], .rdb2rdf_manifest_assets[], .delivery_assets[] | .asset_id] | unique | length))
  and all(.direct_test_class_assets[], .annotated_test_class_assets[], .declared_test_method_assets[], .docker_postgresql_manifest_entries[], .sparql_manifest_assets[], .rdb2rdf_manifest_assets[], .delivery_assets[]; .source_path | type == "string" and length > 0)
  and all(.direct_test_class_assets[], .annotated_test_class_assets[], .declared_test_method_assets[], .docker_postgresql_manifest_entries[], .r2rml_manifest_assets[], .direct_mapping_manifest_assets[], .delivery_assets[]; .classification | IN("in-scope", "baseline-ignored", "excluded"))
' "$catalog" >/dev/null

while IFS= read -r source; do
  git -C "$root/../ontop" cat-file -e "5ec07573b18513f33dfcd59ac45fe26a81f9cdbd:$source"
done <<EOF
$(jq -r '.direct_test_class_assets[].source_path, .annotated_test_class_assets[].source_path, .declared_test_method_assets[].source_path, .docker_postgresql_manifest_entries[].source_path, .sparql_manifest_assets[].source_path, .rdb2rdf_manifest_assets[].source_path, .delivery_assets[].source_path' "$catalog" | sort -u)
EOF

printf '%s\n' 'postgres baseline discovery: valid (18 direct classes, 10 annotated classes, 27 RDB2RDF manifests, 170 in-scope Docker manifest queries, 1 baseline-ignored)'
