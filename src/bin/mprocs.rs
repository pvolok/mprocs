#[tokio::main]
async fn main() -> anyhow::Result<()> {
  lib::mprocs::mprocs_main().await
}
