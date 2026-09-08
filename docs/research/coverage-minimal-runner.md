# #110：最小统一 LLVM 插桩 runner

固定 Ontop 基线为 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。本文件记录
覆盖率采集通路，不把它的当前覆盖率数值误作三层功能等价结论。

运行：

```sh
./scripts/test-coverage-minimal.sh
```

该入口先调用 `cargo llvm-cov --no-report --all --locked`，再启动 `postgres:17`，使用
`tests/compat/postgres-minimal/{init.sql,mapping.obda,query.rq,expected.txt}` 的固定映射和
查询，由同一个 `cargo llvm-cov` target/profile 集合执行 `rtop query`。随后以同一 target
目录中的已插桩二进制重放 PostgreSQL CLI（validate、compile、query、materialize、metadata、
bootstrap、mapping 转换）和 HTTP（health、GET/form/raw SPARQL POST）交付切片。这个夹具来自
Ontop CLI 的 `client/cli/src/test/resources/test/simplemapping.obda` 可观察 PostgreSQL
行为；预期结果独立保存在 fixture，而非由 rtop 实现推导。

runner 在 Rust 测试后记录 `.profraw` 个数，再在真实 CLI 请求后重新计数；第二个数未增加
即失败。它还会在 `target/llvm-cov/minimal/` 生成：

- `coverage-summary.json`：每个生产源文件的行、函数、region 分母与覆盖率；
- `coverage-missing-lines.txt`：逐文件未覆盖行；
- `provenance.json`：`cargo-llvm-cov`、`rustc -Vv`、profile pattern、两阶段 profile 数与
  实际命令，以及 Rust 分支覆盖裁决；
- `branch-coverage-probe.json`：本次 profile 的 LLVM 原始 branch summary。当前本体、映射、
  SPARQL 与运行时四层的 Rust branch denominator 均为零。

工具版本由每次运行写入 provenance；当前环境的工具为 `cargo-llvm-cov 0.9.1`、
`rustc 1.98.0 (LLVM 22.1.8)`。Docker、工具、profile、报告或 CLI profile 任一缺失均以
非零状态结束。`scripts/test-final-gate.sh` 已调用此入口，原有 Rust、PostgreSQL、交付和
OCI gate 均保持不变。

`scripts/validate-module-coverage.sh` 消费同一份 JSON，逐个检查所有 `src/*.rs` 的行、函数
行覆盖率不低于 90%、函数覆盖率不低于 85%，并已由最终 gate 调用。任何低于门槛的模块会列出文件和两个百分比后以
非零状态结束；这意味着当前尚未达到 #118 时最终 gate 会正确失败，而不会因报告存在而误绿。

Rust branch coverage 仍由 #119 单独裁决，不能由行或函数覆盖替代。runner 会把零分母记录为
`external-tool-blocked`；如果工具将来输出非零分母，runner 会失败关闭，要求建立 ADR-0005 的
80% branch gate，而不会把旧的阻断结论误用为通过。

HTTP 切片的二进制由 `cfg(coverage)` 下的 `RTOP_COVERAGE_SERVER_MAX_REQUESTS=4` 在 health
和三种 SPARQL 请求完成后正常退出，以保证 LLVM profiler 刷写数据。发布构建不读取该变量，
也不新增 HTTP 路由或改变 endpoint 生命周期。
