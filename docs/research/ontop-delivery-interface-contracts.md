# Ontop 交付接口与 Java 生态契约审计

**参考版本：** Ontop `version5` 提交 `5ec07573b18513f33dfcd59ac45fe26a81f9cdbd`。
本文件区分跨语言可观察契约与 Java 宿主绑定；后者不因可被终端用户看见就成为
`rtop` 的迁移目标。

## 决策

`rtop` 交付一个原生二进制 CLI、一个 OCI/Docker 镜像和 SPARQL HTTP 服务。保留
`/sparql` 的协议形状、CLI 的任务语义与输入/输出文件；`/ontop/reformulate` 仅作为
明确标注的开发诊断接口。RDF4J Repository、OWLAPI Engine、Protégé 插件和任何
JVM/JDBC 配置、JAR/Maven/OSGi 发行物全部排除。

## HTTP 端点

| 接口 | 参考可观察行为 | `rtop` 决定 |
| --- | --- | --- |
| `GET /sparql` | 必需 `Accept` 与 `query`；可重复 `default-graph-uri`、`named-graph-uri` | 保留。 |
| `POST /sparql` | 支持 `application/x-www-form-urlencoded` 的 `query`，或 `application/sparql-query` 的裸查询体；同样接受 graph URI 参数 | 保留，并按 SPARQL Protocol / 查询结果格式协商做黑盒验收。 |
| 错误 | RDF4J `MalformedQueryException` → 400、`text/plain; charset=UTF-8`；其余 repository/执行异常 → 500、同类文本且 `Cache-Control: no-store` | 保留 HTTP status、content type 与 no-store 语义；不承诺 Java exception 文本或 stack trace。 |
| `/ontop/reformulate` | 只在 `dev=true` 存在；`query` 与可选 `forNativeConsumption`，成功返回 `text/plain`、`X-Query-ID` 和改写文本 | 开发诊断接口，可保留同 URL/参数/header；SQL 文本受目标 dialect 影响，验收只固定它的存在、错误分类和同一基线下的快照。 |
| `/ontology` | 仅 `enable-download-ontology=true` 时存在；无本体时 404 `No ontology found`，否则文本序列化 | 可选 adapter；实现时保留开关和 404，不把 OWLAPI 序列化格式当作固定字节契约。 |
| `/predefined/{id}`、`/ontop/portalConfig`、`/` portal、`/ontop/restart` | 分别依赖 RDF4J predefined-query engine、TOML/静态 web UI，及 Spring 进程内重启/file watcher | **不在首期交付。** 若产品需要，分别在 Rust HTTP 层重新立票；它们不是核心 VKG 协议。 |

源码：`client/endpoint/.../SparqlQueryController.java`、`ReformulateController.java`、
`OntologyFetcherController.java`、`PredefinedQueryController.java`、
`Portal*Controller.java`、`AutoRestartController.java`。

## CLI 与文件任务

`Ontop.getOntopCommandCLI()` 注册可见命令 `version`、`help`、`query`、`materialize`、
`bootstrap`、`validate`、`endpoint`、`extract-db-metadata`，以及隐藏的 `compile`；mapping
组另有 `to-r2rml`、`to-obda`、`pretty-r2rml`、`v1-to-v3`。它们的 AirLine parser、Java
异常打印和 shell-completion 生成器不是迁移对象，但下面的任务边界是：

| CLI 任务 | `rtop` 发布决策 |
| --- | --- |
| `query`、`endpoint`、`validate` | 核心交付；以本体、映射、facts、`rtop` 数据源配置和 SPARQL 输入为契约。 |
| `materialize`、`bootstrap`、`extract-db-metadata` | 仅在不绕过 `DataSource` adapter 且有对应服务端验收时实现；不是核心查询上线门槛。 |
| `to-r2rml`、`to-obda`、`pretty-r2rml`、`v1-to-v3` | 映射互操作工具；前两项的 Turtle/.obda 语义与 #4 一致，legacy v1 转换另以 fixture 固定。 |
| `compile`、`version`、`help`、completion | 前三者作为可用性功能；completion 可按目标 shell 重新实现，不以 AirLine 输出逐字兼容。 |

旧命令和 Docker 环境中的 `--db-url`、`--db-driver`、`jdbc.*`/`ONTOP_DB_URL`/`ONTOP_DB_DRIVER`
均是 JDBC 契约，**不接受为 `rtop` 运行时配置**。`rtop` 配置必须命名所选 adapter 及其
非 Java 连接字段；旧配置转换只能是之后独立工具。

源码：`client/cli/.../Ontop.java` 及各 `Ontop*.java` command；`client/docker/README.md`。

## 容器与发行

参考镜像默认运行 endpoint，也可 `docker run ontop/ontop ontop <command> …` 执行 CLI；它
映射一批 `ONTOP_*` 变量、包含 JVM/JMX 参数、JDBC driver 投放目录、Java healthcheck 和
entrypoint 等机制。`rtop` OCI 镜像仅保留以下交付效果：

1. 默认启动 `endpoint`，并可覆写为任何已支持的 `rtop` CLI 子命令；
2. 从文件/环境读取 mapping、ontology、facts 和**原生 adapter**配置（敏感值支持
   `_FILE` 模式）；
3. 有 HTTP readiness/query healthcheck，且不会捆绑未获再分发许可的 ODBC/厂商驱动；
4. 发布 Linux 原生二进制、镜像、SBOM/许可证材料，而非 ZIP+JAR、Maven artifact 或 JRE
   打包的 Protégé bundle。

`ONTOP_WAIT_FOR` 的“等待 host:port”是可保留的无语言行为；JMX、`ONTOP_JAVA_ARGS`、
`ONTOP_FILE_ENCODING` 的 Java 具体语义排除。Docker 健康检查的 `process`/`port`/SPARQL
probe 意图可保留，但不复用 shell/JVM 实现。

证据：`client/docker/Dockerfile`、`entrypoint.sh`、`healthcheck.sh`、`README.md`，以及
`build/distribution/src/assembly/ontop-cli.xml`、`build/distribution/README.md`。

## 明确排除的 Java 进程内接口

| 参考区域 | 排除理由 |
| --- | --- |
| `binding/rdf4j` 的 `OntopRepository`、connection、query、result iteration 与 predefined engine | RDF4J Java 类型及进程内生命周期，不是跨语言网络协议。 |
| `binding/owlapi` 的 `OntopOWLEngine`、reasoner、OWL connection/statement/result set | OWLAPI Java API；本体文件与 OWL 2 QL 转换语义已在输入层迁移。 |
| `protege/*` 的 reasoner、Swing UI、OSGi plugin 与桌面 bundle | Java/Protégé 宿主、GUI 和打包契约。 |
| Spring Boot/Tomcat/Servlet、AirLine、Guice、SLF4J | 参考实现技术，不是外部效果；由 Rust web/CLI/DI/logging 替换。 |

## 验收边界

HTTP 用 `curl`/SPARQL Protocol fixture 比较 status、headers、协商结果和 RDF/tuple/boolean
结果；CLI 用退出码、stdout/stderr、生成文件和错误代码比较。Docker 在无 JVM 镜像中验证
同一组 HTTP/CLI 情景。任何需 RDF4J、OWLAPI、Protégé、JDBC URL/driver class 或 Java
进程内对象的测试，均只能证明参考行为，不能作为 `rtop` 发布门槛。
