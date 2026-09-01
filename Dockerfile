FROM rust:1.85-alpine AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM alpine:3.21
RUN adduser -D -u 10001 rtop
USER rtop
COPY --from=build /src/target/release/rtop /usr/local/bin/rtop
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["rtop", "endpoint"]
