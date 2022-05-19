# mprocs

_mprocs_ runs multiple commands in parallel and shows output of each command
separately.

When you work on a project you very often need the same list of commands to be
running. For example: `webpack serve`, `jest --watch`, `node src/server.js`.
With mprocs you can list these command in `mprocs.json` and run all of them by
running `mprocs`. Then you can switch between outputs of running commands and
interact with them.

It is simmilar to
[concurrently](https://github.com/open-cli-tools/concurrently) but _mprocs_
shows output of each command separately and allows to interact with processes
(you can even work in _vim_ inside _mprocs_).

<!--ts-->
* [mprocs](#mprocs)
   * [Screenshots](#screenshots)
   * [Installation](#installation)
      * [Download binary (Linux, Macos, Windows)](#download-binary-linux-macos-windows)
      * [npm (Linux, Macos, Windows)](#npm-linux-macos-windows)
      * [homebrew (Macos)](#homebrew-macos)
      * [cargo (All platforms)](#cargo-all-platforms)
      * [scoop (Windows)](#scoop-windows)
   * [Usage](#usage)
      * [Config](#config)
      * [Key bindings](#key-bindings)

<!-- Created by https://github.com/ekalinin/github-markdown-toc -->
<!-- Added by: pvolok, at: Fri May 20 00:30:56 +07 2022 -->

<!--te-->

## Screenshots

<img src="img/screenshot1.png" width="889" height="564" />
<img src="img/screenshot2.png" width="889" height="564" />

## Installation

### Download binary (Linux, Macos, Windows)

[Download](https://github.com/pvolok/mprocs/releases) executable for your
platform and put it into a directory included in PATH.

### npm (Linux, Macos, Windows)

```sh
npm install -g mprocs
```

```sh
yarn global add mprocs
```

### homebrew (Macos)

```sh
brew install pvolok/mprocs/mprocs
```

### cargo (All platforms)

```sh
cargo install mprocs
```

### scoop (Windows)

```sh
scoop install https://raw.githubusercontent.com/pvolok/mprocs/master/scoop.json
```

## Usage

1. Run `mprocs cmd1 cmd2 …` (example: `mprocs "yarn test -w" "webpack serve"`)

OR

1. Create `mprocs.json` file
2. Run `mprocs` command

Example `mprocs.json`:

```json
{
  "procs": {
    "nvim": {
      "cmd": ["nvim"]
    },
    "server": {
      "shell": "nodemon server.js"
    },
    "webpack": {
      "shell": "webpack serve"
    },
    "tests": {
      "shell": "jest -w",
      "env": {
        "NODE_ENV": "test"
      }
    }
  }
}
```

### Config

- **procs**: _object_ - Processes to run.
  - **shell**: _string_ - Shell command to run (only **shell** or **cmd** must
    be provided).
  - **cmd**: _array<string>_ - Array of command and args to run (only **shell**
    or **cmd** must be provided).
  - **env**: _object<string, string|null>_ - Set env variables. Object keys are
    variable names. Assign variable to null, to clear variables inherited from
    parent process.

### Key bindings

Process list focused:

- `q` - Quit (soft kill processes and wait then to exit)
- `Q` - Force quit (terminate processes)
- `C-a` - Focus output pane
- `x` - Soft kill selected process (send SIGTERM signal, hard kill on Windows)
- `X` - Hard kill selected process (send SIGKILL)
- `s` - Start selected process, if it is not running
- `r` - Soft kill selected process and restart it when it stops
- `R` - Hard kill selected process and restart it when it stops
- `k` or `↑` - Select previous process
- `j` or `↓` - Select next process
- `C-d` or `page down` - Scroll output down
- `C-u` or `page up` - Scroll output up

Process output focused:

- `C-a` - Focus processes pane
