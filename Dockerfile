FROM maven:3.9-eclipse-temurin-17 AS build
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends build-essential && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.85.1
ENV PATH=/root/.cargo/bin:$PATH
COPY . .
RUN cargo build --release --locked

FROM postgres:17@sha256:5c855ad7b85e68e48a62f34662853f38b57c1c1d80f3a927ab58034fd6d31c5e
RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*
USER postgres
COPY --from=build /src/target/release/rtop /usr/local/bin/rtop
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fsS http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["rtop", "endpoint"]
CMD ["/etc/rtop/rtop.toml"]
