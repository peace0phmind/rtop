#!/usr/bin/env sh
set -eu

# 以固定 PostgreSQL 17 运行真实 adapter 集成测试。普通 cargo test 在没有连接环境时
# 会安全跳过这些测试；统一 coverage profile 必须显式提供固定数据库，才能覆盖 catalog、
# cancellation 和 streaming 的公开端口行为。
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
: "${RTOP_COVERAGE_PROFILE_DIR:?需要 LLVM profile 目录}"

database=rtop-coverage-adapter-$$
cleanup() {
  docker rm -f "$database" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

# PostGIS 17 是 PostgreSQL 17 的扩展镜像；adapter 的 GeoSPARQL 公开端口需要在同一
# LLVM profile 中真实执行，而非仅停留在 parser 单测。镜像 entrypoint 会重启一次，因而
# 等待两次成功探针以跨越初始化窗口。
docker run -d --name "$database" \
  -e POSTGRES_USER=rtop -e POSTGRES_PASSWORD=rtop -e POSTGRES_DB=rtop_test \
  -p 127.0.0.1::5432 docker.m.daocloud.io/postgis/postgis:17-3.5 >/dev/null
postgres_port=$(docker port "$database" 5432/tcp | sed 's/.*://')
stable=0
while [ "$stable" -lt 2 ]; do
  if docker exec "$database" psql -U rtop -d rtop_test -c 'SELECT PostGIS_Full_Version()' >/dev/null 2>&1; then
    stable=$((stable + 1))
  else
    stable=0
  fi
  sleep 1
done
docker exec -i "$database" psql -U rtop -d rtop_test \
  < "$root/tests/compat/postgres-constraints/init.sql"

cd "$root"
RTOP_POSTGRES_HOST=127.0.0.1 \
RTOP_POSTGRES_PORT="$postgres_port" \
RTOP_POSTGRES_DATABASE=rtop_test \
RTOP_POSTGRES_USER=rtop \
RTOP_POSTGRES_PASSWORD=rtop \
  cargo llvm-cov test --no-clean --locked --test postgres_adapter
