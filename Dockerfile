FROM ubuntu:20.04 as builder
RUN ln -fs /usr/share/zoneinfo/America/New_York /etc/localtime

RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    ca-certificates curl file libssl-dev \
    build-essential \
    autoconf automake autotools-dev libtool xutils-dev \
    libfuse3-dev pkgconf cmake && \
    rm -rf /var/lib/apt/lists/*

# install toolchain
RUN curl https://sh.rustup.rs -sSf | \
    sh -s -- --default-toolchain nightly -y
ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /src
COPY . .
RUN cargo build --features "binc" --no-default-features --all --release

FROM ubuntu:20.04
RUN apt-get update
RUN apt-get install -y libfuse3-dev fuse3 libssl-dev
COPY --from=builder /src/target/release/tifs /tifs
ENTRYPOINT ["/tifs"]
