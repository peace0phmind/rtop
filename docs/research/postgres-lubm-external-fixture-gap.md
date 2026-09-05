# PostgreSQL LUBM manifest 外部 fixture 缺口

固定 Ontop 基线 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 的
`testcases-docker/virtual-mode/lubm/manifest-pgsql.ttl` 包含 14 个原始
`qt:query` 实例。其 PostgreSQL properties 将数据源硬编码为：

```properties
jdbc.url = jdbc:postgresql://obdalin.inf.unibz.it/lubm1
jdbc.user = postgres
```

基线树不含该数据库的 SQL dump、schema 或可再分发 fixture。2026-09-05 的只读
连通性探测使用已认证 `postgres:17` 容器执行 `psql -w`（无密码、仅 `SELECT 1`）；
DNS 得到 `198.18.0.12:5432`，服务器在认证前关闭连接。因此这 14 个 query 当时不能
直接依赖外部服务重放。

这不是范围排除：它们仍是 PostgreSQL manifest 分母。后续工作必须从固定 mapping、
14 条原始 query 与每项基线 `rsi:size` 生成可再现的最小 PostgreSQL schema/data，逐项
执行 Rust CLI，并将每个 query 独立写入账本和 compatibility report。不得用网络失败或
仅 mapping 能加载来替代结果验证。

## 本地 fixture 与完成证据

`tests/compat/postgres-lubm/fixture.sql` 以固定 mapping 的八张 relation 建立可再现
最小数据集；其 SHA-256 为
`25481995e35b7ce0c965feece580b88414cc64c81e5e3288ae087707238dbd3a`。
实现补齐了 manifest 注释前缀解析、匿名 `owl:intersectionOf` 的命名 conjunct
subclass 归一化，以及映射 property 的 domain/range 类型改写。

`scripts/test-postgres-lubm-compat.sh` 在独立 PostgreSQL 17 Docker 容器中从固定
manifest 动态发现 14 个条目，执行原始 `.rq`，并逐项比较原始
`query-result-N.ttl` 的 `rsi:size`。结果依序为：
`4, 0, 6, 34, 720, 7790, 67, 7790, 208, 4, 224, 15, 1, 5916`，全部通过。
每个 query 已独立记入覆盖账本并接入最终 gate；外部服务仍不作为验收依赖。

## 相邻 manifest 分类边界

同一 PostgreSQL 发现集中的 `general/manifest-pgsql.ttl` 不属于未迁移缺口：
`PgsqlDatatypeTest` 将其唯一的 `general-Type: all` 参数化实例列入 `IGNORE`。
发现器和账本验证器因此将它记为 `baseline-ignored`，要求 compatibility report 保留
该分类记录，但禁止把它计为通过的 Rust 查询。
