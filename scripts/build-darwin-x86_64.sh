#!/usr/bin/env bash

set -e

DIR=`dirname $0`

export ARCH="x86_64"

bash $DIR/build-unix.sh
