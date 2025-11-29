#[tokio::main]
async fn main() -> anyhow::Result<()> {
  lib::dekit::dekit_main().await
}
