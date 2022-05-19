#!/usr/bin/env bash

set -e

DIR=`dirname $0`

gh-md-toc --insert --no-backup --skip-header README.md
prettier -w README.md
