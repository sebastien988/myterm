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
- Tabs: independent shell sessions, each with its own PTY, an activity dot for
  background output, and a bell indicator
- Copy and paste that behave like a terminal: `Ctrl+C` still interrupts,
  `Ctrl+Shift+C`/`Ctrl+Shift+V` use the clipboard, multi-line selections keep
  their line breaks, and pastes are bracketed when the shell asks for it
- Scrollback with a slim position indicator you can drag, plus
  `Shift+PageUp`/`PageDown` and `Ctrl+Shift+Home`/`End`
- Live font zoom (`Ctrl+=`, `Ctrl+-`, `Ctrl+0` or `Ctrl+Scroll`), optionally
  using a font file of your own
- Five color schemes that recolor the whole window, not just the grid
- A settings panel (`Ctrl+,`) and a plain-text config file that remembers your
  theme, font size, shells and window size between runs
- Shell profiles: right-click `+` to open PowerShell, `cmd`, WSL, or anything
  else you list in the config

## Keyboard shortcuts

| Keys | Action |
| --- | --- |
| `Ctrl+Shift+T` | New tab |
| `Ctrl+Shift+W` | Close tab |
| `Ctrl+Shift+N` | New window |
| `Ctrl+Shift+Q` | Quit |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Alt+1` … `Alt+9` | Jump to a tab |
| `Ctrl+Shift+PageUp` / `PageDown` | Move the current tab |
| `Ctrl+Shift+C` / `Ctrl+Shift+V` | Copy / paste (`Shift+Insert` pastes too) |
| `Shift+PageUp` / `PageDown` | Scroll one screen |
| `Ctrl+Shift+Home` / `End` | Scroll to the top / bottom of the scrollback |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Zoom in / out / reset |
| `Ctrl+Scroll` | Zoom in / out |
| `Alt+Enter` | Fullscreen |
| `Ctrl+,` | Settings |

Everything else goes to the shell: `Ctrl+C` interrupts, `Ctrl+U` kills the
line, `Alt+B`/`Alt+F` move by word, function and cursor keys send the usual
xterm sequences.

Mouse: click a tab to switch, middle-click or use the close button to close it,
right-click a tab to close it, close the others, or open another of the same
shell, scroll over the tab strip to cycle, double-click the empty strip to
open a tab, drag in the grid to select, `Ctrl+Click` a URL to open it.

## Configuration

Settings live in a plain-text file, written on first run:

- Windows: `%APPDATA%\myterm\myterm.conf`
- other platforms: `$XDG_CONFIG_HOME/myterm/myterm.conf`
- or wherever `MYTERM_CONFIG` points

```ini
# Shell launched in new tabs (default: powershell.exe on Windows).
shell = pwsh.exe
shell_args = -NoLogo

font_size = 14
# font_path = C:\Windows\Fonts\CascadiaMono.ttf

# myterm, alacritty, gruvbox-dark, solarized-dark, solarized-light
theme = myterm

tab_bar_auto_hide = false
confirm_close = true
scrollbar = true

# Extra entries for the new-tab menu (right-click the + button).
profile.PowerShell = powershell.exe -NoLogo
profile.Command Prompt = cmd.exe
profile.Ubuntu = wsl.exe -d Ubuntu
```

The settings panel writes the same file, keeping your comments and any keys it
does not manage. Window size and the last used theme and font size are saved
on exit. `MYTERM_SHELL` still overrides the default shell for one run.

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

It also builds and runs on Linux and macOS, which is handy for development;
new tabs there use `$SHELL`.

## Run

```
cargo run --release
```

or run the built exe directly:

```
target\release\myterm.exe
```

## Tests

```
cargo test
cargo clippy --all-targets
```

The suite covers the parts that are pure logic: key-to-escape-sequence
encoding, config parsing and rewriting (including Windows `%APPDATA%` /
XDG / `MYTERM_CONFIG` path resolution), tab-strip and scrollbar geometry,
tab index bookkeeping, clipboard text extraction and the color palettes.

## How it works

- `egui_term::TerminalBackend` opens a ConPTY per tab and spawns the shell
  attached to it, using `alacritty_terminal` for PTY I/O and VT/ANSI parsing.
- Each backend reports events (new output, title changes, bells, color and
  size queries, exit) over an `mpsc` channel tagged with a tab id;
  `MyTermApp::update` drains that queue every frame, answers the queries the
  shell is waiting on, and routes the rest to the right tab.
- myterm takes keyboard events off egui's queue itself (`input.rs`) and turns
  them into escape sequences or app actions before `TerminalView` sees them.
  That keeps typing working regardless of where the mouse pointer is, and
  stops shortcuts from leaking into the shell.
- `egui_term::TerminalView` renders the parsed terminal grid with egui shapes;
  the tab strip, scrollback indicator, settings panel and overlays are painted
  by `ui.rs`.
- Closing a tab's shell removes that tab; a shell that exits with an error
  leaves the tab in place so the status stays readable. When the last tab
  closes, the window closes.

### Source layout

| File | Contents |
| --- | --- |
| `src/main.rs` | entry point, window setup |
| `src/app.rs` | tabs, PTY event routing, actions, frame loop |
| `src/ui.rs` | tab strip, scrollbar, settings panel, overlays |
| `src/layout.rs` | tab-strip and scrollbar arithmetic (unit-tested) |
| `src/input.rs` | key handling and escape-sequence encoding |
| `src/config.rs` | `myterm.conf` parsing and rewriting |
| `src/theme.rs` | color schemes and the derived window chrome |

## Extending it

Natural next steps:
- Split panes (multiple `TerminalBackend`s laid out side by side in one tab)
- Search in the scrollback
- Session logging (hook into the PTY event stream)
- User-defined key bindings on top of `input::translate`
