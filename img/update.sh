#!/usr/bin/env bash

set -e

cargo build

virterm img/update.lua

convert img/screenshot1.png -density 144 -units PixelsPerInch img/screenshot1.png
convert img/screenshot2.png -density 144 -units PixelsPerInch img/screenshot2.png
