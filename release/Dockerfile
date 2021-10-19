# syntax=docker/dockerfile:experimental

FROM debian:buster-slim

ENV DEBIAN_FRONTEND noninteractive

ARG HTTPS_PROXY
ARG HTTP_PROXY
ARG TARGET=x86_64-unknown-linux-gnu
ARG CC=gcc

ENV http_proxy $HTTP_PROXY
ENV https_proxy $HTTPS_PROXY

RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    ca-certificates curl file libssl-dev \
    build-essential \
    autoconf automake autotools-dev libtool xutils-dev \
    libfuse3-dev pkgconf cmake && \
    rm -rf /var/lib/apt/lists/*

# install toolchain
RUN curl https://sh.rustup.rs -sSf | \
    sh -s -- --default-toolchain nightly-2021-06-01 -y
ENV PATH=/root/.cargo/bin:$PATH

RUN rustup target add $TARGET

RUN if [ -n "$HTTP_PROXY" ]; then echo "[http]\n\
    proxy = \"${HTTP_PROXY}\"\n\
    "\
    >> /root/.cargo/config ; fi

RUN echo "[target.aarch64-unknown-linux-gnu]\n\
    linker = \"aarch64-linux-gnu-gcc\"\n\
    "\
    >> /root/.cargo/config

COPY . /tifs-build

WORKDIR /tifs-build

RUN --mount=type=cache,target=/tifs-build/target \
    --mount=type=cache,target=/root/.cargo/registry \
    cargo build --features "binc" --release --target $TARGET

RUN --mount=type=cache,target=/tifs-build/target \
    cp /tifs-build/target/$TARGET/release/tifs /tifs