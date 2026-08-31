# Ontop 数据源的 ODBC 驱动可用性

**问题。** 对 Ontop 当前以 JDBC 覆盖、而 `rtop` 可能经 `odbc-api` 覆盖的数据库/查询引擎，是否有可获取的 ODBC 二进制驱动？它们是否为厂商官方发行、是否开源？

**结论（2026-08-31）。** 不能把“有 ODBC 驱动”写成“完全替代 JDBC”。对于此清单，大多数产品确实能通过某种 ODBC 驱动接入，但来源、支持边界和许可截然不同：DuckDB 与 TDEngine 有官方开源驱动；Oracle、DB2、Athena、BigQuery、Dremio 与 Denodo 有官方/产品方二进制驱动但不是开源；Presto、Trino、Spark 主要是第三方商业驱动；Teiid 和 H2 未能从其官方资料证实存在官方 ODBC 驱动。

这里的“官方”只表示由产品厂商或该产品官方项目发布/文档化；它**不**表示该驱动采用 Rust、无 C ABI、可商用再分发，或足以通过 Ontop 的元数据与类型测试。

## 矩阵

| 数据源 | ODBC 驱动及来源 | 如何获取 | 开源 / 许可证状态 | `rtop` 的事实判断 |
| --- | --- | --- | --- | --- |
| DuckDB | **官方开源** [`duckdb-odbc`](https://github.com/duckdb/duckdb-odbc)；项目明确称为 DuckDB ODBC Driver。 | 官方 [ODBC 文档](https://duckdb.org/docs/stable/clients/odbc/overview) 指向安装与下载；可从该官方仓库的 [Releases](https://github.com/duckdb/duckdb-odbc/releases) 取预编译包，或自行构建。 | 仓库标注 [MIT License](https://github.com/duckdb/duckdb-odbc/blob/main/LICENSE)。 | 可作为 ODBC 路线的开源驱动；仍是 C/ODBC 驱动，而非 Rust 网络协议栈。 |
| Amazon Athena | **AWS 官方** Amazon Athena ODBC Driver（文档标为 Simba Athena ODBC Driver）。 | AWS [安装文档](https://docs.aws.amazon.com/athena/latest/ug/connect-with-odbc.html) 提供 Windows/macOS/Linux 安装与下载入口。 | AWS 文档没有将该二进制宣布为开源；应按下载页/EULA 的商用条款处理，**不可标为开源**。 | 有厂商二进制，但带 AWS/Simba 许可和部署依赖。 |
| Google BigQuery | **Google 官方分发** Simba ODBC driver。 | Google [ODBC 文档](https://cloud.google.com/bigquery/docs/reference/odbc-jdbc-drivers) 提供下载及配置说明。 | Google 文档将其作为 Simba 驱动下载，未声明开放源代码；**不可标为开源**。 | 有产品方支持的 ODBC 选项，不是 pure-Rust。 |
| Dremio | **Dremio 官方** ODBC Driver。 | Dremio [ODBC 配置文档](https://docs.dremio.com/current/client-applications/odbc/) 指向 Dremio 下载中心取得相应平台驱动。 | 官方文档未声明该二进制为开源；**不可标为开源**，实际许可须随目标版本下载核验。 | 可作为允许二进制驱动时的选项；同时可另评估 Flight SQL。 |
| Denodo | **Denodo 官方** Denodo ODBC Driver，作为 Denodo Platform 的客户端工具。 | 官方 [ODBC 配置文档](https://docs.denodo.com/8.0/en/tools/denodo-platform/administration-guide/administration/administration-tasks/configuring-the-odbc-driver) 说明安装；通常通过 Denodo 支持/下载门户取得产品安装包。 | Denodo 官方资料未将其作为开源发布；商业产品组件，**不可标为开源**。 | 具有厂商路线，但须具备相应 Denodo 发行/支持权限。 |
| Presto | Apache Presto 官方资料可确认 JDBC 客户端，但本次未找到由 PrestoDB 官方发行的 ODBC 驱动。市面上有 **第三方商业** [CData Presto ODBC Driver](https://www.cdata.com/drivers/presto/odbc/)。 | 从第三方 CData 下载页获取；不是 Apache Presto 的发布渠道。 | CData 产品按其商业许可；不是开源。Presto 本身的 Apache-2.0 许可不能延伸到该驱动。 | **无已证实的官方 ODBC 二进制。** 采用第三方驱动是新增供应商依赖。 |
| Trino | 本次未在 Trino 官方客户端文档中证实官方 ODBC 驱动；官方文档提供的是 [JDBC Driver](https://trino.io/docs/current/client/jdbc.html)。可获取的 **第三方商业** 示例为 [CData Trino ODBC Driver](https://www.cdata.com/drivers/trino/odbc/)。 | CData 下载页；不是 Trino 项目发行。 | CData 商业许可，非开源。 | **无已证实的 Trino 官方 ODBC 二进制。** Trino 自身有公开 HTTP client protocol，Rust 更适合直接实现该协议。 |
| Apache Spark | Apache Spark 官方 [JDBC 文档](https://spark.apache.org/docs/latest/sql-data-sources-jdbc.html) 说明 JDBC；本次未证实 Apache 项目发行 ODBC 驱动。可见 **第三方商业** [CData Apache Spark ODBC Driver](https://www.cdata.com/drivers/apachespark/odbc/)。 | 从 CData 下载；某些商业 Spark 发行版也可能另附 Simba 驱动，应按具体发行商核实。 | CData 商业许可、非开源；不能因 Apache Spark 本体是 Apache-2.0 而推定驱动开源。 | **未证实 Apache Spark 官方 ODBC 二进制。** 不应作为基础兼容承诺。 |
| TDEngine | **官方开源** [`taos-connector-odbc`](https://github.com/taosdata/taos-connector-odbc)，描述即为 “odbc connector for tdengine”。 | 官方 [ODBC Connector 文档](https://docs.tdengine.com/tdengine-reference/client-libraries/odbc/) 提供安装说明；仓库 [Releases](https://github.com/taosdata/taos-connector-odbc/releases) 提供发行物。 | 仓库的 [LICENSE](https://github.com/taosdata/taos-connector-odbc/blob/main/LICENSE) 为 MIT。 | 可作为开源 ODBC 选项；仍非 Rust 原生协议实现。 |
| Teiid | 本次查阅的官方 [Client Developer Guide](https://teiid.github.io/teiid-documents/16.0.x/content/client-developer-guide/index.html) 覆盖 JDBC、ODATA 等客户端路径，**未证实**官方 ODBC 驱动或官方二进制下载。 | 无可确认的官方 ODBC 获取渠道。 | 无可确认的 ODBC 驱动许可证。 | 不能将其列作 `odbc-api` 已覆盖的数据源；若业务要求，需单独调查目标 Teiid 发行版或改用服务协议。 |
| Oracle Database | **Oracle 官方** Oracle ODBC Driver，随 Oracle Client/Instant Client 发布。 | Oracle [ODBC 文档](https://docs.oracle.com/en/database/oracle/oracle-database/23/odbcc/index.html) 说明安装；从 [Oracle Instant Client 下载页](https://www.oracle.com/database/technologies/instant-client/downloads.html) 获取相应平台包。 | Oracle Client/ODBC 二进制不是开源；使用受 Oracle [Free Use Terms and Conditions](https://www.oracle.com/downloads/licenses/oracle-free-license.html) 或随产品适用条款约束，须按版本核验。 | 有官方二进制，且通常需安装/捆绑 Oracle Client；不符合严格 pure-Rust。 |
| IBM Db2 | **IBM 官方** IBM Data Server Driver for ODBC and CLI。 | IBM [Data Server Driver Packages 文档](https://www.ibm.com/docs/en/db2/11.5.x?topic=drivers-data-server-driver-packages) 与 [下载/安装说明](https://www.ibm.com/support/pages/db2-odbc-cli-driver-download-and-installation-information) 提供获取途径。 | IBM 提供二进制驱动包，但资料未声明开源；适用 IBM 下载/产品许可，**不可标为开源**。 | 有官方 ODBC 路线，但需要 IBM 客户端二进制与版本兼容测试。 |
| H2（补充） | H2 官方 [Features](https://h2database.github.io/html/features.html) 列出 JDBC API、Server 等，未列 ODBC；本次未证实官方 ODBC 驱动。 | 无可确认的官方 ODBC 获取渠道。 | 无可确认的官方 ODBC 驱动许可证。 | H2 不应作为 `rtop` 的 ODBC 验收后端；它仍是 JVM 嵌入式数据库。 |

## 采用 ODBC 前的约束

1. **驱动可得性 ≠ 产品覆盖。** 特别是 Presto、Trino、Spark、Teiid 与 H2 不能因为 Ontop 的 JDBC 连接器存在，就推导为有可用的官方 ODBC 方案。
2. **二进制供应链需入验收。** 对每个采用 ODBC 的实现 issue，固定 driver manager、驱动厂商/版本、目标 OS/CPU、连接串格式和许可；容器中可否合法分发驱动也需单独确认。
3. **能力逐源验证。** 使用 Docker 中的目标服务，至少验证连接/认证、metadata、参数绑定、NULL、数值与时间类型、取消、流式结果，以及 Ontop 所需 SQL 方言。ODBC API 的共同表面不足以保证这些语义相同。
4. **架构边界不变。** 即使选择 `odbc-api`，也只让它实现 `rtop` 自有的 `DataSource` 适配器；不得让 ODBC 句柄、错误或元数据类型进入 SPARQL 改写和 SQL 方言核心。

## 证据范围

本表只回答“是否存在/可获得 ODBC 驱动”与公开许可证状态，不评价驱动质量、性能、商业支持等级，亦不构成任何第三方许可的法律解释。对标为“未证实”的项目，结论是当前官方来源不足，**不是**对整个生态不存在该类驱动的断言。
