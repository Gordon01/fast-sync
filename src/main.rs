mod api;
mod wft_proto;

use std::net::{IpAddr, SocketAddr};

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
    let pictures = wft.directory(cli.directory).await?;
    //dbg!(&pictures);

    Ok(())
}
