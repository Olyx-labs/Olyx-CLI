FROM rust:1.88-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY runtime ./runtime
RUN cargo build --locked --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 olyx && useradd --uid 10001 --gid olyx --no-create-home olyx \
    && mkdir -p /data /etc/olyx && chown olyx:olyx /data
COPY --from=build /build/target/release/oy /usr/local/bin/oy
COPY containers/olyx.json /etc/olyx/olyx.json
COPY LICENSE NOTICE /usr/share/doc/olyx/
USER 10001:10001
WORKDIR /data
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=3s CMD curl --fail --silent http://127.0.0.1:8787/readyz || exit 1
ENTRYPOINT ["oy"]
CMD ["serve", "--config", "/etc/olyx/olyx.json", "--host", "0.0.0.0", "--port", "8787"]
