# dutui

A fast terminal disk-usage explorer in the spirit of DaisyDisk. The left pane is the
filesystem; the right pane is a treemap of what is taking up the space.

![dutui demo](assets/demo.gif)

## Features

- **Treemap view** with nested folders, coloured to match the file list.
- **Fast parallel scanner.** Uses `getattrlistbulk` on macOS, and results appear while
  the scan is still running. Opening a folder mid-scan cancels the scan and scans that
  folder instead.
- **Starts where you launch it**, and you can still navigate above it. Going up scans
  the parent and reuses what is already known.
- **Trash or delete** from inside the app, with confirmation.
- **Themes:** Tokyo Night, Catppuccin (Latte, Frappé, Macchiato, Mocha), Dracula and
  One Dark, with an optional transparent background.
- **Mouse support:** click to select, double-click to open the folder under the
  cursor, right-click to go up, scroll wheel to move.

## Installation

### macOS (Homebrew)

Homebrew on macOS (Apple Silicon and Intel) is the supported way to install, with prebuilt binaries:

```sh
brew tap simoncoulton/dutui https://github.com/simoncoulton/dutui
brew install dutui
```

### Arch Linux and other distributions

There is no package for Arch or its derivatives yet. Build from source with a recent
[Rust toolchain](https://rustup.rs) (1.89 or newer):

```sh
git clone https://github.com/simoncoulton/dutui
cd dutui
cargo build --release
./target/release/dutui [PATH]
```

`PATH` defaults to the current directory.


## Keys

| Key | Action |
| --- | --- |
| `j` / `k` | Move selection (also `↑` / `↓`) |
| `l` / `Enter` | Open the selected folder (also `→`) |
| `h` / `Backspace` | Go up a level (also `←`) |
| `g` / `G` | Jump to first / last |
| `d` | Move the selection to the Trash (asks first) |
| `D` | Permanently delete the selection (asks first, folders recursively) |
| `r` | Rescan |
| `t` | Theme picker, with live preview |
| `b` | Toggle transparent background |
| `E` | Edit the config file in `$VISUAL` / `$EDITOR` (reloaded on exit) |
| `u` | Toggle units: GB (decimal) / GiB (binary) |
| `x` | Include / skip other mounted drives and volumes (rescans) |
| `?` | Help |
| `q` / `Esc` | Quit |

## Configuration

Everything lives in `~/.config/dutui/` and is plain YAML.

`dutui.yml` is saved whenever you change a setting:

```yaml
theme: tokyo-night        # the name of a file in themes/ (without .yml)
transparent: false        # use the terminal's own background
cross_filesystems: false  # follow other mounted drives and volumes
units: si                 # si (kB, MB, GB) or iec (KiB, MiB, GiB)
```

Changes made in dutui edit only their own line, so your comments are kept. Press `E` to
edit the file by hand. An invalid file (unknown key, bad YAML, unknown theme) is reported
with its path and line instead of being silently replaced.

### Themes

`themes/` holds one `.yml` file per theme. On first run dutui writes a file for each
built-in theme (default, tokyo-night, the four catppuccin flavours, dracula, one-dark,
gruvbox-dark, nord, solarized-dark, rose-pine, monokai, github-dark, everforest, kanagawa),
and the same set torrentui ships. Edit them freely: dutui never overwrites an existing file.

To add a theme, drop a new `.yml` file in the directory. It appears in the picker (`t`) the
next time it opens, with no restart. The file name is the theme's name:

```yaml
base: nord                # optional: take any colour you leave out from this theme
accent: "#ff9e64"         # #rrggbb, an ANSI name (red, darkgray, reset) or 0-255
```

The colours are `bg fg muted accent border selection_bg success info warning error special`.

## Notes

- Sizes are disk usage (allocated blocks), not apparent file size, and hard-linked files
  are counted once. Symlinks are never followed.
- `dutui --bench PATH` scans without the UI and prints the time taken.
- `scripts/gen-example.sh` creates `example/`, about 10 GB of zero-filled files for trying
  the app out. Remove it with `rm -rf example`.
