#![feature(array_chunks)]

pub mod fs;

use std::path::PathBuf;

use fs::async_fs::AsyncFs;
use fs::client::TlsConfig;
use fs::tikv_fs::TiFs;
use fuser::MountOption as FuseMountOption;
use paste::paste;
use tokio::fs::{metadata, read_to_string};
use tracing::debug;

const DEFAULT_TLS_CONFIG_PATH: &str = "~/.tifs/tls.toml";

fn default_tls_config_path() -> anyhow::Result<PathBuf> {
    Ok(DEFAULT_TLS_CONFIG_PATH.parse()?)
}

macro_rules! define_options {
    {
        $name: ident ($type: ident) {
            $(builtin $($optname: literal)? $opt: ident,)*
            $(define $($newoptname: literal)? $newopt: ident $( ( $optval: ident ) )? ,)*
        }
    } =>
    {
        #[derive(Debug,Clone,PartialEq)]
        pub enum $name {
            Unknown(String),
            $($opt,)*
            $($newopt $(($optval))?,)*
        }
        impl $name {
            pub fn to_vec<'a, I: Iterator<Item=&'a str>>(iter: I) -> Vec<Self> {
                iter.map(|v| v.split(',').map(Self::from)).flatten().collect()
            }
            pub fn collect_builtin<'a, I: Iterator<Item=&'a Self>>(iter: I) -> Vec<$type> {
                iter.filter_map(|v| v.to_builtin()).collect()
            }
            pub fn to_builtin(&self) -> Option<$type> {
                match self {
                    $(Self::$opt => Some($type::$opt),)*
                    _ => None,
                }
            }
        }
        paste! {
            impl std::str::FromStr for $name {
                type Err = anyhow::Error;
                fn from_str(fullopt: &str) -> Result<Self, Self::Err> {
                    let mut splitter = fullopt.splitn(2, '=');
                    let optname = splitter.next().unwrap_or("");
                    let optval = splitter.next();
                    let optval_present = optval.is_some();
                    let optval = optval.unwrap_or("");

                    let (parsed, optval_used) = match &optname as &str {
                        // "dirsync" => ( Self::DirSync, false),
                        // "direct_io" if "" != "directio" => ( Self::DirectIO, false),
                        // "blksize" => ( Self::BlkSize ( "0".parse::<u64>()? , false || (None as Option<u64>).is_none() ),
                        $( $($optname if "" != )? stringify!([<$opt:lower>]) => (Self::$opt, false), )*
                        $(
                            $($newoptname if "" != )? stringify!([<$newopt:lower>]) => (
                                Self::$newopt $(( optval.parse::<$optval>()?))? , false $( || (None as Option<$optval>).is_none() )?
                            ),
                        )*
                        _ => (Self::Unknown(fullopt.to_owned()), false),
                    };

                    if !optval_used && optval_present {
                        Err(anyhow::anyhow!("Option {} do not accept an argument", optname))
                    } else if optval_used && !optval_present {
                        Err(anyhow::anyhow!("Argument for {} is not supplied", optname))
                    } else {
                        Ok(parsed)
                    }
                }
            }
            impl<T> From<T> for $name
            where
                T: ToString
            {
                fn from(v: T) -> Self {
                    let fullopt = v.to_string();
                    match fullopt.parse::<Self>() {
                        Ok(v) => v,
                        Err(_) => Self::Unknown(v.to_string()),
                    }
                }
            }
            impl From<$name> for String {
                fn from(v: $name) -> Self {
                    Self::from(&v)
                }
            }
            impl From<&$name> for String {
                fn from(v: &$name) -> Self {
                    match v {
                        // MountOption::DirSync => ("dirsync", "").0.to_owned() ,
                        // MountOption::DirectIO => format!(concat!("{}"), ("direct_io", "directio", "").0 ),
                        // MountOption::BlkSize (v) => format!(concat!("{}", "={}", ""), ("blksize", "").0, v.to_owned() as u64 ),
                        $($name::$opt => ( $($optname,)? stringify!([<$opt:lower>]), "" ).0 .to_owned() , )*
                        $(
                            $name::$newopt $( ( define_options!(@ignore $optval v) ) )? =>
                                format!(
                                    concat!("{}" $(,"={}", define_options!(@ignore $optval) )? ),
                                    ( $($newoptname,)? stringify!([<$newopt:lower>]), "" ).0
                                    $( , v.to_owned() as $optval )?
                                ),
                        )*
                        $name::Unknown(v) => v.to_owned(),
                    }
                }
            }
        }
    };

    // internal rules
    {@ignore $id: tt } => { "" };
    {@ignore $id: tt $($replacement: tt),* } => { $($replacement),* };
}

