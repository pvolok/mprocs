#!/usr/bin/env sh

set -e

mkdir -p release

podman run -it --workdir /app \
  --platform linux/amd64 \
  --env ARCH=x86_64 \
  -v $(pwd)/scripts:/app/scripts:Z \
  -v $(pwd)/Cargo.lock:/app/Cargo.lock:Z \
  -v $(pwd)/Cargo.toml:/app/Cargo.toml:Z \
  -v $(pwd)/vendor:/app/vendor:Z \
  -v $(pwd)/src:/app/src:Z \
  -v $(pwd)/helpers:/app/helpers:Z \
  -v $(pwd)/scripts:/app/scripts:Z \
  -v $(pwd)/release:/app/release:Z \
  rust:1.78.0-alpine3.20 \
  scripts/_build-linux.sh
