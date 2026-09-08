#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
image=rtop-oci-gate:local
postgres_image=postgres:17@sha256:5c855ad7b85e68e48a62f34662853f38b57c1c1d80f3a927ab58034fd6d31c5e
network=rtop-oci-network-$$
database=rtop-oci-postgres-$$
endpoint=rtop-oci-endpoint-$$
rootfs_container=rtop-oci-rootfs-$$
requested_port=${RTOP_OCI_PORT:-}
tmp=$(mktemp -d)

cleanup() {
  docker rm -f "$endpoint" "$database" "$rootfs_container" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

fail_endpoint() {
  docker inspect "$endpoint" --format 'state={{.State.Status}} exit={{.State.ExitCode}} ports={{json .NetworkSettings.Ports}}' >&2 || true
  docker logs "$endpoint" >&2 || true
  exit 1
}

wait_for_healthz() {
  port=$1
  attempt=0
  while [ "$attempt" -lt 30 ]; do
    if curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; then
      return 0
    fi
    attempt=$((attempt + 1))
    sleep 1
  done
  fail_endpoint
}

endpoint_port() {
  port=$(docker port "$endpoint" 8080/tcp 2>/dev/null | sed -n 's/.*:\([0-9][0-9]*\)$/\1/p' || true)
  if [ -z "$port" ]; then
    fail_endpoint
  fi
  printf '%s\n' "$port"
}

start_endpoint() {
  config=$1
  docker run -d --name "$endpoint" --read-only --network "$network" -p "$published_port" \
    -v "$tmp:/etc/rtop:ro" "$image" "$config" >/dev/null
}

report_registry_block() {
  log_file=$1
  if grep -Eqi 'registry|pull access denied|manifest unknown|i/o timeout|TLS handshake timeout|no such host|connection refused|unexpected EOF|too many requests|HTTP 429|HTTP 403|denied: requested access' "$log_file"; then
    printf '%s\n' 'OCI gate blocked: required image registry is unavailable; gate is not marked as passed.' >&2
  fi
}

if ! docker build --pull=false -t "$image" "$root" >"$tmp/build.log" 2>&1; then
  cat "$tmp/build.log" >&2
  report_registry_block "$tmp/build.log"
  exit 1
fi

# scratch 的文件系统契约：发布层只有静态 rtop 与静态 BusyBox，平台注入文件不在 export 中。
files=$(docker create --name "$rootfs_container" "$image")
rootfs_bytes=$(docker export "$files" | wc -c | tr -d ' ')
docker export "$files" | tar -tf - | sort > "$tmp/files.txt"
grep -Fxq busybox "$tmp/files.txt"
grep -Fxq rtop "$tmp/files.txt"
if grep -Ev '^(\.dockerenv|busybox|rtop|dev/?|dev/.*|etc/?|etc/(hostname|hosts|mtab|resolv\.conf)|proc/?|sys/?)$' "$tmp/files.txt" | grep -q .; then
  exit 1
fi
[ "$rootfs_bytes" -le 20971520 ]
compressed_bytes=$(docker image inspect "$image" --format '{{.Size}}')
[ "$compressed_bytes" -le 6291456 ]
printf 'OCI image sizes: rootfs=%s bytes, compressed-layers=%s bytes\n' "$rootfs_bytes" "$compressed_bytes"

healthcheck=$(docker image inspect "$image" --format '{{json .Config.Healthcheck}}')
printf '%s\n' "$healthcheck" | grep -Fq '"/busybox","wget","-q","-O","-","http://127.0.0.1:8080/healthz"'
docker image inspect "$image" --format '{{json .Config.Entrypoint}} {{json .Config.Cmd}}' | grep -Fq '"/rtop","endpoint"'
[ "$(docker image inspect "$image" --format '{{.Config.User}}')" = 10001 ]
if docker image inspect "$image" --format '{{json .Config.Env}} {{json .Config.Labels}}' | grep -Eqi 'java|jre|jdk|postgres|password|curl'; then
  exit 1
fi
if grep -Eq '(^|/)(bin/sh|java|psql|postgres|curl)$' "$tmp/files.txt"; then
  exit 1
fi
docker run --rm --entrypoint /busybox "$image" wget --help >/dev/null
if docker run --rm --entrypoint /rtop "$image"; then
  exit 1
fi

docker network create "$network" >/dev/null
if ! docker run -d --name "$database" --network "$network" --network-alias postgres \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test "$postgres_image" >"$tmp/postgres.log" 2>&1; then
  cat "$tmp/postgres.log" >&2
  report_registry_block "$tmp/postgres.log"
  exit 1
fi
attempt=0
until docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do
  attempt=$((attempt + 1))
  [ "$attempt" -lt 30 ] || exit 1
  sleep 1
done
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
ask='ASK { ?person <https://example.test/type> <https://example.test/Person> }'

start_endpoint /etc/rtop/rtop.toml
port=$(endpoint_port)
wait_for_healthz "$port"
docker exec "$endpoint" /busybox wget -q -O - http://127.0.0.1:8080/healthz >/dev/null
actual=$(curl -sSG --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")
[ "$actual" = '{"head":{},"boolean":true}' ]
actual=$(docker run --rm --read-only --network "$network" --entrypoint /rtop -v "$tmp:/etc/rtop:ro" \
  "$image" query /etc/rtop/rtop.toml /etc/rtop/ask.rq)
[ "$actual" = true ]
docker inspect "$endpoint" --format '{{range .Mounts}}{{if eq .Destination "/etc/rtop"}}{{.RW}}{{end}}{{end}}' | grep -qx false

# endpoint 在启动时无需连接数据库；两类失败都只在 /sparql 暴露，healthcheck 仍表达进程存活。
docker rm -f "$endpoint" >/dev/null
sed '/password_file = "secret"/d' "$tmp/rtop.toml" > "$tmp/invalid.toml"
start_endpoint /etc/rtop/invalid.toml
port=$(endpoint_port)
wait_for_healthz "$port"
actual=$(curl -sSG --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")
printf '%s\n' "$actual" | grep -q '^invalid-config:'

docker rm -f "$endpoint" >/dev/null
sed 's/host = "postgres"/host = "postgres-unreachable"/' "$tmp/rtop.toml" > "$tmp/unreachable.toml"
start_endpoint /etc/rtop/unreachable.toml
port=$(endpoint_port)
wait_for_healthz "$port"
actual=$(curl -sSG --data-urlencode "query=$ask" "http://127.0.0.1:$port/sparql")
printf '%s\n' "$actual" | grep -q '^datasource-failure:'
