# PostgreSQL adapter 生命周期证据边界

Issue #45 的两个原子直接验收 Rust `PostgresDataSource` 的连接级行为，而不是
SPARQL HTTP 协议：`execute_typed_stream` 首行 `Stop` 后连接复用，以及 PostgreSQL
cancel token 中断 `pg_sleep(5)` 后返回 `query-cancelled` 并复用同一连接。

可复现的 rtop 集成测试位于 `tests/postgres_adapter.rs`：

```sh
RTOP_POSTGRES_HOST=127.0.0.1 RTOP_POSTGRES_PORT=... \
RTOP_POSTGRES_USER=rtop RTOP_POSTGRES_PASSWORD=rtop \
RTOP_POSTGRES_DATABASE=rtop_test cargo test --locked --test postgres_adapter
```

Ontop 基线提供 JDBC/RDF4J 的内部结果集，而没有等价的 Rust adapter API；因此不得将
HTTP endpoint 的并发或断开 artifact 伪装为这两个 adapter 原子的双边差分。HTTP
生命周期对照单独由 Issue #83 的
`postgres-http-concurrency-isolation-endpoint` artifact 覆盖。

在建立可比较的 Ontop JDBC harness 前，这两个矩阵原子保持 `pending`；严格账本中的
rtop PostgreSQL gate 仍是其实现回归证据，但不是闭合矩阵所需的 Ontop-vs-rtop provenance。
