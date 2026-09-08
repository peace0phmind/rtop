# Rust 分支覆盖工具调查（2026-09-07）

关联：#119、ADR-0005、ADR-0008。范围是 rtop 的 PostgreSQL 替换内核；此记录不以
行或函数覆盖替代分支覆盖。

## 固定环境

- Rust：`cargo 1.98.0 (797e8a9bc 2026-08-05)`。
- LLVM runner：`cargo-llvm-cov 0.9.1`。
- 主机：Ubuntu 24.04.4、Linux 6.8.0-139-generic、x86_64。
- 基线：只读 `../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。

## LLVM probe：无有效分母

命令：

```sh
cargo llvm-cov report --json --output-path /tmp/rtop-branch-probe.json
jq -r '.data[0].files[]
  | select(.filename | endswith("/src/sparql.rs"))
  | .summary.branches' /tmp/rtop-branch-probe.json
```

输出：

```json
{"count":0,"covered":0,"notcovered":0,"percent":0.0}
```

这表示当前 LLVM/Rust 构建未导出可统计的 Rust branch 分母；`0.0` 不能解释为
0% 覆盖率，也不能解释为通过。相同结论适用于本体、映射与运行时层。

## tarpaulin probe：环境依赖与替代尝试

`cargo search cargo-tarpaulin --limit 5` 确认候选版本是 `cargo-tarpaulin 0.37.2`。
首次固定版本安装命令：

```sh
cargo install cargo-tarpaulin --version 0.37.2 --locked
```

失败于 `openssl-sys 0.9.116`：`pkg-config --libs --cflags openssl` 找不到 `openssl.pc`，
且系统未安装 `libssl-dev`。这是工具构建依赖失败，不是 rtop 测试失败。

该版本的 crate metadata 公开 `vendored-openssl` 特性（将 `git2` 改用 vendored
OpenSSL）。为避免改变主机系统包，后续以如下命令重试：

```sh
cargo install cargo-tarpaulin --version 0.37.2 --locked \
  --no-default-features --features vendored-openssl
```

重试曾出现 `SSL_ERROR_SYSCALL`、`unexpected eof while reading` 与 `Connection reset by
peer`，但随后完成。安装后的版本为：

```text
cargo-tarpaulin-tarpaulin 0.37.2
```

以受限的库测试探针验证其实际输出：

```sh
cargo tarpaulin --branch --lib --locked --out Json \
  --output-dir /tmp/rtop-tarpaulin-branch.W9dttq
```

退出码为 0，52 个库测试通过，输出行为是 line coverage（`2758/5922`，46.57%）。
`tarpaulin-report.json` 的顶层字段仅有：

```json
["coverable", "coverage", "covered", "files"]
```

其中没有 branch/condition 分母或已覆盖数。该工具自己的 `--help` 将 `--branch` 标为
`Branch coverage: NOT IMPLEMENTED`。因此即使安装成功，也不能报告可验证的 Rust branch
覆盖率。

## 当前裁决

分支覆盖率为**未测量，且当前可用 reporter 不提供可靠分母**。逐文件 LLVM 行/函数 90%
gate 继续独立执行；ADR-0005 的三层 80% 分支门槛不能关闭，#119 与依赖它的等价结论必须
保持开放。最终 provenance 必须继续明确这一外部工具阻断，不能将 tarpaulin 的 line-only
JSON 或 LLVM 的零分母误称为 branch 结果。
