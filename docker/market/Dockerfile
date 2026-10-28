# syntax=docker/dockerfile:1
# Build the market API and worker, including optional development custody.
FROM rust:1.94.0-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps/market ./apps/market
COPY apps/arbiter ./apps/arbiter
COPY contracts/demo-nft/artifacts ./contracts/demo-nft/artifacts

ARG DEMO_MODE=false
ARG CARGO_BUILD_JOBS=1
RUN if [ "$DEMO_MODE" = "true" ]; then \
      cargo build --locked --release --package market --features demo-mode; \
    else cargo build --locked --release --package market; fi

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 market \
    && useradd --uid 10001 --gid market --no-create-home market \
    && install -d -o market -g market /var/lib/market/media

COPY --from=build /source/target/release/market /usr/local/bin/market

USER market:market
WORKDIR /var/lib/market
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/market"]
CMD ["serve"]
