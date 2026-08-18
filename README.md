# Luminousity

Luminousity is a lightweight, modal terminal text editor built for Arch Linux / CachyOS using Rust and `crossterm`. It provides a Vim inspired editing experience with custom shortcuts, automatic file history tracking, tab formatting support, and configurable default save locations.

Originally written in Python with `curses`, Luminousity has been rewritten in Rust for better performance and a smaller footprint, while keeping the same keybindings, config format, and overall behavior.

## Features

- Dual Editing Modes: Switch between NORMAL and INSERT modes.
- Recent File Tracking: Quickly reopen the last file or select from a history list.
- Configurable Defaults: Customize settings via `~/.config/luminousity/config.json`.
- Code Formatting: Auto-indentation, configurable tab sizes, space expansion, and backtab support.
- Word Deletion: Ctrl+Backspace deletes the word behind the cursor instead of a single character.
- Desktop Integration: Launchable via desktop files and usable as a default editor in Dolphin and KDE Plasma.

## Requirements

- Rust and Cargo (install via `rustup` or your distro's package manager)

## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/ShadowBytess/luminousity.git
   cd luminousity
   ```

2. Build the release binary:
   ```bash
   cargo build --release
   ```

3. Install it to your PATH:
   ```bash
   sudo cp target/release/luminousity /usr/local/bin/
   sudo chmod +x /usr/local/bin/luminousity
   ```

4. Run it from anywhere:
   ```bash
   luminousity
   luminousity somefile.txt
   ```

To update after pulling new changes, rerun `cargo build --release` and copy the binary over again.

## Usage

### Modes

Luminousity opens in NORMAL mode. Press `i` to enter INSERT mode and start typing. Press `Esc` to return to NORMAL mode.

### Keybindings

**NORMAL mode**

| Key | Action |
|---|---|
| `i` | Enter INSERT mode |
| `n` | Create a new empty buffer |
| `o` | Insert a new line below the cursor and enter INSERT mode |
| `O` | Insert a new line above the cursor and enter INSERT mode |
| `h` / Left | Move cursor left |
| `l` / Right | Move cursor right |
| `k` / Up | Move cursor up |
| `j` / Down | Move cursor down |
| `x` | Delete the character under the cursor |
| `g` | Jump to the first line |
| `G` | Jump to the last line |

**INSERT mode**

| Key | Action |
|---|---|
| `Esc` | Return to NORMAL mode |
| `Tab` | Insert a tab (spaces or a raw tab, per config) |
| `Shift+Tab` | Remove one level of indentation |
| `Enter` | Insert a new line with auto-indent |
| `Backspace` | Delete the character before the cursor, or join with the previous line at column 0 |
| `Ctrl+Backspace` | Delete the word behind the cursor |

**Global hotkeys (any mode)**

| Key | Action |
|---|---|
| `Ctrl+S` | Save the current file |
| `Ctrl+R` | Reopen the most recently edited file |
| `Ctrl+T` | Open a picker to choose from recent files |
| `Ctrl+Q` | Quit |

## Configuration

Luminousity reads `~/.config/luminousity/config.json` on startup and creates it with sane defaults if it does not exist:

```json
{
    "default_save_directory": "/home/youruser",
    "tab_size": 4,
    "expand_tabs": true
}
```

- `default_save_directory`: Where new, unnamed buffers are saved by default.
- `tab_size`: Number of spaces per indentation level.
- `expand_tabs`: If `true`, `Tab` inserts spaces. If `false`, it inserts a raw tab character.

Recently opened and saved files are tracked in `~/.cache/luminousity_history.json`, capped at the 20 most recent entries.

## Desktop Integration

Luminousity can be launched from application launchers like Wofi or set as a default text editor in Dolphin and KDE Plasma by pointing a `.desktop` file's `Exec` line at `/usr/local/bin/luminousity %f`.

## License

Add your preferred license here.
