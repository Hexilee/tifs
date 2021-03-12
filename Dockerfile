FROM ubuntu:20.04
RUN apt-get update 
RUN apt-get install -y libfuse3-dev fuse3 wget
RUN wget https://github.com/Hexilee/tifs/releases/download/v0.1.0/tifs-linux-amd64.tar.gz
RUN tar -xvf tifs-linux-amd64.tar.gz
RUN mv ./bin/release/tifs /tifs
ENTRYPOINT ["/tifs"]
