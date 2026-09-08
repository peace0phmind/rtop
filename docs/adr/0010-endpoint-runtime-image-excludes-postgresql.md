# endpoint 运行时镜像不包含 PostgreSQL

状态：accepted。rtop 的 endpoint 运行时镜像只交付 Rust CLI、HTTP endpoint、健康检查和必要运行时依赖；PostgreSQL 是由 Compose、Kubernetes 或其他部署拓扑独立提供的关系数据源。最终层使用 `scratch`，只复制静态 `rtop` 和静态 BusyBox；以数值 UID `10001` 运行，并受未压缩根文件系统和压缩层传输两项大小门槛约束。健康检查使用 BusyBox `wget` 的 exec 形式，不以保留 shell 或未启动的 PostgreSQL 服务端换取运维工具。

## 影响

构建阶段使用 Alpine Rust 工具链并安装 `build-base`：当前依赖的过程宏在该阶段链接时需要 `crti.o`，但编译器和开发文件不得进入最终层。Alpine 构建器和 BusyBox 均须按目标平台固定 digest。实测静态二进制约 11 MiB，`scratch` 可行镜像导出的运行时根文件系统为 12,755,968 bytes，`docker image inspect .Size` 为 4,814,713 bytes；OCI gate 须以前者不超过 20 MiB、后者不超过 6 MiB 判定。两项分别保留运行时 rootfs 与镜像检查传输大小门槛，不能互相替代。

OCI 验收必须继续以独立的固定 digest `postgres:17` 容器验证网络连接、挂载配置、默认 endpoint、CLI 覆写和 healthcheck。由于最终层没有 `/bin/sh`，无 JVM 断言须改为检查镜像文件清单和显式 BusyBox/rtop 调用，不能以 shell 存在为前提；交互式诊断使用临时调试容器，而非向发布镜像加入 shell。`/healthz` 只表示 endpoint 进程可用，不把关系数据源连通性伪装为存活状态；无效配置或不可达 PostgreSQL 必须仍可由 SPARQL 请求给出稳定诊断。

endpoint 读取只读挂载的配置、映射、facts、本体和 `password_file`，且在只读根文件系统中必须可启动；显式产生文件的 CLI 子命令由调用方提供可写的输出挂载路径。容器由 Docker 注入 DNS 配置，数据源用户名由 rtop 配置显式提供；当前 PostgreSQL adapter 使用 `NoTls`。若未来加入 PostgreSQL TLS、远程资源获取、CA 证书、时区文件或镜像内诊断工具，必须先作独立运行时依赖决策并重新通过大小 gate。首个发布验收平台为 Linux/amd64；新增平台须单独构建并通过同一 OCI gate。
