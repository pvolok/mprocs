#!/usr/bin/env bash

set -e

DIR=`dirname $0`

VERSION=`$DIR/scripts/version.sh`

rm -rf release

# Linux 64

scripts/build-linux64-musl.sh

# Aarch64

# mkdir -p release/mprocs-$VERSION-aarch64
#
# TARGET_CC=aarch64-none-elf-gcc \
# RUSTFLAGS="-C linker=aarch64-none-elf-gcc" \
# cargo build --release --target=aarch64-unknown-linux-musl
#
# cp target/aarch64-unknown-linux-musl/release/mprocs \
#   release/mprocs-$VERSION-aarch64/mprocs
#
# tar -czvf release/mprocs-$VERSION-aarch64.tar.gz \
#   -C release/mprocs-$VERSION-aarch64 \
#   mprocs

# Macos

mkdir -p release/mprocs-$VERSION-macos64

cargo build --release --target=x86_64-apple-darwin

cp target/x86_64-apple-darwin/release/mprocs \
  release/mprocs-$VERSION-macos64/mprocs

upx --brute release/mprocs-$VERSION-macos64/mprocs

tar -czvf release/mprocs-$VERSION-macos64.tar.gz \
  -C release/mprocs-$VERSION-macos64 \
  mprocs
