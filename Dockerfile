# Refresh reviewed base-image digests deliberately; Cargo.lock pins Rust dependencies.
FROM rust:1.97-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release && cp target/release/chronexus-server /chronexus-server

FROM debian:bookworm@sha256:854dd8ab51d62866dbd8f5431d15ce77b12983a2a4a1e1fe43a2395dc7ef9f61
RUN groupadd --gid 10001 chronexus && useradd --uid 10001 --gid 10001 --no-create-home chronexus \
    && mkdir -p /var/lib/chronexus && chown 10001:10001 /var/lib/chronexus && chmod 700 /var/lib/chronexus
COPY --from=build /chronexus-server /usr/local/bin/chronexus-server
USER 10001:10001
ENV CHRONEXUS_DATABASE=/var/lib/chronexus/chronexus.sqlite3
WORKDIR /var/lib/chronexus
EXPOSE 8080
ENTRYPOINT ["chronexus-server"]
CMD ["serve"]
