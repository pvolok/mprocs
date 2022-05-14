"use strict";

var VERSION = require("./package.json").version;

var path = require("path");

function getBinPath() {
  if (process.platform === "darwin") {
    return path.join(__dirname, `mprocs-${VERSION}-macos64/mprocs`);
  }
  if (process.platform === "linux" && process.arch === "x64") {
    return path.join(__dirname, `mprocs-${VERSION}-linux64/mprocs`);
  }
  if (process.platform === "win32" && process.arch === "x64") {
    return path.join(__dirname, `mprocs-${VERSION}-win64/mprocs.exe`);
  }

  return null;
}

module.exports = getBinPath();
