#!/usr/bin/env bash

set -e

cargo metadata --format-version=1 \
  | jq -r '.packages | map(select(.name == "mprocs").version)[0]'
