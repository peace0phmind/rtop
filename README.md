# rtop

## 容器

镜像默认运行 `rtop endpoint /etc/rtop/rtop.toml`。它是仅含静态 `rtop` 与静态 BusyBox 的 `scratch` 镜像，以 UID `10001` 运行；PostgreSQL 必须作为独立服务提供。将 VKG 配置、mapping、facts 或本体挂载到容器，并可在配置中使用相对 `password_file` 保存凭证；不要把 password 直接写入镜像层。endpoint 可使用只读根文件系统运行，镜像没有交互 shell。

```sh
docker run --rm -p 8080:8080 \
  -v "$PWD/rtop.toml:/etc/rtop/rtop.toml:ro" \
  -v "$PWD/secret:/etc/rtop/secret:ro" \
  rtop
```

覆盖 entrypoint 参数可运行已支持的 CLI，例如：`docker run --rm --entrypoint /rtop rtop validate /etc/rtop/rtop.toml`。healthcheck 使用 `/busybox wget` 访问本地 `/healthz`；该路径只表示进程存活，不表示 PostgreSQL 已就绪。
