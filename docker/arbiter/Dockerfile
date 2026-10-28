# syntax=docker/dockerfile:1
FROM rust:1.94.0-bookworm AS build
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps/market ./apps/market
COPY apps/arbiter ./apps/arbiter
ARG DEMO_MODE=false
ARG CARGO_BUILD_JOBS=1
RUN if [ "$DEMO_MODE" = "true" ]; then \
      cargo build --locked --release --package arbiter --features demo-mode; \
    else cargo build --locked --release --package arbiter; fi

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10002 arbiter \
    && useradd --uid 10002 --gid arbiter --no-create-home arbiter \
    && install -d -o arbiter -g arbiter /var/lib/arbiter/artifacts
COPY --from=build /source/target/release/arbiter /usr/local/bin/arbiter
USER arbiter:arbiter
WORKDIR /var/lib/arbiter
ENTRYPOINT ["/usr/local/bin/arbiter"]
CMD ["serve"]
