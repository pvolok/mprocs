#!/usr/bin/env bash

set -e

DIR=`dirname $0`

VERSION=`$DIR/version.sh`

#
# Define ARCH
#
# if [ -z "${!ARCH}" ]; then
if [ -z "$(eval echo \$ARCH)" ]; then
  echo "Error: ARCH is not defined or is empty."
  exit 1
fi

#
# Define OS_TYPE
#
if [[ "$(uname)" == "Darwin" ]]; then
  OS_TYPE="darwin"
elif [[ "$(uname)" == "Linux" ]]; then
  OS_TYPE="linux"
else
  echo "Error: Unsupported operating system."
  exit 1
fi

#
# Define TRIPLE and OS_ARCH
#
case "$OS_TYPE" in
  linux)
    TRIPLE="$ARCH-unknown-linux-musl"
    OS_ARCH="linux-$ARCH-musl"
    ;;
  darwin)
    TRIPLE="$ARCH-apple-darwin"
    OS_ARCH="darwin-$ARCH"
    ;;
  *)
    echo "Error: Unsupported OS_TYPE ($OS_TYPE)."
    exit 1
    ;;
esac

mkdir -p release/mprocs-$VERSION-$OS_ARCH

cargo build -p mprocs --release --target=$TRIPLE

cp target/$TRIPLE/release/mprocs release/mprocs-$VERSION-$OS_ARCH/mprocs

upx --brute release/mprocs-$VERSION-$OS_ARCH/mprocs

tar -czvf release/mprocs-$VERSION-$OS_ARCH.tar.gz \
  -C release/mprocs-$VERSION-$OS_ARCH \
  mprocs
