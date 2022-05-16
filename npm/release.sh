#!/usr/bin/env bash

set -e

DIR=`dirname $0`

# Set version from Cargo.toml
VERSION=`$DIR/../scripts/version.sh`
cat $DIR/package.json | jq ".version = \"$VERSION\"" | sponge $DIR/package.json

# Remove previous artifact
rm -rf $DIR/mprocs-*

# Extract linux64
LINUX64_NAME=mprocs-$VERSION-linux64
mkdir -p $DIR/$LINUX64_NAME/
tar zxvf $DIR/../release/$LINUX64_NAME.tar.gz -C $DIR/$LINUX64_NAME/
# Extract macos64
MACOS64_NAME=mprocs-$VERSION-macos64
mkdir -p $DIR/$MACOS64_NAME/
tar zxvf $DIR/../release/$MACOS64_NAME.tar.gz -C $DIR/$MACOS64_NAME/
# Extract win64
WIN64_NAME=mprocs-$VERSION-win64
mkdir -p $DIR/$WIN64_NAME/
unzip -a $DIR/../release/$WIN64_NAME.zip -d $DIR/$WIN64_NAME/

# npm publish
pushd $DIR
npm publish
popd
