"use strict";

var VERSION = require("./package.json").version;

var path = require("path");

function getBinPath() {
  if (process.platform === "darwin") {
    if (process.arch === "arm64") {
      return path.join(__dirname, `mprocs-${VERSION}-darwin-aarch64/mprocs`);
    } else {
      return path.join(__dirname, `mprocs-${VERSION}-darwin-x86_64/mprocs`);
    }
  }
  if (process.platform === "linux") {
    if (process.arch === "arm64") {
      return path.join(
        __dirname,
        `mprocs-${VERSION}-linux-aarch64-musl/mprocs`
      );
    } else {
      return path.join(__dirname, `mprocs-${VERSION}-linux-x86_64-musl/mprocs`);
    }
  }
  if (process.platform === "win32") {
    return path.join(__dirname, `mprocs-${VERSION}-windows-x86_64/mprocs.exe`);
  }

  const os = process.platform;
  const arch = process.arch;
  throw new Error(
    `Npm package of mprocs doesn't include binaries for ${os}-${arch}.`
  );
}

module.exports = getBinPath();