define_options! { MountOption (FuseMountOption) {
    builtin Dev,
    builtin NoDev,
    builtin Suid,
    builtin NoSuid,
    builtin RO,
    builtin RW,
    builtin Exec,
    builtin NoExec,
    builtin DirSync,
    define "direct_io" DirectIO,
    define BlkSize(String),
    define MaxSize(String), // size of filesystem
    define Tls(String),
//    define "opt" OptionName(Display_Debug_Clone_PartialEq_FromStr_able)
}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_mount_options() {
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["direct_io", "nodev,exec"].iter().copied())
            ),
            "[DirectIO, NoDev, Exec]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["direct_io="].iter().copied())
            ),
            "[Unknown(\"direct_io=\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["direct_io=1"].iter().copied())
            ),
            "[Unknown(\"direct_io=1\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["direct_io=1=2"].iter().copied())
            ),
            "[Unknown(\"direct_io=1=2\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["undefined"].iter().copied())
            ),
            "[Unknown(\"undefined\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["undefined=foo"].iter().copied())
            ),
            "[Unknown(\"undefined=foo\")]"
        );
        assert_eq!(
            format!("{:?}", MountOption::to_vec(vec!["dev="].iter().copied())),
            "[Unknown(\"dev=\")]"
        );
        assert_eq!(
            format!("{:?}", MountOption::to_vec(vec!["dev=1"].iter().copied())),
            "[Unknown(\"dev=1\")]"
        );
        assert_eq!(
            format!("{:?}", MountOption::to_vec(vec!["blksize"].iter().copied())),
            "[Unknown(\"blksize\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["blksize="].iter().copied())
            ),
            "[Unknown(\"blksize=\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["blksize=32"].iter().copied())
            ),
            "[BlkSize(32)]"
        );
        assert_eq!(
            format!("{:?}", MountOption::to_vec(vec!["tls"].iter().copied())),
            "[Unknown(\"tls\")]"
        );
        assert_eq!(
            format!("{:?}", MountOption::to_vec(vec!["tls="].iter().copied())),
            "[Tls(\"\")]"
        );
        assert_eq!(
            format!("{:?}", MountOption::to_vec(vec!["tls=xx"].iter().copied())),
            "[Tls(\"xx\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["tls=/root"].iter().copied())
            ),
            "[Tls(\"/root\")]"
        );
        assert_eq!(
            format!(
                "{:?}",
                MountOption::to_vec(vec!["direct_io", "nodev,blksize=32"].iter().copied())
            ),
            "[DirectIO, NoDev, BlkSize(32)]"
        );
    }

    #[test]
    fn convert_mount_options() {
        assert_eq!(
            MountOption::NoDev.to_builtin(),
            Some(FuseMountOption::NoDev)
        );
        assert_eq!(
            MountOption::DirSync.to_builtin(),
            Some(FuseMountOption::DirSync)
        );
        assert_eq!(MountOption::DirectIO.to_builtin(), None);
        assert_eq!(MountOption::BlkSize("1".to_owned()).to_builtin(), None);
        assert_eq!(MountOption::MaxSize("1".to_owned()).to_builtin(), None);
    }

    #[test]
    fn format_mount_options() {
        assert_eq!(String::from(MountOption::NoDev), "nodev");
        assert_eq!(String::from(MountOption::DirectIO), "direct_io");
        assert_eq!(
            String::from(MountOption::BlkSize("123".to_owned())),
            "blksize=123"
        );
        assert_eq!(
            String::from(MountOption::BlkSize("1MiB".to_owned())),
            "blksize=1MiB"
        );
    }
}

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
        FuseMountOption::DefaultPermissions,
    ];

    #[cfg(target_os = "linux")]
    fuse_options.push(FuseMountOption::AutoUnmount);

    fuse_options.extend(MountOption::collect_builtin(options.iter()));

    let tls_cfg_path = options
        .iter()
        .find_map(|opt| {
            if let MountOption::Tls(path) = opt {
                Some(path.parse().map_err(Into::into))
            } else {
                None
            }
        })
        .unwrap_or_else(default_tls_config_path)?;

    let client_cfg = if metadata(&tls_cfg_path).await.is_ok() {
        let client_cfg_contents = read_to_string(tls_cfg_path).await?;
        toml::from_str::<TlsConfig>(&client_cfg_contents)?.into()
    } else {
        Default::default()
    };

    debug!("use tikv client config: {:?}", client_cfg);
    let fs_impl = TiFs::construct(endpoints, client_cfg, options).await?;

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
