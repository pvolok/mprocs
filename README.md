# mprocs

_mprocs_ runs multiple commands in parallel and shows output of each command
separately.

When you work on a project you very often need the same list of commands to be
running. For example: `webpack serve`, `jest --watch`, `node src/server.js`.
With mprocs you can list these command in `mprocs.yaml` and run all of them by
running `mprocs`. Then you can switch between outputs of running commands and
interact with them.

It is similar to
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
    - [Keymap](#keymap)
    - [$select operator](#select-operator)
  - [Default keymap](#default-keymap)
  - [Remote control](#remote-control)
- [FAQ](#faq)
  - [mprocs vs tmux/screen](#mprocs-vs-tmuxscreen)

<!-- Created by https://github.com/ekalinin/github-markdown-toc -->
<!-- Added by: pvolok, at: Mon Jun 13 00:09:23 +07 2022 -->

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
scoop install mprocs
```

### AUR (Arch Linux)

```sh
yay mprocs
```

```sh
yay mprocs-bin
```

### MPR (Debian/Ubuntu)

```sh
git clone 'https://mpr.makedeb.org/mprocs'
cd mprocs/
makedeb -si
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
  - **shell**: _string_ - Shell command to run (exactly one of **shell** or
    **cmd** must be provided).
  - **cmd**: _array<string>_ - Array of command and args to run (exactly one of
    **shell** or **cmd** must be provided).
  - **cwd**: _string_ - Set working directory for the process. Prefix
    `<CONFIG_DIR>` will be replaced with the path of the directory where the
    config is located.
  - **env**: _object<string, string|null>_ - Set env variables. Object keys are
    variable names. Assign variable to null, to clear variables inherited from
    parent process.
  - **add_path**: _string|array<string>_ - Add entries to the _PATH_
    environment variable.
  - **autostart**: _bool_ - Start process when mprocs starts. Default: _true_.
  - **stop**: _"SIGINT"|"SIGTERM"|"SIGKILL"|{send-keys:
    array<key>}|"hard-kill"_ -
    A way to stop a process (using `x` key or when quitting mprocs).

#### Keymap

Key bindings can be overridden globally in file `~/.config/mprocs/mprocs.yaml`
or in a local config (`mprocs.yaml` in current directory by default). Keymaps are separate for process list and terminal windows.

There are three keymap levels:

- Default keymaps
- `~/.config/mprocs/mprocs.yaml`
- `./mprocs.yaml` (can be overridden by the _-c/--config_ cli arg)

Lower levers override bindings from previous levels. Key bindings from previous
levels can be cleared by specifying `reset: true` field at the same level as
keys:

```yaml
keymap_procs: # keymap when process list is focused
  <C-q>: { c: toggle-focus }
  <C-a>: null # unbind key
keymap_term: # keymap when terminal is focused
  reset: true
  <C-q>: { c: toggle-focus }
  <C-j>:
    c: batch
    cmds:
      - { c: focus-procs }
      - { c: next-proc }
```

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

#### Running scripts from package.json

If you run _mprocs_ with an `--npm` argument, it will load scripts from
`package.json`. But the scripts are not run by default, and you can launch
desired scripts manually.

```sh
# Run mprocs with scripts from package.json
mprocs --npm
```

### Default keymap

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
- `M-1` - `M-8` - Select process 1-8
- `C-d` or `page down` - Scroll output down
- `C-u` or `page up` - Scroll output up
- `C-e` - Scroll output down by 3 lines
- `C-y` - Scroll output up by 3 lines
- `z` - Zoom into terminal window

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
- `{c: toggle-focus}` - Toggle focus between process list and terminal.
- `{c: focus-procs}` - Focus process list
- `{c: focus-term}` - Focus process terminal window
- `{c: zoom}` - Zoom into terminal window
- `{c: next-proc}`
- `{c: prev-proc}`
- `{c: select-proc, index: <PROCESS INDEX>}` - Select process by index
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
- `{c: scrol-down-lines, n: <COUNT>}`
- `{c: scroll-up-lines, n: <COUNT>}`
- `{c: send-key, key: "<KEY>"}` - Send key to current process. Key
  examples: `<C-a>`, `<Enter>`
- `{c: batch, cmds: [{c: focus-procs}, …]}` - Send multiple commands

## FAQ

### mprocs vs tmux/screen

_mprocs_ is meant to make it easier to run specific commands that you end up
running repeatedly, such as compilers and test runners. This is in contrast
with _tmux_, which is usually used to run much more long-lived processes -
usually a shell - in each window/pane. Another difference is that _tmux_ runs a
server and a client, which allows the client to detach and reattach later,
keeping the processes running. _mprocs_ is meant more for finite lifetime
processes that you keep re-running, but when _mprocs_ ends, so do the processes
it is running within its windows.
