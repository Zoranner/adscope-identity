FROM oven/bun:1.3.14-debian AS web-build
WORKDIR /src/center/web
COPY center/web/package.json center/web/bun.lock ./
RUN bun install --frozen-lockfile
COPY center/web/ ./
RUN bun run build

FROM rust:1.93.1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY center/ center/
COPY connector/ connector/
COPY crates/ crates/
RUN cargo build --release --locked -p adss-center

FROM debian:bookworm-slim
ARG VERSION=0.1.0
ARG REVISION=unknown
LABEL org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app adss \
    && mkdir -p /app/web /data \
    && chown -R adss:adss /app /data
COPY --from=rust-build /src/target/release/adss-center /app/adss-center
COPY --from=web-build /src/center/web/.output/public/ /app/web/
ENV ADSS_BIND_ADDR=0.0.0.0:8080 \
    ADSS_WEB_ROOT=/app/web \
    ADSS_DATABASE_URL=sqlite:///data/adss.db?mode=rwc
USER 10001:10001
VOLUME ["/data"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD curl --fail --silent http://127.0.0.1:8080/api/health || exit 1
ENTRYPOINT ["/app/adss-center"]
