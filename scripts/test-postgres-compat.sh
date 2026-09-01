#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
name=rtop-postgres-compat-$$
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM
docker run -d --name "$name" -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test -p 55432:5432 postgres:17 >/dev/null
until docker exec "$name" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do sleep 1; done
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-minimal/init.sql"
actual=$(cd "$root" && cargo run --quiet -- query tests/compat/postgres-minimal/rtop.toml tests/compat/postgres-minimal/query.rq)
expected=$(cat "$root/tests/compat/postgres-minimal/expected.txt")
[ "$actual" = "$expected" ]
