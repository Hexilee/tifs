#![feature(map_into_keys_values)]
#![feature(async_closure)]
#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]

mod fs;

use clap::{crate_version, App, Arg};
use fuser::MountOption;
use tracing::Level;
use tracing_subscriber::EnvFilter;

use fs::async_fs::AsyncFs;
use fs::tikv_fs::TiFs;

#[async_std::main]
async fn main() {
    let matches = App::new("TiFS")
        .version(crate_version!())
        .author("Hexi Lee")
        .arg(
            Arg::with_name("pd")
                .long("pd endpoints")
                .multiple(true)
                .value_name("ENDPOINTS")
                .default_value("127.0.0.1:2379")
                .help("set all pd endpoints of the tikv cluster")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("mount-point")
                .long("mount-point")
                .value_name("MOUNT_POINT")
                .default_value("")
                .help("Act as a client, and mount FUSE at given path")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .get_matches();

    let verbosity: u64 = matches.occurrences_of("v");
    let log_level = match verbosity {
        0 => Level::ERROR,
        1 => Level::WARN,
        2 => Level::INFO,
        3 => Level::DEBUG,
        _ => Level::TRACE,
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_env_filter(EnvFilter::from_default_env().add_directive("echo=trace".parse().unwrap()))
        .try_init()
        .unwrap();

    let mut options = vec![
        MountOption::FSName("tifs".to_string()),
        MountOption::AutoUnmount,
    ];

    let endpoints: Vec<&str> = matches
        .values_of("pd")
        .unwrap_or_default()
        .to_owned()
        .collect();

    let mountpoint: String = matches.value_of("mount-point").unwrap().to_string();
    let fs_impl = TiFs::construct(endpoints, Default::default())
        .await
        .unwrap();
    fuser::mount2(AsyncFs::from(fs_impl), mountpoint, &options).unwrap();
}
