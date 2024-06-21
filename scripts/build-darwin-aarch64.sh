#!/usr/bin/env bash

set -e

DIR=`dirname $0`

export ARCH="aarch64"

bash $DIR/build-unix.sh
