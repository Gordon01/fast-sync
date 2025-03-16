mod api;
mod wft_proto;

use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Instant,
};

use anyhow::Context;
use clap::Parser;
use readable::byte::Byte;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;
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
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    debug!("CL arguments: {cli:#?}");
    let wft = Wft::new(SocketAddr::new(cli.ip, cli.port));
    let (size, pictures) = wft.directory(&cli.directory).await?;

    let mut out_dir = PathBuf::from(cli.output.unwrap_or("out".to_string()));
    out_dir.extend(cli.directory.split("/"));
    std::fs::create_dir_all(&out_dir)?;
    let start = Instant::now();
    let mut done = 0;
    for file in pictures.files {
        let mut out_file = out_dir.clone();
        out_file.push(&file.name);
        let out_path = format!("{}/{}", cli.directory.trim_end_matches("/"), file.name);
        let bytes = wft.download_file(&out_path).await?;
        std::fs::write(out_file, &bytes).context("create output file")?;

        // Statistics
        done += bytes.len();
        let speed = done as u64 / (start.elapsed().as_secs() + 1);
        info!(
            "Bytes left: {}, {}/s, done: {out_path}",
            Byte::from(size - done as u64),
            Byte::from(speed)
        );
    }

    Ok(())
}
