# Movie annotation 精确计数实施计划

基线：`../ontop@5ec07573b18513f33dfcd59ac45fe26a81f9cdbd` 的
`AnnotationMovieTest`。

`newSyntaxMovieontology.obda` 的 source 选择条件决定可合成、可复现的 PostgreSQL 数据集：

| 基线查询 | mapping source | 目标行数 |
| --- | --- | ---: |
| `dc:description` / `dc:date` / `mo:idTitle` | `title WHERE kind_id = 1`（idTitle 无 WHERE） | 444090 / 443300 / 444090 |
| `dbpedia:gross` | `movie_info WHERE info_type_id = 107` | 112576 |
| `mo:belongsToGenre` | `movie_info WHERE info_type_id = 3` | 546032 |
| `mo:hasMaleActor`, `mo:Vip` | `cast_info WHERE role_id = 1`，query LIMIT 100000 | 100000 |
| `mo:companyId` | `movie_companies WHERE company_type_id = 2` | 131645 |

实施位于 `scripts/test-postgres-compat.sh` 的最后阶段：先完成其它共享 `title`/`movie_info` 场景，再 `TRUNCATE` 并以 `generate_series` 装载精确数据。每条**原始 SPARQL query**由 `cargo run ... query` 执行，其 stdout 直接流式交给 `wc -l` 比对；未将验证改写成 PostgreSQL `COUNT(*)`。

`VkgRuntime::query` 在排序、投影后收集为 `Vec<Binding>`，`main.rs` 才逐行打印。因此 546032 行 gate 会先物化全集。实测 description 的 444090 行查询用时约 2.85 秒、峰值 RSS 约 1.12 GiB；当前验证环境可以承受，未引入改变 SPARQL 语义的旁路计数接口。

共享 fixture 顺序已核对：annotation 查询位于 expression 查询之前，而后者仍会使用 `title`/`movie_info`。精确 seed 必须追加到 compatibility script 的全部现有查询之后，才可安全 `TRUNCATE` 这些表。

结果：八条 query 已在 postgres:17 Docker 与 Rust CLI 上通过，行数依序为
444090、443300、112576、546032、100000、100000、131645、444090。为复现
description/date 的差异，444090 条 `title.kind_id=1` 中后 790 条的
`production_year` 为 NULL；它们仍产生 description，但会被 NULL-row 抑制规则排除
在 date 之外。
