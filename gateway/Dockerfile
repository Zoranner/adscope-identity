FROM rust:1.93.1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY center/ center/
COPY connector/ connector/
COPY crates/ crates/
COPY gateway/ gateway/
RUN cargo build --release --locked -p adscope-gateway

FROM debian:bookworm-slim
ARG VERSION=0.2.0
ARG REVISION=unknown
LABEL org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app adscope
COPY --from=rust-build /src/target/release/adscope-gateway /app/adscope-gateway
COPY gateway/static/ /app/web/
ENV LISTEN_ADDR=0.0.0.0:8443 \
    WEB_ROOT=/app/web
WORKDIR /app
USER 10001:10001
EXPOSE 8443
ENTRYPOINT ["/app/adscope-gateway"]
