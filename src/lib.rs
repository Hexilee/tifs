#![feature(map_into_keys_values)]
#![feature(async_closure)]
#![feature(array_chunks)]

pub mod fs;

use fs::async_fs::AsyncFs;
use fs::tikv_fs::TiFs;

use fuser::MountOption as FuseMountOption;
use paste::paste;

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
    define BlkSize(u64),
//    define "opt" OptionName(Display_Debug_Clone_PartialEq_FromStr_able)
}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_mount_options() {
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["direct_io", "nodev,exec"].iter().map(|v| v.clone()))), "[DirectIO, NoDev, Exec]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["direct_io="].iter().map(|v| v.clone()))), "[Unknown(\"direct_io=\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["direct_io=1"].iter().map(|v| v.clone()))), "[Unknown(\"direct_io=1\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["direct_io=1=2"].iter().map(|v| v.clone()))), "[Unknown(\"direct_io=1=2\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["undefined"].iter().map(|v| v.clone()))), "[Unknown(\"undefined\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["undefined=foo"].iter().map(|v| v.clone()))), "[Unknown(\"undefined=foo\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["dev="].iter().map(|v| v.clone()))), "[Unknown(\"dev=\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["dev=1"].iter().map(|v| v.clone()))), "[Unknown(\"dev=1\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["blksize"].iter().map(|v| v.clone()))), "[Unknown(\"blksize\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["blksize="].iter().map(|v| v.clone()))), "[Unknown(\"blksize=\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["blksize=xx"].iter().map(|v| v.clone()))), "[Unknown(\"blksize=xx\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["blksize=32=1"].iter().map(|v| v.clone()))), "[Unknown(\"blksize=32=1\")]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["blksize=32"].iter().map(|v| v.clone()))), "[BlkSize(32)]");
        assert_eq!(format!("{:?}", MountOption::to_vec(vec!["direct_io", "nodev,blksize=32"].iter().map(|v| v.clone()))), "[DirectIO, NoDev, BlkSize(32)]");
    }

    #[test]
    fn convert_mount_options() {
        assert_eq!(MountOption::NoDev.into_builtin(), Some(FuseMountOption::NoDev));
        assert_eq!(MountOption::DirSync.into_builtin(), Some(FuseMountOption::DirSync));
        assert_eq!(MountOption::DirectIO.into_builtin(), None);
        assert_eq!(MountOption::BlkSize (123) .into_builtin(), None);
    }

    #[test]
    fn format_mount_options() {
        assert_eq!(String::from(MountOption::NoDev), "nodev");
        assert_eq!(String::from(MountOption::DirectIO), "direct_io");
        assert_eq!(String::from(MountOption::BlkSize (123)), "blksize=123");
        assert_eq!(String::from(MountOption::BlkSize (0)), "blksize=0");
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
