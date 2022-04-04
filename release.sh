set -e

VERSION=0.1.0

rm -rf release
mkdir -p release

# Linux 64

TARGET_CC=x86_64-linux-musl-gcc \
RUSTFLAGS="-C linker=x86_64-linux-musl-gcc" \
cargo build --release --target=x86_64-unknown-linux-musl

cp target/x86_64-unknown-linux-musl/release/mprocs \
  release/mprocs-$VERSION-linux64

upx-head --brute release/mprocs-$VERSION-linux64

# Macos

cargo build --release --target=x86_64-apple-darwin

cp target/x86_64-apple-darwin/release/mprocs \
  release/mprocs-$VERSION-macos64

upx --brute release/mprocs-$VERSION-macos64
