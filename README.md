# mprocs

mprocs runs multiple commands in parallel and shows output of each command
separately.

## Screenshots

<img src="img/screenshot1.png" width="656" height="410" />
<img src="img/screenshot2.png" width="656" height="410" />

## Installation

[Download](https://github.com/pvolok/mprocs/releases) executable for your
platform and put it into a directory included in PATH.

## Usage

1. Create `mprocs.json` file
2. Run `mprocs` command

Example `mprocs.json`:

```json
{
  "procs": {
    "nvim": {
      "cmd": "nvim",
      "args": ["nvim"]
    },
    "server": {
      "shell": "nodemon server.js"
    },
    "webpack": {
      "shell": "webpack serve"
    },
    "tests": {
      "shell": "jest -w",
      "tty": false
    }
  }
}
```

### Key bindings

Process list focused:
- `q` - Quit
- `C-a` - Focus output pane
- `x` - Kill selected process
- `s` - Start selected process, if it is not running
- `k` - Select previous process
- `j` - Select next process

Process output focused:
- `C-a` - Focus processes pane
