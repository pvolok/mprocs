#!/usr/bin/env bash

set -e

docker build . -t mprocs-build-linux64-musl -f Dockerfile.linux64
docker run --rm -v "$PWD/release":/app/release -it \
  --entrypoint scripts/build-linux64-musl-entrypoint.sh \
  mprocs-build-linux64-musl
