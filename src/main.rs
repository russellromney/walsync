mod ltx;
mod sync;
mod s3;
mod wal;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "walsync")]
#[command(about = "Lightweight SQLite WAL sync to S3/Tigris")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Watch SQLite databases and sync WAL changes to S3
    Watch {
        /// Database files to watch
        #[arg(required = true)]
        databases: Vec<PathBuf>,

        /// S3 bucket (e.g., "s3://my-bucket/prefix")
        #[arg(short, long)]
        bucket: String,

        /// Snapshot interval in seconds (default: 3600 = 1 hour)
        #[arg(long, default_value = "3600")]
        snapshot_interval: u64,

        /// S3 endpoint URL (for Tigris/MinIO/etc)
        #[arg(long, env = "AWS_ENDPOINT_URL_S3")]
        endpoint: Option<String>,
    },

    /// Restore a database from S3
    Restore {
        /// Database name (as registered in S3)
        name: String,

        /// Output path for restored database
        #[arg(short, long)]
        output: PathBuf,

        /// S3 bucket
        #[arg(short, long)]
        bucket: String,

        /// S3 endpoint URL
        #[arg(long, env = "AWS_ENDPOINT_URL_S3")]
        endpoint: Option<String>,

        /// Restore to specific point in time (ISO 8601)
        #[arg(long)]
        point_in_time: Option<String>,
    },

    /// List databases in S3 bucket
    List {
        /// S3 bucket
        #[arg(short, long)]
        bucket: String,

        /// S3 endpoint URL
        #[arg(long, env = "AWS_ENDPOINT_URL_S3")]
        endpoint: Option<String>,
    },

    /// Take an immediate snapshot
    Snapshot {
        /// Database file
        database: PathBuf,

        /// S3 bucket
        #[arg(short, long)]
        bucket: String,

        /// S3 endpoint URL
        #[arg(long, env = "AWS_ENDPOINT_URL_S3")]
        endpoint: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "walsync=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Watch {
            databases,
            bucket,
            snapshot_interval,
            endpoint,
        } => {
            sync::watch(databases, &bucket, snapshot_interval, endpoint.as_deref()).await?;
        }
        Commands::Restore {
            name,
            output,
            bucket,
            endpoint,
            point_in_time,
        } => {
            sync::restore(&name, &output, &bucket, endpoint.as_deref(), point_in_time.as_deref()).await?;
        }
        Commands::List { bucket, endpoint } => {
            sync::list(&bucket, endpoint.as_deref()).await?;
        }
        Commands::Snapshot {
            database,
            bucket,
            endpoint,
        } => {
            sync::snapshot(&database, &bucket, endpoint.as_deref()).await?;
        }
    }

    Ok(())
}
