# syntax=docker/dockerfile:experimental

FROM ubuntu:22.04 as builder
RUN ln -fs /usr/share/zoneinfo/America/New_York /etc/localtime

RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    ca-certificates curl file libssl-dev \
    build-essential \
    autoconf automake autotools-dev libtool xutils-dev \
    libfuse3-dev fuse3 pkgconf cmake && \
    rm -rf /var/lib/apt/lists/*

# install toolchain
RUN curl https://sh.rustup.rs -sSf | \
    sh -s -- --default-toolchain nightly-2021-06-01 -y
ENV PATH=/root/.cargo/bin:$PATH
COPY . /tifs-build
WORKDIR /tifs-build
RUN --mount=type=cache,target=/tifs-build/target \
    --mount=type=cache,target=/root/.cargo/registry \
    cargo build --release --all
RUN --mount=type=cache,target=/tifs-build/target \
    cp /tifs-build/target/release/tifs /tifs-build/target/tifs
ENTRYPOINT ["/tifs"]
