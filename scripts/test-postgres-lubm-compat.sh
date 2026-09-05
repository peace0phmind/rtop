#!/usr/bin/env sh
set -eu

# 固定 Ontop LUBM PostgreSQL manifest 的逐条可重放验收。外部 lubm1 服务不可达，
# 因此使用本仓库版本化最小 fixture，并以原始 rsi:size 作为每项断言。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
baseline=$root/../ontop/test/docker-tests/src/test/resources/testcases-docker/virtual-mode/lubm
manifest=$baseline/manifest-pgsql.ttl
fixture=$root/tests/compat/postgres-lubm/fixture.sql
fixture_sha256=25481995e35b7ce0c965feece580b88414cc64c81e5e3288ae087707238dbd3a
name=rtop-postgres-lubm-$$

cleanup() {
  docker rm -f "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

actual_fixture_sha256=$(sha256sum "$fixture" | awk '{print $1}')
[ "$actual_fixture_sha256" = "$fixture_sha256" ] || {
  echo "LUBM fixture hash 不匹配：$actual_fixture_sha256" >&2
  exit 1
}

query_names=$(awk '
  /mf:entries/ { entries = 1 }
  entries { print }
  entries && /\)[[:space:]]*\./ { exit }
' "$manifest" | rg -o ':query-[0-9]+' | sed 's/^://')
query_count=$(printf '%s\n' "$query_names" | sed '/^$/d' | wc -l | tr -d ' ')
[ "$query_count" = 14 ] || {
  echo "LUBM manifest 应含 14 个 query，实际为 $query_count" >&2
  exit 1
}
unique_query_count=$(printf '%s\n' "$query_names" | sort -u | wc -l | tr -d ' ')
[ "$unique_query_count" = "$query_count" ] || {
  echo "LUBM manifest entry 不能重复" >&2
  exit 1
}

docker run -d --name "$name" \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test \
  -p 127.0.0.1::5432 postgres:17 >/dev/null
postgres_port=$(docker port "$name" 5432/tcp | sed 's/.*://')
until docker exec "$name" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do
  sleep 1
done
docker exec -i "$name" psql -U rtop -d rtop_test < "$fixture" >/dev/null

for query_name in $query_names; do
  number=${query_name#query-}
  query=$baseline/$query_name.rq
  result=$baseline/query-result-$number.ttl
  expected=$(sed -n 's/.*rsi:size[[:space:]]*"\([0-9][0-9]*\)".*/\1/p' "$result")
  [ -n "$expected" ] || {
    echo "$query_name 缺少原始 rsi:size" >&2
    exit 1
  }
  actual=$(RTOP_POSTGRES_PORT="$postgres_port" cargo run --quiet -- \
    query "$root/tests/compat/postgres-lubm/rtop.toml" "$query" 2>/dev/null | wc -l | tr -d ' ')
  [ "$actual" = "$expected" ] || {
    echo "$query_name 行数不匹配：actual=$actual expected=$expected" >&2
    exit 1
  }
  printf 'LUBM %s rows=%s\n' "$query_name" "$actual"
done
