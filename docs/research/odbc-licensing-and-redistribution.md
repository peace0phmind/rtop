# ODBC 供应链：许可、再分发与服务端费用核对

**范围与结论（2026-08-31）。** 本文核对的是 `rtop` 若使用 Rust
[`odbc-api`](https://github.com/pacman82/odbc-api) 时，实际会被带入生产环境的
ODBC 供应链。它不是法律意见，也不以“页面允许下载”推定“允许随 Docker 镜像
再分发”。对专有驱动，只有明确适用于**所下载版本、目标操作系统和部署方式**的
EULA/合同才能给出再分发权；没有公开条款时，结论一律是“向供应商取得书面许可”。

这一路线能做到“无 JVM”，但**不属于 Rust 原生协议栈**：驱动管理器和每个 ODBC
驱动都是本地共享库，后者往往由供应商以闭源二进制交付。

## 判定口径

- **生产使用可公开确认**：仅表示能从公开许可证/文档确认安装运行的依据；不表示
  服务本身免费，亦不表示该驱动能满足 Ontop 所需的元数据、类型和取消语义。
- **镜像再分发可公开确认**：包括将安装包或 `.so` 放入由 `rtop` 制作并发布的 Docker
  镜像。运行者自行在镜像启动后下载，仍可能触发许可、网络、版本可复现性和凭据问题，
  不等同于解决再分发权。
- **服务端费用/许可**：只说数据源服务本身；它和客户端驱动的许可是两件事。

## 基础层

| 项目 | 获取与许可依据 | 生产使用 | Docker 再分发 | 服务端费用/行动结论 |
| --- | --- | --- | --- | --- |
| Rust [`odbc-api`](https://crates.io/crates/odbc-api) | Cargo；crate 元数据声明 **MIT**。 | 可按 MIT 使用。 | MIT 允许复制、发布、再许可和出售，但应保留版权与许可文本。 | 无服务端。可作为可再分发的 Rust 包。 |
| Linux [`unixODBC`](https://github.com/lurcher/unixODBC) | 发行版包或源码；上游 [`COPYING`](https://github.com/lurcher/unixODBC/blob/master/COPYING) 为 **LGPL-2.1**。 | 可按 LGPL-2.1 动态使用。 | 不是“无需条件”：镜像须保留许可证/版权声明；若修改或静态链接，需满足 LGPL 的相应源代码、替换/重链接等义务。实际镜像应做 OSS 合规审查。 | 无服务端。**本表仅覆盖 Linux**；Windows 的 Driver Manager（Windows Data Access Components）和 macOS 的 iODBC/unixODBC 分发权须分别核查。 |

## 数据源驱动矩阵

| 数据源 | 获取方式与一手依据 | 生产使用能否由公开资料确认 | Docker 再分发能否由公开资料确认 | 服务端费用/许可与行动结论 |
| --- | --- | --- | --- | --- |
| **DuckDB** | 官方 [`duckdb-odbc` Releases](https://github.com/duckdb/duckdb-odbc/releases) 或源码；仓库 [`LICENSE`](https://github.com/duckdb/duckdb-odbc/blob/main/LICENSE) 为 **MIT**。 | 是，MIT。 | 是，MIT 明确允许 distribute/sublicense/sell；保留许可证。 | DuckDB 核心也是 [MIT](https://github.com/duckdb/duckdb/blob/main/LICENSE)。无必须购买的服务端；商业支持另行采购。可放入开源镜像，但仍是本地 C/C++ 驱动。 |
| **TDEngine** | 官方 [ODBC 文档](https://docs.tdengine.com/tdengine-reference/client-libraries/odbc/) 与 [`taos-connector-odbc`](https://github.com/taosdata/taos-connector-odbc) 源码/Release。应以选定 tag 的 `LICENSE` 为准（该项目公开标注 MIT）。 | 是，就开源驱动而言可按该 tag 的 MIT 运行。 | 原则上可按 MIT 再分发，并保留许可证；**同时**核对镜像是否带入 TDengine 其他组件及其版本许可。 | [TDengine OSS](https://github.com/taosdata/TDengine) 有开源版；企业功能、云服务与支持可能收费。实施时锁定 connector 与 server 的版本。 |
| **Amazon Athena Simba ODBC** | AWS [安装说明](https://docs.aws.amazon.com/athena/latest/ug/connect-with-odbc.html) 的官方下载路径（Windows/macOS/Linux）。 | **有限确认**：AWS 文档说明客户如何安装使用；实际授权取决于下载时显示/随包附带的 AWS/Simba 条款。 | **否，公开文档不足以确认。** 不要把下载的 Simba 包 bake 进公开或私有镜像，除非适用 EULA 明示允许或 AWS/Simba 书面授权。 | Athena 服务按官方 [Pricing](https://aws.amazon.com/athena/pricing/) 计费（查询、存储/附加服务等）；驱动是否单列收费不能由安装文档推出。先取得目标驱动版本 EULA 和部署许可。 |
| **Google BigQuery Simba ODBC** | Google [ODBC/JDBC drivers 文档](https://cloud.google.com/bigquery/docs/reference/odbc-jdbc-drivers) 指向 Simba 下载/安装资料。 | **有限确认**：Google 文档支持客户取得并配置驱动，但闭源驱动的运行权以下载页/安装包 EULA 为准。 | **否，公开文档不足以确认。** 需向 Google/Simba 确认容器再分发、离线镜像和 CI 缓存权。 | BigQuery 依 [官方价格](https://cloud.google.com/bigquery/pricing) 收取计算/存储等费用；这不代表驱动许可免费。 |
| **Dremio ODBC** | Dremio [ODBC 文档](https://docs.dremio.com/current/client-applications/odbc/) 指向 Dremio 下载中心。 | **有限确认**：可由拥有相应 Dremio 发行版访问权的客户按随包条款使用；公开文档未给出可独立引用的开放源代码许可。 | **否。** 在未取得所用版本 EULA/订单或 Dremio 书面许可前，不发布含该二进制的镜像。 | [Dremio editions](https://docs.dremio.com/current/get-started/editions/) 包含社区/商业与云产品；准确费用、ODBC 可用性及支持权由部署版本/合同决定。优先评估 Flight SQL 的 Rust 实现以避开该二进制。 |
| **Denodo ODBC** | Denodo [ODBC 配置文档](https://docs.denodo.com/8.0/en/tools/denodo-platform/administration-guide/administration/administration-tasks/configuring-the-odbc-driver)；通常经客户支持/下载门户取得。 | **有限确认**：文档证明产品组件存在，运行权取决于 Denodo 平台订阅/许可和随包 EULA。 | **否。** 门户取得不构成把组件再分发给镜像用户的权利；需 Denodo 书面确认。 | Denodo 是商业平台，许可、云/支持费用以报价/合同为准。没有合同不应将此适配器列为可交付覆盖。 |
| **Oracle Database ODBC / Instant Client** | Oracle [ODBC Programmer's Guide](https://docs.oracle.com/en/database/oracle/oracle-database/23/odbcc/index.html) 与 [Instant Client 下载页](https://www.oracle.com/database/technologies/instant-client/downloads.html)。下载页要求接受版本相应许可；常见公开条款是 [Free Use Terms and Conditions](https://www.oracle.com/downloads/licenses/oracle-free-license.html)。 | **有限确认**：FUTC/下载条款能在其范围内授权下载和使用；必须把实际包的许可、版本、CPU/OS 与用途逐一留档。 | **不能从此处一概确认。** FUTC 的许可范围、受限程序、云/托管及再分发情形须由 Oracle 条款或书面许可覆盖；不要默认可将 Instant Client 放入发布镜像。 | Oracle Database/Cloud 的许可和使用费依 [Oracle 定价/合同](https://www.oracle.com/database/pricing/)；客户端免费可下载不等于服务端免费。需要 Oracle 许可负责人确认。 |
| **IBM Db2 CLI/ODBC** | IBM [Data Server Driver Packages](https://www.ibm.com/docs/en/db2/11.5.x?topic=drivers-data-server-driver-packages) 和 [CLI/ODBC 下载安装资料](https://www.ibm.com/support/pages/db2-odbc-cli-driver-download-and-installation-information)。 | **有限确认**：IBM 提供客户端下载，但安装/下载会受 IBM License Agreement 或该包随附许可约束。 | **否，公开产品文档不足以确认。** 应从 IBM 获取指定 Data Server Driver 版本的许可文本，并书面确认镜像再分发、托管服务和地域限制。 | Db2 有不同 server/cloud/Connect 产品；参见 [Db2 pricing](https://www.ibm.com/products/db2/pricing)。客户端包的可得性不消灭服务器或 Connect 许可。 |
| **Presto / Trino / Apache Spark（CData ODBC）** | 不是这些项目的官方 ODBC：从 [CData Presto](https://www.cdata.com/drivers/presto/odbc/)、[Trino](https://www.cdata.com/drivers/trino/odbc/)、[Apache Spark](https://www.cdata.com/drivers/apachespark/odbc/) 获取试用或商业产品。各上游官方仍主要给出 JDBC/HTTP 路线，例如 [Trino JDBC](https://trino.io/docs/current/client/jdbc.html)。 | **有限确认**：CData 下载页表明可试用；生产使用需购买/取得 CData 的对应产品许可，具体数量与期限按订单/EULA。 | **否，默认不允许推定。** 第三方商业驱动的镜像嵌入、OEM、SaaS/托管再分发必须由 CData OEM/嵌入式条款或书面许可明确授权。 | Presto/Trino/Spark 本体的 Apache-2.0 许可不覆盖 CData 驱动；托管服务另有云厂商费用。若必须 Rust 栈，Trino/Presto 应优先走 HTTP 协议而非引入 CData。 |
| **Teiid / H2** | 官方 Teiid [Client Guide](https://teiid.github.io/teiid-documents/16.0.x/content/client-developer-guide/index.html) 和 H2 [Features](https://h2database.github.io/html/features.html) 都未给出官方 ODBC 驱动下载。 | 不适用：未确认官方驱动。 | 不适用。 | 不将它们列作 ODBC 覆盖承诺。H2 还属于 JVM 嵌入式工具链，与 `rtop` 的 Rust 协议栈约束冲突。 |

## 落地门槛

对每一种非 MIT/Apache 等明确开源驱动，实施 issue 必须附上：

1. 目标版本、平台、校验和与**随包 EULA 的归档链接**；
2. 运行模式（用户本机安装、私有镜像、公开镜像、SaaS）与供应商书面许可；
3. 服务器产品/云账号的有效授权、计费责任和支持边界；
4. SBOM、许可证 notices，以及 Linux 动态链接/静态链接对 `unixODBC` LGPL-2.1 的合规审查；
5. 用真实服务执行的连接、认证、参数、类型、metadata、取消和流式结果验收。

在上述材料缺失时，`rtop` 的产品说明只能写“可由用户自行配置经 ODBC 使用”，
不能宣称内置支持、可再分发，也不能把该驱动包含进官方 Docker 镜像。

## 证据限制

厂商下载页常把完整 EULA 放在登录、点击接受或安装包中，无法由公开文档可靠推导再分发权。
本文的“否”通常表示“**公开资料无法确认**”，不是断言供应商绝不提供该权利。采购、OEM、
嵌入式或云托管合同可能改变结论，必须由法务/采购依据目标合同确认。
