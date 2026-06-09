#[tokio::main]
async fn main() {
  if let Err(err) = lib::dekit::main::dekit_main().await {
    eprintln!("Error: {}", err);
    std::process::exit(1);
  }
}
