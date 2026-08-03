#!/usr/bin/env python3
import curses
import json
import os
import sys
from pathlib import Path

CONFIG_DIR = Path.home() / ".config" / "luminousity"
CONFIG_FILE = CONFIG_DIR / "config.json"
HISTORY_FILE = Path.home() / ".cache" / "luminousity_history.json"


def load_config():
    """Loads configuration options or creates default config if missing."""
    default_config = {
        "default_save_directory": str(Path.home()),
        "tab_size": 4,
        "expand_tabs": True,  # Use spaces instead of raw tabs
    }

    if not CONFIG_FILE.exists():
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        try:
            with open(CONFIG_FILE, "w") as f:
                json.dump(default_config, f, indent=4)
        except Exception:
            pass
        return default_config

    try:
        with open(CONFIG_FILE, "r") as f:
            user_config = json.load(f)
            for key, val in default_config.items():
                user_config.setdefault(key, val)
            return user_config
    except Exception:
        return default_config


def load_history():
    if HISTORY_FILE.exists():
        try:
            with open(HISTORY_FILE, "r") as f:
                return json.load(f)
        except Exception:
            return []
    return []


def save_history(history):
    HISTORY_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(HISTORY_FILE, "w") as f:
        json.dump(history, f)


def record_recent_file(filepath):
    if not filepath:
        return
    abs_path = str(Path(filepath).resolve())
    history = load_history()
    if abs_path in history:
        history.remove(abs_path)
    history.insert(0, abs_path)
    save_history(history[:20])


