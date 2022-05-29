# mprocs

_mprocs_ runs multiple commands in parallel and shows output of each command
separately.

When you work on a project you very often need the same list of commands to be
running. For example: `webpack serve`, `jest --watch`, `node src/server.js`.
With mprocs you can list these command in `mprocs.yaml` and run all of them by
running `mprocs`. Then you can switch between outputs of running commands and
interact with them.

It is simmilar to
[concurrently](https://github.com/open-cli-tools/concurrently) but _mprocs_
shows output of each command separately and allows to interact with processes
(you can even work in _vim_ inside _mprocs_).

<!--ts-->

- [Screenshots](#screenshots)
- [Installation](#installation)
  - [Download binary (Linux, Macos, Windows)](#download-binary-linux-macos-windows)
  - [npm (Linux, Macos, Windows)](#npm-linux-macos-windows)
  - [homebrew (Macos)](#homebrew-macos)
  - [cargo (All platforms)](#cargo-all-platforms)
  - [scoop (Windows)](#scoop-windows)
  - [AUR (Arch Linux)](#aur-arch-linux)
- [Usage](#usage)
  - [Config](#config)
    - [$select operator](#select-operator)
  - [Key bindings](#key-bindings)
  - [Remote control](#remote-control)

<!-- Created by https://github.com/ekalinin/github-markdown-toc -->
<!-- Added by: pvolok, at: Mon May 30 00:07:24 +07 2022 -->

<!--te-->

## Screenshots

<img src="img/screenshot1.png" width="889" height="564" />
<img src="img/screenshot2.png" width="889" height="564" />

## Installation

[![Packaging status](https://repology.org/badge/vertical-allrepos/mprocs.svg)](https://repology.org/project/mprocs/versions)

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

### AUR (Arch Linux)

```sh
yay mprocs
```

```sh
yay mprocs-bin
```

## Usage

1. Run `mprocs cmd1 cmd2 …` (example: `mprocs "yarn test -w" "webpack serve"`)

OR

1. Create `mprocs.yaml` file
2. Run `mprocs` command

Example `mprocs.yaml`:

```yaml
procs:
  nvim:
    cmd: ["nvim"]
  server:
    shell: "nodemon server.js"
  webpack: "webpack serve"
  tests:
    shell: "jest -w"
    env:
      NODE_ENV: test
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

#### `$select` operator

You can define different values depending on the current operating system.
To provide different values based on current OS define an object with:

- First field `$select: os`
- Fields defining values for different OSes: `macos: value`. Possible
  values are listed here:
  https://doc.rust-lang.org/std/env/consts/constant.OS.html.
- Field `$else: default value` will be matched if no value was defined for
  current OS. If current OS is not matched and field `$else` is missing, then
  mprocs will fail to load config.

Example `mprocs.yaml`:

```yaml
procs:
  my processs:
    shell:
      $select: os
      windows: "echo %TEXT%"
      $else: "echo $TEXT"
    env:
      TEXT:
        $select: os
        windows: Windows
        linux: Linux
        macos: Macos
        freebsd: FreeBSD
```

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
- `a` - Add new process
- `d` - Remove selected process (process must be stopped first)
- `k` or `↑` - Select previous process
- `j` or `↓` - Select next process
- `C-d` or `page down` - Scroll output down
- `C-u` or `page up` - Scroll output up

Process output focused:

- `C-a` - Focus processes pane

### Remote control

Optionally, _mprocs_ can listen on TCP port for remote commands.
You have to define remote control server address in `mprocs.yaml`
(`server: 127.0.0.1:4050`) or via cli argument (`mprocs --server 127.0.0.1:4050`). To send a command to running _mprocs_ instance
use the **ctl** argument: `mprocs --ctl '{c: quit}'` or `mprocs --ctl '{c: send-key, key: <C-c>}'`.

Commands are encoded as yaml. Available commands:

- `{c: quit}`
- `{c: force-quit}`
- `{c: toggle-scope}` - Toggle focus between process list and terminal.
- `{c: next-proc}`
- `{c: prev-proc}`
- `{c: start-proc}`
- `{c: term-proc}`
- `{c: kill-proc}`
- `{c: restart-proc}`
- `{c: force-restart-proc}`
- `{c: show-add-proc}`
- `{c: add-proc, cmd: "<SHELL COMMAND>"}`
- `{c: show-remove-proc}`
- `{c: remove-proc, id: "<PROCESS ID>"}`
- `{c: scrol-down}`
- `{c: scroll-up}`
- `{c: send-key, key: "<KEY>"}` - Send key to current process. Key
  examples: `<C-a>`, `<Enter>`
