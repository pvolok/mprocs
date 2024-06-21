#!/usr/bin/env sh

set -e

apk add --no-cache musl-dev bash jq make
apk add --no-cache -X http://dl-cdn.alpinelinux.org/alpine/edge/community upx

DIR=`dirname $0`

bash $DIR/build-unix.sh
