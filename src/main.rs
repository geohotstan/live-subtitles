mod app;
mod audio;
mod config;
mod macos_capture;
mod streaming;
mod transcribe;
mod ui;

use crate::app::run;
use crate::config::Cli;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,subtitles=info".into()),
        )
        .init();

    let cli = <Cli as clap::Parser>::parse();
    run(cli)
}
