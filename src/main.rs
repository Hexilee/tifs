use clap::{crate_version, App, Arg};
use tracing_subscriber::EnvFilter;

use tifs::mount_tifs;
use tifs::MountOption;

#[async_std::main]
async fn main() {
    let matches = App::new("TiFS")
        .version(crate_version!())
        .author("Hexi Lee")
        .arg(
            Arg::with_name("pd")
                .long("pd-endpoints")
                .short("p")
                .multiple(true)
                .value_name("ENDPOINTS")
                .default_value("127.0.0.1:2379")
                .help("set all pd endpoints of the tikv cluster")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("mount-point")
                .long("mount-point")
                .short("m")
                .value_name("MOUNT_POINT")
                .required(true)
                .help("Act as a client, and mount FUSE at given path")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("options")
                .value_name("OPTION")
                .long("option")
                .short("o")
                .multiple(true)
                .help("filesystem mount options"),
        )
        .get_matches();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init()
        .unwrap();

    let endpoints: Vec<&str> = matches
        .values_of("pd")
        .unwrap_or_default()
        .to_owned()
        .collect();

    let mountpoint: String = matches.value_of("mount-point").unwrap().to_string();
    let options = MountOption::to_vec(matches.values_of("options").unwrap_or_default());

    mount_tifs(mountpoint, endpoints, options).await.unwrap();
}
