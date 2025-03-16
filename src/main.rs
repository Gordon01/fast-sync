mod api;
mod wft_proto;

use std::{
    fs::File,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::Parser;
use tracing::{debug, info};
use wft_proto::Wft;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long, short)]
    ip: IpAddr,
    #[arg(long, short, default_value_t = 1234)]
    port: u16,
    #[arg(long, short, default_value = "/DCIM/Camera")]
    directory: String,
    #[arg(long, short)]
    output: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let cli = Cli::parse();
    debug!("CL arguments: {cli:#?}");
    let wft = Wft::new(SocketAddr::new(cli.ip, cli.port));
    let pictures = wft.directory(&cli.directory).await?;

    let mut out_dir = PathBuf::from(cli.output.unwrap_or("out".to_string()));
    out_dir.extend(cli.directory.split("/"));
    std::fs::create_dir_all(&out_dir)?;
    for file in pictures.files {
        let mut out_file = out_dir.clone();
        out_file.push(&file.name);
        let out_path = format!("{}/{}", cli.directory.trim_end_matches("/"), file.name);
        info!("out path: {out_path}, file {out_file:?}");
        let bytes = wft.download_file(out_path).await?;
        std::fs::write(out_file, bytes).context("create output file")?;
    }

    Ok(())
}
