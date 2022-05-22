#!/usr/bin/env bash

set -e

DIR=`dirname $0`
ROOT=`dirname $DIR`

VERSION=`$DIR/../scripts/version.sh`
RELEASE_URL="https://github.com/pvolok/mprocs/releases/download/v$VERSION/mprocs-$VERSION-win64.zip"

cat $ROOT/scoop/mprocs.json | jq ".version = \"$VERSION\"" | sponge $ROOT/scoop/mprocs.json
cat $ROOT/scoop/mprocs.json | jq ".url = \"$RELEASE_URL\"" | sponge $ROOT/scoop/mprocs.json

SHA256=`curl -LJ0s $RELEASE_URL | shasum -a 256 | cut -f 1 -d " "`
cat $ROOT/scoop/mprocs.json | jq ".hash = \"$SHA256\"" | sponge $ROOT/scoop/mprocs.json
