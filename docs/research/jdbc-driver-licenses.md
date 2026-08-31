# Ontop 所列数据源的 JDBC 驱动许可与取得核查

**问题。** Amazon Athena、Google BigQuery、Dremio、Denodo、Oracle Database 与 IBM Db2 在当前 Ontop 中可经 JDBC 接入；其 JDBC 驱动是否开源、从何处取得，以及驱动本身、服务/数据库和商业支持是否需要付费？

**结论（2026-08-31）。** 不可把它们笼统归为“均可用开源 JDBC 驱动”。此组中只有 **Dremio OSS 的 JDBC client** 有可核对的公开源码和 Apache-2.0 许可。Oracle 的 `ojdbc` 可公开下载/Maven 获取，但使用的是 Oracle FUTC/FDHUT 条款，**不是开源许可证**。Athena、BigQuery、Denodo、Db2 的官方文档能够确认 JDBC 驱动/下载路线，却没有把相应驱动作为开源项目发布；在没有驱动包内许可证或厂商书面条款的证据时，均按**非开源或未证实为开源**处理。

这里的“开源”指代码可取得且适用 OSI 式开源许可证；“可免费下载”不等于开源。“费用未证实”只表示本次未找到官方、适用于该驱动版本的价格或免费声明，**不应推导为免费**。

