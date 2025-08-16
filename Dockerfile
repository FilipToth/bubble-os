FROM debian:bookworm-slim
WORKDIR /build

RUN apt-get update && apt-get install -y \
    make \
    nasm \
    grub-pc-bin xorriso mtools dosfstools \
    qemu-system-x86 netcat-openbsd \
    build-essential bison flex libgmp3-dev libmpc-dev libmpfr-dev texinfo \
    wget curl git pkg-config libssl-dev ca-certificates \
 && rm -rf /var/lib/apt/lists/*

ENV PATH="/root/.cargo/bin:${PATH}"
ARG RUST_CHANNEL=nightly

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "${RUST_CHANNEL}" \
    && rustup component add rust-src --toolchain "${RUST_CHANNEL}"

RUN wget -O /usr/local/bin/xargo \
    https://github.com/FilipToth/xargo/releases/download/target/xargo \
    && chmod +x /usr/local/bin/xargo

CMD ["make", "full_build"]
