use subtitles::config::Cli;
use subtitles::run_headless;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,subtitles=info".into()),
        )
        .init();

    let cli = <Cli as clap::Parser>::parse();
    run_headless(cli)
}
