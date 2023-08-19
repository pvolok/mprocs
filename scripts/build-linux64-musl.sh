#!/usr/bin/env bash

set -e

podman build . -t mprocs-build-linux64-musl -f Dockerfile.linux64
podman run --rm -v "$PWD/release":/app/release -it \
  --entrypoint scripts/build-linux64-musl-entrypoint.sh \
  mprocs-build-linux64-musl
