#!/usr/bin/env bash

set -e

DIR=`dirname $0`

VERSION=`$DIR/version.sh`

cargo build --release

mkdir -p release/mprocs-$VERSION-linux64
cp target/release/mprocs release/mprocs-$VERSION-linux64/mprocs

upx --brute release/mprocs-$VERSION-linux64/mprocs

tar -czvf release/mprocs-$VERSION-linux64.tar.gz \
  -C release/mprocs-$VERSION-linux64 \
  mprocs
