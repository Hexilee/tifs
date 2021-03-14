FROM ubuntu:20.04 as builder
WORKDIR /src
RUN apt-get update 
RUN apt-get install -y libfuse3-dev build-essential
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
COPY . .
RUN cargo build --features "binc" --no-default-features --all --release

FROM ubuntu:20.04
RUN apt-get update
RUN apt-get install -y libfuse3-dev fuse3
COPY --from=builder /src/target/release/tifs /tifs
ENTRYPOINT ["/tifs"]
