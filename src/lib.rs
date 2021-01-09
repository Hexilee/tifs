#![feature(map_into_keys_values)]
#![feature(async_closure)]

pub mod fs;

use fs::async_fs::AsyncFs;
use fs::tikv_fs::TiFs;

use fuser::MountOption as FuseMountOption;
use paste::paste;

macro_rules! define_options {
    { $name: ident, [ $($newopt: ident),* $(,)? ], [ $($opt: ident),* $(,)? ] } =>
    {
        define_options!{ $name(FuseMountOption), [ $($newopt,)* ], [ $($opt,)* ]}
    };
    { $name: ident ($type: ident), [ $($newopt: ident),* $(,)? ], [ $($opt: ident),* $(,)? ] } =>
    {
        #[derive(Debug,Clone)]
        pub enum $name {
            Unknown(String),
            $($opt,)*
            $($newopt,)*
        }
        impl $name {
            pub fn to_vec<'a, I: Iterator<Item=&'a str>>(iter: I) -> Vec<Self> {
                iter.map(|v| v.split(',').map(|v| Self::from(v))).flatten().collect()
            }
            pub fn to_builtin<'a, I: Iterator<Item=&'a Self>>(iter: I) -> Vec<$type> {
                iter.filter_map(|v| v.into_builtin()).collect()
            }
            pub fn into_builtin(&self) -> Option<$type> {
                match self {
                    $(Self::$opt => Some($type::$opt),)*
                    _ => None,
                }
            }
        }
        paste! {
            impl<T> From<T> for $name
            where
                T: ToString
            {
                fn from(v: T) -> Self {
                    match &v.to_string() as &str {
                        $(stringify!([<$opt:lower>]) => Self::$opt,)*
                        $(stringify!([<$newopt:snake>]) => Self::$newopt,)*
                        k => Self::Unknown(k.to_owned()),
                    }
                }
            }
            impl From<&$name> for String {
                fn from(v: &$name) -> Self {
                    match v {
                        $($name::$opt => stringify!([<$opt:lower>]),)*
                        $($name::$newopt => stringify!([<$newopt:snake>]),)*
                        $name::Unknown(v) => v,
                    }.to_owned()
                }
            }
        }
    };
}

define_options! { MountOption, [DirectIO], [
    Dev,
    NoDev,
    Suid,
    NoSuid,
    RO,
    RW,
    Exec,
    NoExec,
    DirSync,
]}

pub async fn mount_tifs_daemonize<F>(
    mountpoint: String,
    endpoints: Vec<&str>,
    options: Vec<MountOption>,
    make_daemon: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<()>,
{
    let mut fuse_options = vec![
        FuseMountOption::FSName(format!("tifs:{}", endpoints.join(","))),
        FuseMountOption::AllowOther,
        FuseMountOption::AutoUnmount,
        FuseMountOption::DefaultPermissions,
    ];

    fuse_options.extend(MountOption::to_builtin(options.iter()));

    let fs_impl = TiFs::construct(endpoints, Default::default(), options).await?;

    make_daemon()?;

    fuser::mount2(AsyncFs::from(fs_impl), mountpoint, &fuse_options)?;

    Ok(())
}

pub async fn mount_tifs(
    mountpoint: String,
    endpoints: Vec<&str>,
    options: Vec<MountOption>,
) -> anyhow::Result<()> {
    mount_tifs_daemonize(mountpoint, endpoints, options, || Ok(())).await
}
