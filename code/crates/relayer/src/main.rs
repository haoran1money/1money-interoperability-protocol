use clap::Parser;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    tracing_subscriber::fmt::init();
    color_eyre::install()?;
    let cli = relayer::cli::Cli::parse();
    cli.run().await?;
    Ok(())
}
