mod wft;

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
use wft::{Directory, Wft};

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
    let directory = wft.directory(&cli.directory).await?;

    process_files(&cli, &directory, &wft).await
}

#[derive(Debug)]
enum FileStatus {
    Missing,
    UpToDate,
    Outdated,
}

async fn process_files(cli: &Cli, directory: &Directory, wft: &Wft) -> anyhow::Result<()> {
    let out_dir = {
        let mut dir = PathBuf::from(cli.output.as_deref().unwrap_or("out"));
        dir.extend(cli.directory.split('/'));
        dir
    };
    std::fs::create_dir_all(&out_dir)?;

    let start = Instant::now();
    let mut done = 0;
    let size: u64 = directory.files.iter().map(|e| e.size).sum();

    for file in &directory.files {
        let out_file = out_dir.join(&file.name);

        let status = file_status(&out_file, file.size, file.modified);
        debug!("got {} with {status:?}", out_file.display());
        match status {
            FileStatus::UpToDate => continue,
            FileStatus::Outdated | FileStatus::Missing => {
                let out_path = format!("{}/{}", cli.directory.trim_end_matches('/'), file.name);
                let bytes = wft.download_file(&out_path).await?;
                std::fs::write(&out_file, &bytes)
                    .with_context(|| format!("Failed to write output file {:?}", out_file))?;
                done += bytes.len();
            }
        }

        let elapsed_secs = start.elapsed().as_secs() + 1; // Avoid division by zero
        let speed = done as u64 / elapsed_secs;
        info!(
            "Bytes left: {}, {}/s, done: {}",
            Byte::from(size - done as u64),
            Byte::from(speed),
            out_file.display()
        );
    }

    Ok(())
}

fn file_status(out_file: &PathBuf, remote_size: u64, _remote_mtime: u64) -> FileStatus {
    match std::fs::metadata(out_file) {
        Ok(metadata) => {
            let local_size = metadata.len();
            if local_size == remote_size {
                FileStatus::UpToDate
            } else {
                FileStatus::Outdated
            }
        }
        Err(_) => FileStatus::Missing,
    }
}
