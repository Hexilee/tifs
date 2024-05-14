use std::fmt::Debug;
use std::io::{stdin, stdout, BufRead, BufReader, Write};

use anyhow::{anyhow, Result};
use clap::{crate_version, App, Arg};
use tifs::fs::inode::Inode;
use tifs::fs::key::{ScopedKey, ROOT_INODE};
use tifs::fs::tikv_fs::TiFs;
use tifs::fs::transaction::Txn;
use tikv_client::TransactionClient;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("TiFS Debugger")
        .version(crate_version!())
        .author("Hexi Lee")
        .arg(
            Arg::with_name("pd")
                .long("pd-endpoints")
                .multiple(true)
                .value_name("ENDPOINTS")
                .default_value("127.0.0.1:2379")
                .help("set all pd endpoints of the tikv cluster")
                .takes_value(true),
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

    let console = Console::construct(endpoints).await?;

    loop {
        match console.interact().await {
            Ok(true) => break Ok(()),
            Err(err) => eprintln!("{}", err),
            _ => continue,
        }
    }
}

struct Console {
    pd_endpoints: Vec<String>,
    client: TransactionClient,
}

impl Console {
    async fn construct<S>(pd_endpoints: Vec<S>) -> Result<Self>
    where
        S: Clone + Debug + Into<String>,
    {
        let client =
            TransactionClient::new_with_config(pd_endpoints.clone(), Default::default())
                .await
                .map_err(|err| anyhow!("{}", err))?;
        Ok(Self {
            client,
            pd_endpoints: pd_endpoints.into_iter().map(Into::into).collect(),
        })
    }

    async fn interact(&self) -> Result<bool> {
        let mut txn = Txn::begin_optimistic(
            &self.client,
            TiFs::DEFAULT_BLOCK_SIZE,
            None,
            TiFs::MAX_NAME_LEN,
        )
        .await?;
        match self.interact_with_txn(&mut txn).await {
            Ok(exit) => {
                txn.commit().await?;
                Ok(exit)
            }
            Err(err) => {
                txn.rollback().await?;
                Err(err)
            }
        }
    }

    async fn interact_with_txn(&self, txn: &mut Txn) -> Result<bool> {
        print!("{:?}> ", &self.pd_endpoints);
        stdout().flush()?;

        let mut buffer = String::new();
        BufReader::new(stdin()).read_line(&mut buffer)?;
        let commands: Vec<&str> = buffer.split(' ').map(|seg| seg.trim()).collect();
        if commands.is_empty() {
            return Ok(false);
        }

        match commands[0] {
            "exit" => return Ok(true),
            "reset" => self.reset(txn).await?,
            "get" => self.get_block(txn, &commands[1..]).await?,
            "get_str" => self.get_block_str(txn, &commands[1..]).await?,
            "get_attr" => self.get_attr(txn, &commands[1..]).await?,
            "get_raw" => self.get_attr_raw(txn, &commands[1..]).await?,
            "get_inline" => self.get_inline(txn, &commands[1..]).await?,
            "rm" => self.delete_block(txn, &commands[1..]).await?,
            cmd => return Err(anyhow!("unknow command `{}`", cmd)),
        }

        Ok(false)
    }

    async fn reset(&self, txn: &mut Txn) -> Result<()> {
        let next_inode = txn
            .read_meta()
            .await?
            .map(|meta| meta.inode_next)
            .unwrap_or(ROOT_INODE);
        for inode in txn
            .scan(
                ScopedKey::inode_range(ROOT_INODE..next_inode),
                (next_inode - ROOT_INODE) as u32,
            )
            .await?
            .map(|pair| Inode::deserialize(pair.value()))
        {
            let inode = inode?;
            txn.clear_data(inode.ino).await?;
            txn.remove_inode(inode.ino).await?;
        }
        txn.delete(ScopedKey::meta()).await?;
        Ok(())
    }

    async fn get_block(&self, txn: &mut Txn, args: &[&str]) -> Result<()> {
        if args.len() < 2 {
            return Err(anyhow!("invalid arguments `{:?}`", args));
        }
        match txn
            .get(ScopedKey::block(args[0].parse()?, args[1].parse()?))
            .await?
        {
            Some(value) => println!("{:?}", &value[args.get(2).unwrap_or(&"0").parse()?..]),
            None => println!("Not Found"),
        }
        Ok(())
    }

    async fn get_block_str(&self, txn: &mut Txn, args: &[&str]) -> Result<()> {
        if args.len() < 2 {
            return Err(anyhow!("invalid arguments `{:?}`", args));
        }
        match txn
            .get(ScopedKey::block(args[0].parse()?, args[1].parse()?))
            .await?
        {
            Some(value) => println!("{:?}", String::from_utf8_lossy(&value)),
            None => println!("Not Found"),
        }
        Ok(())
    }

    async fn get_attr(&self, txn: &mut Txn, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            return Err(anyhow!("invalid arguments `{:?}`", args));
        }
        match txn.get(ScopedKey::inode(args[0].parse()?)).await? {
            Some(value) => println!("{:?}", Inode::deserialize(&value)?),
            None => println!("Not Found"),
        }
        Ok(())
    }

    async fn get_attr_raw(&self, txn: &mut Txn, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            return Err(anyhow!("invalid arguments `{:?}`", args));
        }
        match txn.get(ScopedKey::inode(args[0].parse()?)).await? {
            Some(value) => println!("{}", &*String::from_utf8_lossy(&value)),
            None => println!("Not Found"),
        }
        Ok(())
    }

    async fn get_inline(&self, txn: &mut Txn, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            return Err(anyhow!("invalid arguments `{:?}`", args));
        }
        match txn.get(ScopedKey::inode(args[0].parse()?)).await? {
            Some(value) => {
                let inline = Inode::deserialize(&value)?
                    .inline_data
                    .unwrap_or_else(Vec::new);
                println!("{}", String::from_utf8_lossy(&inline));
            }
            None => println!("Not Found"),
        }
        Ok(())
    }

    async fn delete_block(&self, txn: &mut Txn, args: &[&str]) -> Result<()> {
        if args.len() < 2 {
            return Err(anyhow!("invalid arguments `{:?}`", args));
        }
        txn.delete(ScopedKey::block(args[0].parse()?, args[1].parse()?))
            .await?;
        Ok(())
    }
}