| 数据源 | JDBC 驱动及官方取得方式 | 源码仓库与许可证证据 | 是否开源 | 驱动费用 | 数据库/云服务费用 | 商业支持费用 |
|---|---|---|---|---|---|---|
| Amazon Athena | AWS [JDBC 文档](https://docs.aws.amazon.com/athena/latest/ug/connect-with-jdbc.html) 明确提供 2.x 和 3.x 两代驱动及各自下载入口。 | AWS 页面是二进制下载/配置文档，未链接驱动源码仓库，也未将该 JDBC 驱动声明为开放源代码。 | **否（没有官方开源发布证据）**。不能将 AWS SDK 的开源许可套用到 JDBC 驱动。 | **未证实**：官方连接文档没有给出驱动的独立价格或“免费”承诺；须核对目标下载物随附条款。 | **需要付费/按量**：AWS 对 Athena 的说明是“只为运行的查询付费”（[What is Athena](https://docs.aws.amazon.com/athena/latest/ug/what-is.html)）。这与驱动费不同。 | **未证实**：取决于 AWS Support 方案/合同，本表不把它计入驱动费用。 |
| Google BigQuery | Google 的 [ODBC/JDBC drivers 文档](https://cloud.google.com/bigquery/docs/reference/odbc-jdbc-drivers) 是官方 JDBC 驱动说明与下载入口。 | 官方页面将其作为 Simba JDBC driver 分发，未给出相应驱动的 Google 源码仓库或开放源代码许可证。 | **否（没有官方开源发布证据）**。 | **未证实**：官方文档未在该页给出驱动单独价格；须按下载/购买条款核实。 | **可能收费，按 BigQuery 计费模型**；费用由查询、存储等服务使用决定，不能从驱动是否取得免费推断。见 [BigQuery pricing](https://cloud.google.com/bigquery/pricing)。 | **未证实**：Google Cloud Support 与驱动许可应分别核对。 |
| Dremio | Dremio 的开源仓库含 [`client/jdbc`](https://github.com/dremio/dremio-oss/tree/master/client/jdbc) 模块；可从源码构建，发行版 JDBC jar 的具体版本应依 Dremio 官方下载/文档取得。 | [`dremio/dremio-oss`](https://github.com/dremio/dremio-oss) 根目录 [`LICENSE`](https://github.com/dremio/dremio-oss/blob/master/LICENSE) 为 **Apache License 2.0**，且 JDBC 模块在该仓库中。 | **是（Dremio OSS JDBC client，Apache-2.0）**。注意：不能据此断言 Dremio Cloud 或 Enterprise 的全部客户端包/附加组件同样开源。 | **无许可证费的开源代码路径**：Apache-2.0 允许取得、使用和再分发；某一预构建发行包是否另附条款须按包核对。 | **产品部署/云服务可能收费**：OSS 自托管与 Dremio Cloud/Enterprise 是不同产品，本次未将其定价写成驱动费用。 | **未证实**：商业支持/Enterprise 合同应另行询价。 |
| Denodo | Denodo 的 [JDBC driver 配置文档](https://docs.denodo.com/8.0/en/tools/denodo-platform/administration-guide/administration/administration-tasks/configuring-the-jdbc-driver) 说明其 JDBC driver 的使用；通常随 Denodo Platform/客户下载渠道提供。 | 未找到 Denodo 官方公开源码仓库或适用于 JDBC jar 的开源许可证。 | **否（商业产品组件；无官方开源发布证据）**。 | **未证实**：官方公开资料未证实驱动是否独立收费、是否包含在平台许可内。 | **未证实**：Denodo 产品/云的价格与驱动费须分别向 Denodo 核对。 | **未证实**：通常涉及商业合同，但本次没有找到可引用的公开价格。 |
| Oracle Database | Oracle [JDBC/UCP Downloads](https://www.oracle.com/database/technologies/appdev/jdbc-downloads.html) 提供 `ojdbc8/11/17` 下载，并明确可从 [Maven Central](https://central.sonatype.com/artifact/com.oracle.database.jdbc/ojdbc11) 获取。 | `ojdbc11` 的 Maven POM 标注 **Oracle Free Use Terms and Conditions (FUTC)**（见 [POM](https://repo1.maven.org/maven2/com/oracle/database/jdbc/ojdbc11/23.8.0.25.04/ojdbc11-23.8.0.25.04.pom)）；下载页则链接 [FDHUT license](https://download.oracle.com/otn-pub/otn_software/jdbc/FDHUT_LICENSE.txt)。两者都不是 OSI 开源许可证。未找到 Oracle 对主 JDBC 驱动的官方开源源码仓库。 | **否**。`oracle/ojdbc-extensions` 等辅助仓库不能代表 `ojdbc` 主驱动的许可。 | **可公开下载，但不等于开源或无条件免费**；官方条款允许范围以目标版本的 FUTC/FDHUT 为准。本次未证实额外的独立驱动售价。 | Oracle Database、Autonomous Database 等服务/产品许可可能收费；与 jar 取得分开。见 [Oracle Database licensing](https://www.oracle.com/database/technologies/oracle-database-licensing.html)。 | **未证实**：支持权利以 Oracle Support/产品合同为准。 |
| IBM Db2 | IBM 将 JDBC/SQLJ 列为 [IBM Data Server Driver Package](https://www.ibm.com/docs/en/db2/11.5.x?topic=drivers-data-server-driver-packages) 的支持对象；安装/下载走 IBM Data Server Driver Package 或 IBM 支持下载渠道。 | 该 IBM 文档描述的是二进制 driver package，并未提供 JDBC/SQLJ driver 的官方源码仓库或开源许可证。 | **否（没有官方开源发布证据）**。 | **未证实**：未找到适用于当前下载版本、由 IBM 官方公开给出的独立驱动价格/免费声明；须接受下载许可并按目标包核验。 | Db2 Server、Db2 on Cloud、Db2 Connect 等许可/服务费用是独立问题；不能由 driver package 推导。见 [Db2 licensing](https://www.ibm.com/docs/en/db2/11.5.x?topic=licensing-db2-licenses)。 | **未证实**：IBM 支持资格/费用依订阅或合同，不能当作 JDBC 驱动费用。 |

## 对 `rtop` 的迁移含义

1. 这些 JDBC 驱动都是 **JVM API 的实现**；即使 Dremio 的 JDBC client 开源，也不能成为“只采用 Rust 协议栈”的运行时依赖。
2. 不应把“有官方二进制 JDBC 驱动”写成“Rust 可直接替代”。应逐源选择公开服务协议：Athena/BigQuery 的 HTTP API、Dremio 的 Flight SQL（若部署启用）等；Denodo、Oracle、Db2 在严格 Rust 协议栈下仍须单独可行性验证。
3. 若日后允许 Java/JDBC sidecar 或原生厂商客户端，应新增逐版本的许可与部署验收票据；本表不是再分发权利意见。

## 不确定性与后续核验

- Athena、BigQuery、Denodo、Db2：本次结论是“没有找到官方开源发布证据”，而不是声称所有历史版本绝无源码。采用具体版本前，必须保存其 jar、LICENSE/EULA、下载页和 SHA-256。
- 费用：没有把厂商营销试用、云赠金、企业折扣或支持捆绑误记为驱动价格；未有官方证据的地方均为“未证实”。
- Dremio：结论仅覆盖公开的 `dremio-oss/client/jdbc` 源码路径，不覆盖 Dremio 所有商业产品或任何第三方 Simba 驱动。
