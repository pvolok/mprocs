## Unreleased

## 0.7.0 - 2024-06-24

- Terminal sequences parser rewritten using termwiz vt parser
- Copying fixes for Wayland (#88)
- Support cursor shapes
- Various fixes for keyboard handling
- Accept configs with `.json` extension
- Introduce commands menu (press `p`)
- Add autorestart proccess config option
- Add scrollback config option

## 0.6.4 - 2023-02-17

- Add command for renaming the currently selected process (default: `e`)

## 0.6.3 - 2022-08-20

- Reimplement copying.

## 0.6.2 - 2022-08-09

- Fix global mprocs.yaml path when XDG_CONFIG_HOME env var is defined

## 0.6.1 - 2022-07-22

- Add copy mode
- Add `procs_list_width` to settings
- Add mouse scroll config
- Add quit confirmation dialog

## 0.6.0 - 2022-07-04

- Add `hide_keymap_window` to settings
- Add `--npm` argument
- Add `add_path` to proc config
- Highlight changed unselected processes
- Keymap help now uses actual keys (respecting config)
- Clears the terminal before the first render.

## 0.5.0 - 2022-06-20

- Add command for scrolling by N lines (`C-e`/`C-y`)
- Add mouse support
- Add autostart field to the process config

## 0.4.1 - 2022-06-17

- Zoom mode
- Support batching commands
- Allow passing `null` to clear key bindings

## 0.4.0 - 2022-06-08

- Add _--names_ cli argument
- Add stop field to the process config
- Add cwd field to the process config
- Add key bindings for selecting procs by index (`M-1` - `M-8`)
- Add keymap settings

## 0.3.0 - 2022-05-30

- Add "Remove process"
- Change default config path to mprocs.yaml
- Parse config file as yaml

## 0.2.3 - 2022-05-28

- Add "Add process" feature
- Use only indexed colors

## 0.2.2 - 2022-05-22

- Add experimental remote control
- Add $select operator in config
- Add restart command
- Add new arrow and page keybindings
- Fix build on rust stable

## 0.2.1 - 2022-05-15

- Fix terminal size on Windows

## 0.2.0 - 2022-05-15

- Scrolling terminal with <C-u>/<C-d>
- Environment variables per process in config
- Set commands via cli args

## 0.1.0 - 2022-04-05

- Full rewrite in Rust. Now compiles well on Windows
