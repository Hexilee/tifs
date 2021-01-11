### TiKV Cluster Deployment

You can install [tiup](https://github.com/pingcap/tiup) and just execute `tiup playground` to deploy a tikv cluster, however, it's not very suitable for tifs. To deploy a tikv cluster with high performance, this document may be helpful.

- create an user named 'tiup'
- execute `visudo` and add new line: `tiup ALL=(ALL) NOPASSWD: ALL`
- genenrate a pair of ssh key and execute `ssh-copy-id tiup@127.0.0.1` 
- install [tiup](https://github.com/pingcap/tiup) user 'tiup'
- create a file `tifs.yaml` in `~/.tiup`:
    ```yaml
    global:
      user: "tiup"
      ssh_port: 22
      deploy_dir: "/home/tiup/.tiup/deploy"
      data_dir: "/home/tiup/.tiup/deploy/data"

    server_configs:
      pd.replication.location-labels:
        - host
      tikv:
        rocksdb.titan.enabled: true

        # following resources config should be altered according to your machine
        readpool.unified.max-thread-count: 8
        storage.block-cache.capacity: 16GB
        server.grpc-concurrency: 8

    tikv_servers:
      - host: 127.0.0.1
        port: 20160
        status_port: 20180
        config.server.labels.host: "127.0.0.1"
      - host: 127.0.0.1
        port: 20161
        status_port: 20181
        config.server.labels.host: "127.0.0.1"
      - host: 127.0.0.1
        port: 20162
        status_port: 20182
        config.server.labels.host: "127.0.0.1"

    pd_servers:
      - host: 127.0.0.1

    monitoring_servers:
      - host: 127.0.0.1
    ```
- execute `tiup cluster deploy tifs nightly tifs.yaml`
- execute `tiup cluster start tifs`
