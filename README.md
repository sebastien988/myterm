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
  - Click a tab to switch, click the `x` to close, click `+` for a new one
- Dark theme with rounded pill tabs and a purple accent on the active tab
  (Konsole-style tab strip, Warp-style visual polish)
- Window title mirrors the active tab's shell title

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

## Run

```
cargo run --release
```

or run the built exe directly:

```
target\release\myterm.exe
```

By default each new tab launches `powershell.exe`. Override with:

```
$env:MYTERM_SHELL = "cmd.exe"
target\release\myterm.exe
```

## How it works

- `egui_term::TerminalBackend` opens a ConPTY per tab and spawns the shell
  attached to it, using `alacritty_terminal` for PTY I/O and VT/ANSI parsing.
- Each backend reports events (new output, title changes, exit) over an
  `mpsc` channel tagged with a tab id; `MyTermApp::update` drains that queue
  every frame and routes each event to the right tab.
- `egui_term::TerminalView` renders the parsed terminal grid directly with
  egui shapes and forwards keyboard/mouse input back into the PTY.
- Closing a tab's shell (or hitting the close button) removes that tab; when
  the last tab closes, the window closes.

## Extending it

This is the base you own — natural next steps:
- Custom keybindings via `egui_term::Binding`/`BindingAction`
- Split panes (multiple `TerminalBackend`s laid out side by side in one tab)
- Persisted themes/fonts via `egui_term::TerminalTheme`/`TerminalFont`
- Session logging (hook into the PTY event stream)
