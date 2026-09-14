# myterm

<img src="docs/logo/icon-1024.png" width="96" height="96" alt="myterm logo">

A native Windows terminal emulator with its own window and tabs — built with
[egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui)
for rendering and [egui_term](https://github.com/Harzu/egui_term) (backed by
[alacritty_terminal](https://github.com/alacritty/alacritty)) for PTY
management and VT/ANSI parsing.

## Features

- Real windowed GUI app (no console box) — a genuine terminal emulator, not a
  console wrapper
- Tabs: independent shell sessions, each with its own PTY
  - `Ctrl+Shift+T` — new tab
  - `Ctrl+Shift+W` — close active tab
  - `Ctrl+Tab` / `Ctrl+Shift+Tab` — cycle tabs
  - Click a tab to switch, click the `×` to close, click `+` for a new one
  - App shortcuts are consumed before the terminal sees them, so they never
    reach the shell as `^T`/`^W`
- Dark theme with rounded pill tabs and a purple accent on the active tab
  (Konsole-style tab strip, Warp-style visual polish)
- Window title mirrors the active tab's shell title, including after switching
  or closing tabs
- Answers the escape sequences programs use to interrogate the terminal —
  device attributes, cursor position, text area size, `OSC 4/10/11` color
  queries — instead of leaving them to time out
- Copies on `OSC 52`, so `yank`-style clipboard integration works over SSH

## Build

```
cargo build --release
```

The binary is written to `target/release/myterm.exe`.

### Toolchain

This project builds with the **GNU** Rust toolchain (`x86_64-pc-windows-gnu`)
plus a MinGW-w64 (WinLibs) linker, not MSVC — no Visual Studio Build Tools
required. If you're building on a fresh machine:

```
winget install --id Rustlang.Rustup -e
winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
```

Make sure the MinGW `bin` directory (containing `gcc.exe`) is on `PATH` —
and make sure that path has **no spaces** in it. MinGW's internal linker
invocation breaks on paths like `C:\Users\your name\...`; if your WinGet
install location has a space in it, copy the `mingw64` folder somewhere
space-free (e.g. `C:\mingw64`) and put that on `PATH` instead.

Windows is the target platform, but the app builds and runs on Linux too
(`egui_term` opens a normal Unix PTY there), which is what CI tests against.

## Run

```
cargo run --release
```

or run the built exe directly:

```
target\release\myterm.exe
```

### Configuration

Read from the environment at startup:

| Variable             | Default                             | Meaning                                  |
| -------------------- | ----------------------------------- | ---------------------------------------- |
| `MYTERM_SHELL`       | `powershell.exe` (`$SHELL` on Unix) | Program launched in each new tab         |
| `MYTERM_SHELL_ARGS`  | *(none)*                            | Arguments for it, quote-aware            |
| `MYTERM_FONT_SIZE`   | `14`                                | Terminal font size in points, 6–72       |

```
$env:MYTERM_SHELL = "cmd.exe"
$env:MYTERM_SHELL_ARGS = "/k echo hello"
target\release\myterm.exe
```

If the shell can't be started, the window says so and offers to retry rather
than closing silently — release builds have no console to print an error to.

## How it works

- `egui_term::TerminalBackend` opens a ConPTY per tab and spawns the shell
  attached to it, using `alacritty_terminal` for PTY I/O and VT/ANSI parsing.
- Each backend reports events (new output, title changes, terminal queries,
  exit) over an `mpsc` channel tagged with a tab id; `MyTermApp::update`
  drains that queue every frame and routes each event to the right tab.
- Queries get answered from that same loop: replies are written straight back
  into the originating tab's PTY, and color queries are answered from the same
  palette the grid is painted with.
- `egui_term::TerminalView` renders the parsed terminal grid directly with
  egui shapes and forwards keyboard/mouse input back into the PTY.
- Closing a tab's shell (or hitting the close button) removes that tab; when
  the last tab closes, the window closes.

### Layout

| File            | Contents                                                     |
| --------------- | ------------------------------------------------------------ |
| `src/main.rs`   | Entry point and window setup                                 |
| `src/app.rs`    | `MyTermApp`: PTY event loop, shortcuts, tab strip, terminal  |
| `src/tabs.rs`   | Tab list and selection arithmetic, generic over the payload  |
| `src/config.rs` | Environment-driven configuration                             |
| `src/theme.rs`  | App chrome colors and the palette the terminal is painted in |

## Develop

```
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

`tabs`, `config` and `theme` hold the logic that is easy to get quietly wrong —
which tab is selected after a close, how a title is truncated, how a color
index maps to a color — and are unit tested. CI runs the same three commands
on Linux and builds the Windows binary with the GNU toolchain.

## Extending it

This is the base you own — natural next steps:
- Custom keybindings via `egui_term::Binding`/`BindingAction`
- Split panes (multiple `TerminalBackend`s laid out side by side in one tab)
- Persisted themes/fonts via `egui_term::TerminalTheme`/`TerminalFont`
- Session logging (hook into the PTY event stream)
- Answering `OSC 52` paste requests (`ClipboardLoad`), which needs a clipboard
  read path egui does not currently expose

## License

MIT — see [LICENSE](LICENSE).
