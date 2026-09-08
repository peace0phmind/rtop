FROM --platform=linux/amd64 rust:1.85.1-alpine3.21@sha256:813376c206852d4250641eb86720c04fd20fbc6547d1a030c93a45b22a1303a3 AS build
WORKDIR /src
RUN apk add --no-cache build-base
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --target x86_64-unknown-linux-musl \
    && strip target/x86_64-unknown-linux-musl/release/rtop

FROM --platform=linux/amd64 busybox:1.37.0-musl@sha256:29989570aeecad61a019f684218ea74d4b8c1c74f9e0abeb34ca926b81174ee1 AS busybox

FROM scratch
COPY --from=build /src/target/x86_64-unknown-linux-musl/release/rtop /rtop
COPY --from=busybox /bin/busybox /busybox
USER 10001
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD ["/busybox", "wget", "-q", "-O", "-", "http://127.0.0.1:8080/healthz"]
ENTRYPOINT ["/rtop", "endpoint"]
CMD ["/etc/rtop/rtop.toml"]
