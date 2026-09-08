#!/usr/bin/env sh
set -eu

# 这个切片只负责真实 PostgreSQL/CLI 路径。覆盖率 runner 负责提供已插桩的
# 二进制和 LLVM_PROFILE_FILE；单独保留它可以防止 Docker fixture 细节混入
# 覆盖率工具的生命周期管理。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
: "${RTOP_COVERAGE_PROFILE_DIR:?需要 LLVM profile 目录}"

if [ "${RTOP_COVERAGE_USE_CARGO_LLVM_COV:-0}" != 1 ]; then
  : "${RTOP_COVERAGE_BINARY:?需要已插桩的 rtop 二进制}"
  [ -x "$RTOP_COVERAGE_BINARY" ] || {
    echo "coverage PostgreSQL CLI gate: rtop binary is not executable" >&2
    exit 1
  }
fi

name=rtop-coverage-postgres-cli-$$
coverage_run_report=
cleanup() {
  docker rm -f "$name" >/dev/null 2>&1 || true
  [ -z "$coverage_run_report" ] || rm -f "$coverage_run_report"
}
trap cleanup EXIT INT TERM

profile_count() {
  find "$RTOP_COVERAGE_PROFILE_DIR" -name '*.profraw' -type f -print | wc -l | tr -d ' '
}

before=$(profile_count)
docker run -d --name "$name" \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test \
  -p 127.0.0.1::5432 postgres:17 >/dev/null
postgres_port=$(docker port "$name" 5432/tcp | sed 's/.*://')
until docker exec "$name" psql -U rtop -d rtop_test -c 'SELECT 1' >/dev/null 2>&1; do
  sleep 1
done
docker exec -i "$name" psql -U rtop -d rtop_test < "$root/tests/compat/postgres-minimal/init.sql"

if [ "${RTOP_COVERAGE_USE_CARGO_LLVM_COV:-0}" = 1 ]; then
  coverage_run_report=$(mktemp "${TMPDIR:-/tmp}/rtop-coverage-cli-report.XXXXXX")
  actual=$(cd "$root" && RTOP_POSTGRES_PORT="$postgres_port" \
    cargo llvm-cov run --no-clean --json --output-path "$coverage_run_report" \
    --locked --bin rtop -- \
    query tests/compat/postgres-minimal/rtop.toml tests/compat/postgres-minimal/query.rq)
else
  actual=$(RTOP_POSTGRES_PORT="$postgres_port" "$RTOP_COVERAGE_BINARY" \
    query "$root/tests/compat/postgres-minimal/rtop.toml" \
    "$root/tests/compat/postgres-minimal/query.rq")
fi
expected=$(tr -d '\r' < "$root/tests/compat/postgres-minimal/expected.txt")
[ "$actual" = "$expected" ] || {
  echo "coverage PostgreSQL CLI gate: query result differs from fixed fixture" >&2
  exit 1
}

after=$(profile_count)
[ "$after" -gt "$before" ] || {
  echo "coverage PostgreSQL CLI gate: CLI did not produce a new LLVM profile" >&2
  exit 1
}
printf '%s\n' "coverage PostgreSQL CLI gate: passed (profiles $before -> $after)"
