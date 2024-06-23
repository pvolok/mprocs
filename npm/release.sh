#!/usr/bin/env bash

set -e

DIR=`dirname $0`

# Set version from Cargo.toml
VERSION=`$DIR/../scripts/version.sh`
cat $DIR/package.json | jq ".version = \"$VERSION\"" | sponge $DIR/package.json

# Remove previous artifact
rm -rf $DIR/mprocs-*

# linux x64
NAME=mprocs-$VERSION-linux-x86_64-musl
mkdir -p $DIR/$NAME/
tar zxvf $DIR/../release/$NAME.tar.gz -C $DIR/$NAME/
# linux arm64
NAME=mprocs-$VERSION-linux-aarch64-musl
mkdir -p $DIR/$NAME/
tar zxvf $DIR/../release/$NAME.tar.gz -C $DIR/$NAME/
# macos x64
NAME=mprocs-$VERSION-darwin-x86_64
mkdir -p $DIR/$NAME/
tar zxvf $DIR/../release/$NAME.tar.gz -C $DIR/$NAME/
# macos arm64
NAME=mprocs-$VERSION-darwin-aarch64
mkdir -p $DIR/$NAME/
tar zxvf $DIR/../release/$NAME.tar.gz -C $DIR/$NAME/
# windows x64
NAME=mprocs-$VERSION-windows-x86_64
mkdir -p $DIR/$NAME/
unzip -a $DIR/../release/$NAME.zip -d $DIR/$NAME/

# npm publish
pushd $DIR
npm publish
popd
