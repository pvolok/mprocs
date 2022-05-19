#!/usr/bin/env bash

set -e

DIR=`dirname $0`

VERSION=`$DIR/scripts/version.sh`

rm -rf release

# Linux 64

mkdir -p release/mprocs-$VERSION-linux64

TARGET_CC=x86_64-linux-musl-gcc \
RUSTFLAGS="-C linker=x86_64-linux-musl-gcc" \
cargo build --release --target=x86_64-unknown-linux-musl

cp target/x86_64-unknown-linux-musl/release/mprocs \
  release/mprocs-$VERSION-linux64/mprocs

upx-head --brute release/mprocs-$VERSION-linux64/mprocs

tar -czvf release/mprocs-$VERSION-linux64.tar.gz \
  -C release/mprocs-$VERSION-linux64 \
  mprocs

# Macos

mkdir -p release/mprocs-$VERSION-macos64

cargo build --release --target=x86_64-apple-darwin

cp target/x86_64-apple-darwin/release/mprocs \
  release/mprocs-$VERSION-macos64/mprocs

upx --brute release/mprocs-$VERSION-macos64/mprocs

tar -czvf release/mprocs-$VERSION-macos64.tar.gz \
  -C release/mprocs-$VERSION-macos64 \
  mprocs
