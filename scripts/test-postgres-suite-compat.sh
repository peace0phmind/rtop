#!/usr/bin/env sh
set -eu

# DockerPostgresTestSuite 的独立 gate。该基线 dump 自行创建 stockexchange
# relation，故不与通用 PostgreSQL gate 共用服务，避免 fixture 名称冲突。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
name=rtop-postgres-suite-compat-$$
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

docker run -d --name "$name" \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test \
  -p 127.0.0.1::5432 postgres:17 >/dev/null
postgres_port=$(docker port "$name" 5432/tcp | sed 's/.*://')
export RTOP_POSTGRES_PORT="$postgres_port"
until docker exec "$name" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do
  sleep 1
done

# 基线 dump 预期由超级用户创建并切换独立 database；gate 已在 rtop_test 中
# 隔离运行，保留 schema/data，其余三条 database/session 指令在流中移除。
sed '/^DROP DATABASE /d; /^CREATE DATABASE /d; /^\\connect /d; /^CREATE SCHEMA public;/d' \
  "$root/../ontop/test/docker-tests/src/test/resources/dump/stockexchange-create-pgsql.sql" \
  | docker exec -i "$name" psql -U rtop -d rtop_test >/dev/null

assert_rows() {
  config=$1
  query=$2
  expected=$3
  actual=$(cd "$root" && cargo run --quiet -- query "$config" "$query" 2>/dev/null | sed '/^$/d' | wc -l | tr -d ' ')
  [ "$actual" = "$expected" ]
}

assert_ask() {
  config=$1
  query=$2
  expected=$3
  actual=$(cd "$root" && cargo run --quiet -- query "$config" "$query" 2>/dev/null | tr -d '\r\n')
  [ "$actual" = "$expected" ]
}

assert_error() {
  config=$1
  query=$2
  if (cd "$root" && cargo run --quiet -- query "$config" "$query" >/dev/null 2>&1); then
    return 1
  fi
}

# 每个执行型 manifest 都是固定基线分母。由 qt:query 动态读取，既避免手写列表
# 漏项，也在输出中留下每条原始 query 的稳定执行记录。
run_manifest() {
  manifest=$1
  config=$2
  kind=$3
  manifest_root=$(dirname "$manifest")
  sed -n 's/.*qt:query <\([^>]*\)>.*/\1/p' "$manifest" | while IFS= read -r query; do
    result=${query%.rq}.ttl
    case "$kind" in
      ask)
        expected=$(sed -n 's/.*<boolean>[[:space:]]*\(true\|false\).*/\1/p' "$manifest_root/${query%.rq}.srx")
        assert_ask "$config" "$manifest_root/$query" "$expected"
        printf 'suite %s/%s result=%s expected=%s\n' "${manifest#*$root/../ontop/}" "$query" "$expected" "$expected"
        ;;
      rows)
        expected=$(sed -n 's/.*rsi:size[[:space:]]*"\(-\{0,1\}[0-9][0-9]*\)".*/\1/p' "$manifest_root/$result")
        if [ "$expected" = -1 ]; then
          assert_error "$config" "$manifest_root/$query"
          printf 'suite %s/%s result=error expected=error\n' "${manifest#*$root/../ontop/}" "$query"
        else
          actual=$(cd "$root" && cargo run --quiet -- query "$config" "$manifest_root/$query" 2>/dev/null | sed '/^$/d' | wc -l | tr -d ' ')
          [ "$actual" = "$expected" ]
          printf 'suite %s/%s result=%s expected=%s\n' "${manifest#*$root/../ontop/}" "$query" "$actual" "$expected"
        fi
        ;;
    esac
  done
}

baseline_root="$root/../ontop/test/docker-tests/src/test/resources/testcases-docker"
run_manifest "$baseline_root/sparql/ask/manifest-pgsql.ttl" tests/compat/postgres-suite/ask.toml ask
run_manifest "$baseline_root/virtual-mode/stockexchange/filters/manifest-pgsql.ttl" tests/compat/postgres-suite/filters.toml rows
run_manifest "$baseline_root/virtual-mode/stockexchange/datatypes/manifest-pgsql.ttl" tests/compat/postgres-suite/datatypes.toml rows
run_manifest "$baseline_root/virtual-mode/stockexchange/modifiers/manifest-pgsql.ttl" tests/compat/postgres-suite/rtop.toml rows
run_manifest "$baseline_root/virtual-mode/stockexchange/simplecq/manifest-pgsql.ttl" tests/compat/postgres-suite/simplecq.toml rows
