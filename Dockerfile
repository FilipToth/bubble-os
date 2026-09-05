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


# ---------------------------------------------------------------------------
# x86_64-elf cross toolchain
#
# The host gcc in this image targets x86_64-linux-gnu and sees glibc's
# headers, so it cannot build a freestanding libc. newlib's configure also
# looks for the target tools under their `x86_64-elf-` prefix. Both problems
# go away with a real cross toolchain, which is what the bison/flex/gmp/mpc/
# mpfr/texinfo packages above are for.
#
# This is the expensive layer: roughly half an hour on a first build. It is
# also completely static, so it is cached from then on and never rebuilds.
# ---------------------------------------------------------------------------

ENV BUBBLE_TARGET=x86_64-elf
ENV BUBBLE_PREFIX=/opt/bubble-toolchain
ENV PATH="/opt/bubble-toolchain/bin:${PATH}"

ARG BINUTILS_VERSION=2.43
ARG GCC_VERSION=14.2.0

RUN mkdir -p /opt/src && cd /opt/src \
 && wget -q https://ftp.gnu.org/gnu/binutils/binutils-${BINUTILS_VERSION}.tar.xz \
 && tar -xf binutils-${BINUTILS_VERSION}.tar.xz \
 && mkdir build-binutils && cd build-binutils \
 && ../binutils-${BINUTILS_VERSION}/configure \
        --target="${BUBBLE_TARGET}" \
        --prefix="${BUBBLE_PREFIX}" \
        --with-sysroot \
        --disable-nls \
        --disable-werror \
 && make -j"$(nproc)" \
 && make install \
 && cd /opt/src \
 && rm -rf binutils-${BINUTILS_VERSION} build-binutils binutils-${BINUTILS_VERSION}.tar.xz

# Stage-1 gcc: --without-headers because there is no libc to build against
# yet, which is the chicken-and-egg newlib is about to resolve. Only all-gcc
# and all-target-libgcc are built; the rest of the tree needs a libc.
RUN cd /opt/src \
 && wget -q https://ftp.gnu.org/gnu/gcc/gcc-${GCC_VERSION}/gcc-${GCC_VERSION}.tar.xz \
 && tar -xf gcc-${GCC_VERSION}.tar.xz \
 && mkdir build-gcc && cd build-gcc \
 && ../gcc-${GCC_VERSION}/configure \
        --target="${BUBBLE_TARGET}" \
        --prefix="${BUBBLE_PREFIX}" \
        --disable-nls \
        --disable-multilib \
        --enable-languages=c \
        --without-headers \
 && make -j"$(nproc)" all-gcc \
 && make -j"$(nproc)" all-target-libgcc \
 && make install-gcc install-target-libgcc \
 && cd /opt/src \
 && rm -rf gcc-${GCC_VERSION} build-gcc gcc-${GCC_VERSION}.tar.xz

# ---------------------------------------------------------------------------
# newlib source
#
# Fetched and unpacked here rather than into the repo: it is a toolchain
# dependency, not source. /opt/newlib is inside the image, so it never
# appears in the /workspace bind mount. Configuring and building it is
# build.mk's `newlib` target, so the flags can change without a 30 minute
# image rebuild.
# ---------------------------------------------------------------------------

ARG NEWLIB_VERSION=4.6.0.20260123
ENV NEWLIB_VERSION=${NEWLIB_VERSION}
ENV NEWLIB_SRC=/opt/newlib/newlib-${NEWLIB_VERSION}

RUN mkdir -p /opt/newlib && cd /opt/newlib \
 && wget -q "ftp://sourceware.org/pub/newlib/newlib-${NEWLIB_VERSION}.tar.gz" \
 && tar -xf newlib-${NEWLIB_VERSION}.tar.gz \
 && rm newlib-${NEWLIB_VERSION}.tar.gz

# compose overrides this with `sleep infinity` and drives builds through
# `docker compose exec`. This keeps `docker run <image>` doing a full build
CMD ["make", "-f", "build.mk", "full_build"]
