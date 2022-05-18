#!/usr/bin/env bash

set -e

DIR=`dirname $0`

pushd $DIR/.. > /dev/null
cargo metadata --format-version=1 \
  | jq -r '.packages | map(select(.name == "mprocs").version)[0]'
popd > /dev/null
