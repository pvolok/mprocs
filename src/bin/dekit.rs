#[tokio::main]
async fn main() -> anyhow::Result<()> {
  lib::dekit::main::dekit_main().await
}
