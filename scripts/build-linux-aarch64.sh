#!/usr/bin/env sh

set -e

mkdir -p release

podman run -it --workdir /app \
  --platform linux/arm64 \
  --env ARCH=aarch64 \
  -v $(pwd)/scripts:/app/scripts \
  -v $(pwd)/Cargo.lock:/app/Cargo.lock \
  -v $(pwd)/Cargo.toml:/app/Cargo.toml \
  -v $(pwd)/src:/app/src \
  -v $(pwd)/helpers:/app/helpers \
  -v $(pwd)/scripts:/app/scripts \
  -v $(pwd)/release:/app/release \
  rust:1.87.0-alpine3.21 \
  scripts/_build-linux.sh