class LuminousityEditor:

    def __init__(self, stdscr, filename=None):
        self.stdscr = stdscr
        self.filename = filename
        self.lines = [""]
        self.cy = 0
        self.cx = 0
        self.scroll_top = 0
        self.mode = "NORMAL"
        self.config = load_config()
        self.tab_size = self.config.get("tab_size", 4)
        self.status_msg = "Welcome to Luminousity | Press 'i' to Edit"

        if filename and os.path.exists(filename):
            self.load_file(filename)

    def load_file(self, filename):
        try:
            with open(filename, "r", encoding="utf-8") as f:
                content = f.read().splitlines()
                self.lines = content if content else [""]
            self.filename = filename
            record_recent_file(filename)
            self.status_msg = f"Opened: {filename}"
        except Exception as e:
            self.status_msg = f"Error opening file: {e}"

    def save_file(self, filename=None):
        target = filename or self.filename
        if not target:
            save_dir = Path(
                os.path.expanduser(
                    self.config.get("default_save_directory", "~")
                )
            )
            input_name = self.prompt_input(f"Save file in [{save_dir}]: ")
            if not input_name:
                self.status_msg = "Save cancelled."
                return

            target_path = save_dir / input_name
            target = str(target_path)
            self.filename = target

        try:
            Path(self.filename).parent.mkdir(parents=True, exist_ok=True)
            with open(self.filename, "w", encoding="utf-8") as f:
                f.write("\n".join(self.lines))
            record_recent_file(self.filename)
            self.status_msg = f"Successfully saved: {self.filename}"
        except Exception as e:
            self.status_msg = f"Error saving file: {e}"

    def prompt_input(self, prompt):
        height, width = self.stdscr.getmaxyx()
        self.stdscr.addstr(height - 1, 0, prompt.ljust(width - 1))
        curses.echo()
        curses.curs_set(1)
        inp = self.stdscr.getstr(height - 1, len(prompt)).decode("utf-8")
        curses.noecho()
        return inp.strip()

    def select_recent_file(self):
        history = load_history()
        if not history:
            self.status_msg = "No recent files found."
            return

        height, width = self.stdscr.getmaxyx()
        menu_win = curses.newwin(min(12, height - 2), min(65, width - 4), 2, 2)
        menu_win.keypad(True)

        idx = 0
        while True:
            menu_win.erase()
            menu_win.border()
            menu_win.addstr(0, 2, "Luminousity Recent Files")

            max_items = min(10, len(history))
            for i in range(max_items):
                display_str = history[i][: width - 12]
                if i == idx:
                    menu_win.addstr(
                        i + 1, 2, f"> {display_str}", curses.A_REVERSE
                    )
                else:
                    menu_win.addstr(i + 1, 2, f"  {display_str}")

            menu_win.refresh()
            key = menu_win.getch()

            if key in (curses.KEY_UP, ord("k")):
                idx = (idx - 1) % max_items
            elif key in (curses.KEY_DOWN, ord("j")):
                idx = (idx + 1) % max_items
            elif key in (10, 13):
                self.load_file(history[idx])
                break
            elif key == 27:
                break

    def get_leading_spaces(self, line):
        return line[: len(line) - len(line.lstrip(" "))]

    def run(self):
        curses.use_default_colors()
        curses.raw()
        self.stdscr.keypad(True)

        while True:
            height, width = self.stdscr.getmaxyx()
            self.stdscr.erase()

            if self.cy < self.scroll_top:
                self.scroll_top = self.cy
            elif self.cy >= self.scroll_top + (height - 2):
                self.scroll_top = self.cy - (height - 2) + 1

            # Render text
            for idx in range(height - 2):
                line_idx = self.scroll_top + idx
                if line_idx < len(self.lines):
                    line_num = f"{line_idx + 1:3d} "
                    # Expand tabs visually so curses doesn't misplace characters
                    render_line = self.lines[line_idx].expandtabs(self.tab_size)
                    text = render_line[: width - 5]
                    self.stdscr.addstr(idx, 0, line_num, curses.A_DIM)
                    self.stdscr.addstr(idx, 4, text)

            # Statusline
            file_disp = self.filename or "[New Buffer]"
            status_text = f" LUMINOUSITY | {self.mode} | {file_disp} | Ln {self.cy+1}, Col {self.cx+1} "
            self.stdscr.addstr(
                height - 2, 0, status_text.ljust(width - 1), curses.A_REVERSE
            )

            # Output Bar
            self.stdscr.addstr(
                height - 1, 0, self.status_msg[: width - 1], curses.A_BOLD
            )

            # Cursor position calculation
            screen_y = self.cy - self.scroll_top
            screen_x = min(self.cx + 4, width - 1)
            self.stdscr.move(screen_y, screen_x)

            key = self.stdscr.getch()
            self.status_msg = ""

            # Hotkeys
            if key == 18:  # Ctrl + R
                history = load_history()
                if history:
                    self.load_file(history[0])
                else:
                    self.status_msg = "No recent file history."
                continue
            elif key == 20:  # Ctrl + T
                self.select_recent_file()
                continue
            elif key == 19:  # Ctrl + S
                self.save_file()
                continue
            elif key == 17:  # Ctrl + Q
                break

            # Normal Mode
            if self.mode == "NORMAL":
                if key == ord("i"):
                    self.mode = "INSERT"
                elif key == ord("n"):
                    self.lines = [""]
                    self.cy, self.cx = 0, 0
                    self.filename = None
                    self.status_msg = "Created a new empty buffer."
                elif key == ord("o"):
                    indent = self.get_leading_spaces(self.lines[self.cy])
                    self.lines.insert(self.cy + 1, indent)
                    self.cy += 1
                    self.cx = len(indent)
                    self.mode = "INSERT"
                elif key == ord("O"):
                    indent = self.get_leading_spaces(self.lines[self.cy])
                    self.lines.insert(self.cy, indent)
                    self.cx = len(indent)
                    self.mode = "INSERT"
                elif key == ord("h") or key == curses.KEY_LEFT:
                    self.cx = max(0, self.cx - 1)
                elif key == ord("l") or key == curses.KEY_RIGHT:
                    self.cx = min(len(self.lines[self.cy]), self.cx + 1)
                elif key == ord("k") or key == curses.KEY_UP:
                    if self.cy > 0:
                        self.cy -= 1
                        self.cx = min(self.cx, len(self.lines[self.cy]))
                elif key == ord("j") or key == curses.KEY_DOWN:
                    if self.cy < len(self.lines) - 1:
                        self.cy += 1
                        self.cx = min(self.cx, len(self.lines[self.cy]))
                elif key == ord("x"):
                    line = self.lines[self.cy]
                    if len(line) > 0 and self.cx < len(line):
                        self.lines[self.cy] = (
                            line[: self.cx] + line[self.cx + 1 :]
                        )
                elif key == ord("g"):
                    self.cy = 0
                elif key == ord("G"):
                    self.cy = len(self.lines) - 1

            # Insert Mode
            elif self.mode == "INSERT":
                if key == 27:  # ESC
                    self.mode = "NORMAL"

                elif key == 9:  # TAB Key
                    if self.config.get("expand_tabs", True):
                        spaces = " " * self.tab_size
                        line = self.lines[self.cy]
                        self.lines[self.cy] = (
                            line[: self.cx] + spaces + line[self.cx :]
                        )
                        self.cx += self.tab_size
                    else:
                        line = self.lines[self.cy]
                        self.lines[self.cy] = (
                            line[: self.cx] + "\t" + line[self.cx :]
                        )
                        self.cx += 1

                elif key == curses.KEY_BTAB:  # Shift + TAB (Backtab)
                    line = self.lines[self.cy]
                    if line.startswith(" " * self.tab_size):
                        self.lines[self.cy] = line[self.tab_size :]
                        self.cx = max(0, self.cx - self.tab_size)

                elif key in (10, 13):  # Enter key (Auto-Indent)
                    current_line = self.lines[self.cy]
                    left = current_line[: self.cx]
                    right = current_line[self.cx :]

                    indent = self.get_leading_spaces(left)
                    self.lines[self.cy] = left
                    self.lines.insert(self.cy + 1, indent + right)
                    self.cy += 1
                    self.cx = len(indent)

                elif key in (curses.KEY_BACKSPACE, 127, 8):
                    if self.cx > 0:
                        line = self.lines[self.cy]
                        # Check if deleting an indented space block
                        if (
                            self.cx >= self.tab_size
                            and line[
                                self.cx - self.tab_size : self.cx
                            ]
                            == " " * self.tab_size
                        ):
                            self.lines[self.cy] = (
                                line[: self.cx - self.tab_size]
                                + line[self.cx :]
                            )
                            self.cx -= self.tab_size
                        else:
                            self.lines[self.cy] = (
                                line[: self.cx - 1] + line[self.cx :]
                            )
                            self.cx -= 1
                    elif self.cy > 0:
                        prev_len = len(self.lines[self.cy - 1])
                        self.lines[self.cy - 1] += self.lines.pop(self.cy)
                        self.cy -= 1
                        self.cx = prev_len

                elif 32 <= key <= 126:  # Regular printable characters
                    line = self.lines[self.cy]
                    self.lines[self.cy] = (
                        line[: self.cx] + chr(key) + line[self.cx :]
                    )
                    self.cx += 1


def main():
    filepath = sys.argv[1] if len(sys.argv) > 1 else None
    curses.wrapper(lambda stdscr: LuminousityEditor(stdscr, filepath).run())


if __name__ == "__main__":
    main()
