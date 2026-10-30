FROM rust:1.93.1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY core/ core/
COPY syncer/ syncer/
COPY crates/ crates/
COPY portal/ portal/
RUN cargo build --release --locked -p adscope-core

FROM debian:bookworm-slim
ARG VERSION=0.4.0
ARG REVISION=unknown
LABEL org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --home /app adscope \
    && mkdir -p /app/data \
    && chown -R adscope:adscope /app
COPY --from=rust-build /src/target/release/adscope-core /app/adscope-core
ENV BIND_ADDR=0.0.0.0:8080
WORKDIR /app
USER 1000:1000
VOLUME ["/app/data"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD curl --fail --silent http://127.0.0.1:8080/api/health || exit 1
ENTRYPOINT ["/app/adscope-core"]
