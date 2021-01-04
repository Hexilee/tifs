use clap::{crate_version, App, Arg};
use tracing_subscriber::EnvFilter;

use tifs::MountOption;
use tifs::mount_tifs_daemonize;
//use daemonize::Daemonize;

#[async_std::main]
async fn main() {
    let matches = App::new("mount.tifs")
        .version(crate_version!())
        .author("Hexi Lee")
        .arg(
            Arg::with_name("device")
                .value_name("ENDPOINTS")
                .required(true)
                .help("all pd endpoints of the tikv cluster, separated by commas (e.g. tifs:127.0.0.1:2379)")
                .index(1)
        )
        .arg(
            Arg::with_name("mount-point")
                .value_name("MOUNT_POINT")
                .required(true)
                .help("Act as a client, and mount FUSE at given path")
                .index(2)
        )
        .arg(
            Arg::with_name("options")
                .value_name("OPTION")
                .short("o")
                .multiple(true)
                .help("filesystem mount options")
        )
        .get_matches();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .unwrap();

    let device = matches
        .value_of("device")
        .unwrap_or_default();

    let endpoints: Vec<&str> = device
        .strip_prefix("tifs:")
        .unwrap_or(device)
        .split(",")
        .collect();

    let mountpoint: String = matches.value_of("mount-point").unwrap().to_string();
    let options = MountOption::to_vec(matches
        .values_of("options")
        .unwrap_or_default());

    mount_tifs_daemonize(mountpoint, endpoints, options, || {
//        Daemonize::new().working_directory("/").start()?;

        Ok(())
    }).await.unwrap();
}
