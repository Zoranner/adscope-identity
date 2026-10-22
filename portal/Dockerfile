FROM rust:1.93.1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY core/ core/
COPY syncer/ syncer/
COPY crates/ crates/
COPY portal/ portal/
RUN cargo build --release --locked -p adscope-portal

FROM debian:bookworm-slim
ARG VERSION=0.2.0
ARG REVISION=unknown
LABEL org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --home /app adscope
COPY --from=rust-build /src/target/release/adscope-portal /app/adscope-portal
COPY portal/static/ /app/web/
ENV LISTEN_ADDR=0.0.0.0:18086 \
    WEB_ROOT=/app/web
WORKDIR /app
USER 1000:1000
EXPOSE 18086
ENTRYPOINT ["/app/adscope-portal"]
