# syntax=docker/dockerfile:1
FROM debian:bookworm-slim
ARG TARGETARCH
RUN test "$TARGETARCH" = amd64 \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl python3 libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /tmp/dependencies
RUN curl -fsSLo foundry.tar.gz https://github.com/foundry-rs/foundry/releases/download/v1.8.6/foundry_v1.8.6_linux_amd64.tar.gz \
    && echo '58850106984038f3a7200f38a4182c8cb423d3a4f4c7848eacbc2453e6bc35fc  foundry.tar.gz' | sha256sum -c - \
    && tar xzf foundry.tar.gz -C /usr/local/bin anvil
RUN curl -fsSLo zebra.tar.gz https://github.com/ZcashFoundation/zebra/releases/download/v7.0.0/zebrad-7.0.0-x86_64-unknown-linux-gnu.tar.gz \
    && echo 'eb9fb2499c6502489d1c85af22c777af6073a49f2d634ba89e546fff329d5cce  zebra.tar.gz' | sha256sum -c - \
    && tar xzf zebra.tar.gz -C /usr/local/bin zebrad
RUN curl -fsSLo zallet.tar.gz https://github.com/zcash/zallet/releases/download/v0.1.0-beta.3/zallet-v0.1.0-beta.3-linux-amd64.tar.gz \
    && echo '1df2398df016ae6e9a1cba3f8ef0cc86ca0d7b48a0934b34df6f68114ebe9306  zallet.tar.gz' | sha256sum -c - \
    && tar xzf zallet.tar.gz --strip-components=1 -C /usr/local/bin zallet-v0.1.0-beta.3-linux-amd64/zallet-zaino \
    && rm -rf /tmp/dependencies
COPY scripts/localnet /opt/localnet
ENV PYTHONUNBUFFERED=1
WORKDIR /opt/localnet
ENTRYPOINT ["python3", "/opt/localnet/node.py"]
