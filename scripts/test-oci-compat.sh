#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
image=rtop-oci-gate:local
network=rtop-oci-network-$$
database=rtop-oci-postgres-$$
endpoint=rtop-oci-endpoint-$$
requested_port=${RTOP_OCI_PORT:-}
tmp=$(mktemp -d)
cleanup() {
  docker rm -f "$endpoint" "$database" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

if ! docker build --pull=false -t "$image" "$root" >"$tmp/build.log" 2>&1; then
  cat "$tmp/build.log" >&2
  if grep -Eqi 'registry|pull access denied|manifest unknown|i/o timeout|TLS handshake timeout|no such host' "$tmp/build.log"; then
    printf '%s\n' 'OCI gate blocked: required image registry is unavailable; gate is not marked as passed.' >&2
  fi
  exit 1
fi
docker image inspect "$image" --format '{{json .Config.Healthcheck}}' | grep -q 'healthz'
docker image inspect "$image" --format '{{json .Config.Entrypoint}} {{json .Config.Cmd}}' | grep -q 'endpoint'
if docker image inspect "$image" --format '{{json .Config.Env}}' | grep -qi 'java\|jre\|jdk'; then
  exit 1
fi
if docker run --rm --entrypoint /bin/sh "$image" -c 'command -v java'; then
  exit 1
fi

docker network create "$network" >/dev/null
docker run -d --name "$database" --network "$network" --network-alias postgres \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test postgres:17 >/dev/null
until docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do sleep 1; done
docker exec -i "$database" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-query-kinds/init.sql"

cp "$root/tests/compat/postgres-query-kinds/mapping.obda" "$tmp/mapping.obda"
cp "$root/tests/compat/postgres-query-kinds/ask.rq" "$tmp/ask.rq"
cp "$root/tests/compat/postgres-query-kinds/secret" "$tmp/secret"
sed \
  -e 's/host = "127.0.0.1"/host = "postgres"/' \
  -e 's/port = 55432//' \
  -e 's/password = "rtop"/password_file = "secret"/' \
  "$root/tests/compat/postgres-query-kinds/rtop.toml" > "$tmp/rtop.toml"
chmod 755 "$tmp"
chmod 644 "$tmp/mapping.obda" "$tmp/ask.rq" "$tmp/secret" "$tmp/rtop.toml"

if [ -n "$requested_port" ]; then
  published_port="127.0.0.1:$requested_port:8080"
else
  published_port='127.0.0.1::8080'
fi
docker run -d --name "$endpoint" --network "$network" -p "$published_port" \
  -v "$tmp:/etc/rtop:ro" "$image" >/dev/null
port=$(docker port "$endpoint" 8080/tcp | sed -n 's/.*:\([0-9][0-9]*\)$/\1/p')
[ -n "$port" ]
until curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; do sleep 1; done
ask='ASK { ?person <https://example.test/type> <https://example.test/Person> }'
actual=$(curl -sSG --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")
if [ "$actual" != '{"head":{},"boolean":true}' ]; then
  printf '%s\n' "$actual" >&2
  docker logs "$endpoint" >&2 || true
  exit 1
fi
actual=$(docker run --rm --network "$network" --entrypoint rtop -v "$tmp:/etc/rtop:ro" \
  "$image" query /etc/rtop/rtop.toml /etc/rtop/ask.rq)
[ "$actual" = true ]

# endpoint 在启动时无需连接数据库；错误配置和不可达 PostgreSQL 必须在 /sparql
# 请求中返回稳定诊断，且 healthcheck 仍只反映进程可用性。
docker rm -f "$endpoint" >/dev/null
sed '/password_file = "secret"/d' "$tmp/rtop.toml" > "$tmp/invalid.toml"
docker run -d --name "$endpoint" --network "$network" -p "$published_port" \
  -v "$tmp:/etc/rtop:ro" "$image" /etc/rtop/invalid.toml >/dev/null
port=$(docker port "$endpoint" 8080/tcp | sed -n 's/.*:\([0-9][0-9]*\)$/\1/p')
[ -n "$port" ]
until curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; do sleep 1; done
actual=$(curl -sSG --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")
printf '%s\n' "$actual" | grep -q '^invalid-config:'

docker rm -f "$endpoint" >/dev/null
sed 's/host = "postgres"/host = "postgres-unreachable"/' "$tmp/rtop.toml" > "$tmp/unreachable.toml"
docker run -d --name "$endpoint" --network "$network" -p "$published_port" \
  -v "$tmp:/etc/rtop:ro" "$image" /etc/rtop/unreachable.toml >/dev/null
port=$(docker port "$endpoint" 8080/tcp | sed -n 's/.*:\([0-9][0-9]*\)$/\1/p')
[ -n "$port" ]
until curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; do sleep 1; done
actual=$(curl -sSG --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")
printf '%s\n' "$actual" | grep -q '^datasource-failure:'
