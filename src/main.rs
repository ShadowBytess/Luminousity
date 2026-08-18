use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Attribute, Print, SetAttribute},
    terminal::{self, ClearType},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------
// Config + history helpers (mirrors the Python load_config / history fns)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    default_save_directory: String,
    tab_size: usize,
    expand_tabs: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            default_save_directory: home_dir_string(),
            tab_size: 4,
            expand_tabs: true,
        }
    }
}

fn home_dir_string() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
}

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("luminousity")
}

fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

fn history_file() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("luminousity_history.json")
}

/// Loads configuration options or creates default config if missing.
fn load_config() -> Config {
    let default_config = Config::default();
    let cfg_path = config_file();

    if !cfg_path.exists() {
        let _ = fs::create_dir_all(config_dir());
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            let _ = fs::write(&cfg_path, json);
        }
        return default_config;
    }

    match fs::read_to_string(&cfg_path) {
        Ok(contents) => match serde_json::from_str::<Value>(&contents) {
            Ok(mut val) => {
                // setdefault-style merge: fill in any missing keys, then deserialize.
                if let Value::Object(ref mut map) = val {
                    let defaults = serde_json::to_value(&default_config).unwrap();
                    if let Value::Object(default_map) = defaults {
                        for (k, v) in default_map {
                            map.entry(k).or_insert(v);
                        }
                    }
                }
                serde_json::from_value(val).unwrap_or(default_config)
            }
            Err(_) => default_config,
        },
        Err(_) => default_config,
    }
}

fn load_history() -> Vec<String> {
    let path = history_file();
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_history(history: &[String]) {
    let path = history_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(history) {
        let _ = fs::write(&path, json);
    }
}

fn record_recent_file(filepath: &str) {
    if filepath.is_empty() {
        return;
    }
    let abs_path = fs::canonicalize(filepath)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| filepath.to_string());

    let mut history = load_history();
    history.retain(|p| p != &abs_path);
    history.insert(0, abs_path);
    history.truncate(20);
    save_history(&history);
}

fn expand_user(path: &str) -> String {
    if let Some(rest) = path.strip_prefix('~') {
        let home = home_dir_string();
        format!("{home}{rest}")
    } else {
        path.to_string()
    }
}

// ---------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------

#[derive(PartialEq, Eq, Clone, Copy)]
enum Mode {
    Normal,
    Insert,
}

impl Mode {
    fn label(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
        }
    }
}

struct Editor {
    filename: Option<String>,
    lines: Vec<String>,
    cy: usize,
    cx: usize,
    scroll_top: usize,
    mode: Mode,
    config: Config,
    tab_size: usize,
    status_msg: String,
}

impl Editor {
    fn new(filename: Option<String>) -> Self {
        let config = load_config();
        let tab_size = config.tab_size;

        let mut editor = Editor {
            filename: None,
            lines: vec![String::new()],
            cy: 0,
            cx: 0,
            scroll_top: 0,
            mode: Mode::Normal,
            config,
            tab_size,
            status_msg: "Welcome to Luminousity | Press 'i' to Edit".to_string(),
        };

        if let Some(f) = filename {
            if Path::new(&f).exists() {
                editor.load_file(&f);
            } else {
                editor.filename = Some(f);
            }
        }

        editor
    }

    fn load_file(&mut self, filename: &str) {
        match fs::read_to_string(filename) {
            Ok(content) => {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                self.lines = if lines.is_empty() {
                    vec![String::new()]
                } else {
                    lines
                };
                self.filename = Some(filename.to_string());
                record_recent_file(filename);
                self.status_msg = format!("Opened: {filename}");
            }
            Err(e) => {
                self.status_msg = format!("Error opening file: {e}");
            }
        }
    }

    fn save_file(&mut self) -> io::Result<()> {
        let target = if let Some(f) = &self.filename {
            f.clone()
        } else {
            let save_dir = expand_user(&self.config.default_save_directory);
            let prompt = format!("Save file in [{save_dir}]: ");
            let input_name = self.prompt_input(&prompt)?;
            if input_name.is_empty() {
                self.status_msg = "Save cancelled.".to_string();
                return Ok(());
            }
            let target_path = PathBuf::from(&save_dir).join(&input_name);
            let target = target_path.to_string_lossy().to_string();
            self.filename = Some(target.clone());
            target
        };

        let path = PathBuf::from(&target);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        match fs::write(&path, self.lines.join("\n")) {
            Ok(_) => {
                record_recent_file(&target);
                self.status_msg = format!("Successfully saved: {target}");
            }
            Err(e) => {
                self.status_msg = format!("Error saving file: {e}");
            }
        }
        Ok(())
    }

    /// Blocking single-line text prompt shown on the bottom status row.
    fn prompt_input(&mut self, prompt: &str) -> io::Result<String> {
        let (width, height) = terminal::size()?;
        let width = width as usize;
        let row = height.saturating_sub(1);

        let mut stdout = io::stdout();
        let padded = format!("{:<width$}", prompt, width = width.saturating_sub(1));
        queue!(
            stdout,
            cursor::MoveTo(0, row),
            terminal::Clear(ClearType::CurrentLine),
            Print(&padded),
        )?;
        stdout.flush()?;

        let mut input = String::new();
        let start_col = prompt.chars().count() as u16;
        execute!(stdout, cursor::MoveTo(start_col, row), cursor::Show)?;

        loop {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Enter => break,
                    KeyCode::Esc => {
                        input.clear();
                        break;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                    }
                    _ => {}
                }
                let redraw = format!("{}{}", prompt, input);
                let padded = format!("{:<width$}", redraw, width = width.saturating_sub(1));
                queue!(
                    stdout,
                    cursor::MoveTo(0, row),
                    terminal::Clear(ClearType::CurrentLine),
                    Print(&padded),
                    cursor::MoveTo(start_col + input.chars().count() as u16, row),
                )?;
                stdout.flush()?;
            }
        }

        execute!(stdout, cursor::Hide)?;
        Ok(input.trim().to_string())
    }

    fn select_recent_file(&mut self) -> io::Result<()> {
        let history = load_history();
        if history.is_empty() {
            self.status_msg = "No recent files found.".to_string();
            return Ok(());
        }

        let (term_width, term_height) = terminal::size()?;
        let win_height = std::cmp::min(12, term_height.saturating_sub(2)).max(3);
        let win_width = std::cmp::min(65, term_width.saturating_sub(4)).max(10);
        let win_x = 2u16;
        let win_y = 2u16;

        let max_items = std::cmp::min(10, history.len());
        let mut idx = 0usize;
        let mut stdout = io::stdout();

        loop {
            // Draw a bordered box, matching curses' newwin + border + addstr.
            queue!(stdout, SetAttribute(Attribute::Reset))?;
            for row in 0..win_height {
                queue!(stdout, cursor::MoveTo(win_x, win_y + row))?;
                if row == 0 || row == win_height - 1 {
                    let line = format!(
                        "+{}+",
                        "-".repeat((win_width as usize).saturating_sub(2))
                    );
                    queue!(stdout, Print(&line))?;
                } else {
                    let line = format!(
                        "|{}|",
                        " ".repeat((win_width as usize).saturating_sub(2))
                    );
                    queue!(stdout, Print(&line))?;
                }
            }

            queue!(stdout, cursor::MoveTo(win_x + 2, win_y), Print("Luminousity Recent Files"))?;

            for i in 0..max_items {
                let avail = (term_width as usize).saturating_sub(12);
                let display: String = history[i].chars().take(avail).collect();
                queue!(stdout, cursor::MoveTo(win_x + 2, win_y + 1 + i as u16))?;
                if i == idx {
                    queue!(
                        stdout,
                        SetAttribute(Attribute::Reverse),
                        Print(format!("> {display}")),
                        SetAttribute(Attribute::Reset),
                    )?;
                } else {
                    queue!(stdout, Print(format!("  {display}")))?;
                }
            }
            stdout.flush()?;

            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        idx = (idx + max_items - 1) % max_items;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        idx = (idx + 1) % max_items;
                    }
                    KeyCode::Enter => {
                        let chosen = history[idx].clone();
                        self.load_file(&chosen);
                        break;
                    }
                    KeyCode::Esc => break,
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn get_leading_spaces(line: &str) -> String {
        line.chars().take_while(|&c| c == ' ').collect()
    }

    fn run(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

        let result = self.main_loop();

        execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;

        result
    }

    fn main_loop(&mut self) -> io::Result<()> {
        loop {
            self.render()?;

            let event = event::read()?;
            let (code, modifiers) = match event {
                Event::Key(KeyEvent { code, modifiers, .. }) => (code, modifiers),
                _ => continue,
            };

            self.status_msg.clear();

            // Hotkeys (Ctrl+R / Ctrl+T / Ctrl+S / Ctrl+Q), any mode.
            if modifiers.contains(KeyModifiers::CONTROL) {
                if let KeyCode::Char(c) = code {
                    match c {
                        'r' => {
                            let history = load_history();
                            if let Some(first) = history.first().cloned() {
                                self.load_file(&first);
                            } else {
                                self.status_msg = "No recent file history.".to_string();
                            }
                            continue;
                        }
                        't' => {
                            self.select_recent_file()?;
                            continue;
                        }
                        's' => {
                            self.save_file()?;
                            continue;
                        }
                        'q' => break,
                        _ => {}
                    }
                }
            }

            match self.mode {
                Mode::Normal => self.handle_normal(code),
                Mode::Insert => self.handle_insert(code, modifiers),
            }
        }
        Ok(())
    }

    fn handle_normal(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('n') => {
                self.lines = vec![String::new()];
                self.cy = 0;
                self.cx = 0;
                self.filename = None;
                self.status_msg = "Created a new empty buffer.".to_string();
            }
            KeyCode::Char('o') => {
                let indent = Self::get_leading_spaces(&self.lines[self.cy]);
                self.lines.insert(self.cy + 1, indent.clone());
                self.cy += 1;
                self.cx = indent.chars().count();
                self.mode = Mode::Insert;
            }
            KeyCode::Char('O') => {
                let indent = Self::get_leading_spaces(&self.lines[self.cy]);
                self.lines.insert(self.cy, indent.clone());
                self.cx = indent.chars().count();
                self.mode = Mode::Insert;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.cx = self.cx.saturating_sub(1);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let len = self.lines[self.cy].chars().count();
                self.cx = std::cmp::min(len, self.cx + 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cy > 0 {
                    self.cy -= 1;
                    let len = self.lines[self.cy].chars().count();
                    self.cx = std::cmp::min(self.cx, len);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cy < self.lines.len() - 1 {
                    self.cy += 1;
                    let len = self.lines[self.cy].chars().count();
                    self.cx = std::cmp::min(self.cx, len);
                }
            }
            KeyCode::Char('x') => {
                let mut chars: Vec<char> = self.lines[self.cy].chars().collect();
                if !chars.is_empty() && self.cx < chars.len() {
                    chars.remove(self.cx);
                    self.lines[self.cy] = chars.into_iter().collect();
                }
            }
            KeyCode::Char('g') => self.cy = 0,
            KeyCode::Char('G') => self.cy = self.lines.len() - 1,
            _ => {}
        }
    }

    fn handle_insert(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        // Many terminals send Ctrl+Backspace as the same byte as Ctrl+H
        // (ASCII 0x08), which crossterm reports as Char('h') + CONTROL
        // rather than KeyCode::Backspace. Treat it as backspace explicitly.
        if modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char('h') | KeyCode::Char('H') = code {
                self.delete_word_backward();
                return;
            }
        }

        match code {
            KeyCode::Esc => self.mode = Mode::Normal,

            KeyCode::Tab => {
                if self.config.expand_tabs {
                    let spaces = " ".repeat(self.tab_size);
                    let mut chars: Vec<char> = self.lines[self.cy].chars().collect();
                    for (offset, c) in spaces.chars().enumerate() {
                        chars.insert(self.cx + offset, c);
                    }
                    self.lines[self.cy] = chars.into_iter().collect();
                    self.cx += self.tab_size;
                } else {
                    let mut chars: Vec<char> = self.lines[self.cy].chars().collect();
                    chars.insert(self.cx, '\t');
                    self.lines[self.cy] = chars.into_iter().collect();
                    self.cx += 1;
                }
            }

            KeyCode::BackTab => {
                let line = self.lines[self.cy].clone();
                let prefix = " ".repeat(self.tab_size);
                if line.starts_with(&prefix) {
                    self.lines[self.cy] = line[self.tab_size..].to_string();
                    self.cx = self.cx.saturating_sub(self.tab_size);
                }
            }

            KeyCode::Enter => {
                let current_line: Vec<char> = self.lines[self.cy].chars().collect();
                let left: String = current_line[..self.cx].iter().collect();
                let right: String = current_line[self.cx..].iter().collect();

                let indent = Self::get_leading_spaces(&left);
                self.lines[self.cy] = left;
                self.lines.insert(self.cy + 1, format!("{indent}{right}"));
                self.cy += 1;
                self.cx = indent.chars().count();
            }

            KeyCode::Backspace => self.backspace(),

            KeyCode::Char(c) => {
                // Ignore Ctrl+letter chords here (e.g. Ctrl+H handled above,
                // and anything else with Ctrl held isn't meant to be typed
                // as a literal character).
                if modifiers.contains(KeyModifiers::CONTROL) {
                    return;
                }
                // Mirrors the Python `32 <= key <= 126` printable-ASCII check.
                if (' '..='~').contains(&c) {
                    let mut chars: Vec<char> = self.lines[self.cy].chars().collect();
                    chars.insert(self.cx, c);
                    self.lines[self.cy] = chars.into_iter().collect();
                    self.cx += 1;
                }
            }

            _ => {}
        }
    }

    /// Delete the character before the cursor, collapsing an indent block
    /// as a single step and joining with the previous line at column 0.
    fn backspace(&mut self) {
        if self.cx > 0 {
            let chars: Vec<char> = self.lines[self.cy].chars().collect();
            let is_indent_block = self.cx >= self.tab_size
                && chars[self.cx - self.tab_size..self.cx]
                    .iter()
                    .all(|&c| c == ' ');

            if is_indent_block {
                let mut new_chars = chars.clone();
                new_chars.drain(self.cx - self.tab_size..self.cx);
                self.lines[self.cy] = new_chars.into_iter().collect();
                self.cx -= self.tab_size;
            } else {
                let mut new_chars = chars.clone();
                new_chars.remove(self.cx - 1);
                self.lines[self.cy] = new_chars.into_iter().collect();
                self.cx -= 1;
            }
        } else if self.cy > 0 {
            let prev_len = self.lines[self.cy - 1].chars().count();
            let current = self.lines.remove(self.cy);
            self.lines[self.cy - 1].push_str(&current);
            self.cy -= 1;
            self.cx = prev_len;
        }
    }

    /// Delete the "word" behind the cursor (Ctrl+Backspace). Skips any
    /// trailing whitespace first, then deletes back through the run of
    /// non-whitespace characters, stopping at the next word boundary.
    /// At column 0, falls back to joining with the previous line.
    fn delete_word_backward(&mut self) {
        if self.cx == 0 {
            self.backspace();
            return;
        }

        let chars: Vec<char> = self.lines[self.cy].chars().collect();
        let end = std::cmp::min(self.cx, chars.len());
        let mut start = end;

        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }

        let mut new_chars = chars;
        new_chars.drain(start..end);
        self.lines[self.cy] = new_chars.into_iter().collect();
        self.cx = start;
    }

    fn render(&mut self) -> io::Result<()> {
        let (width, height) = terminal::size()?;
        let width = width as usize;
        let height = height as usize;
        let mut stdout = io::stdout();

        queue!(stdout, terminal::Clear(ClearType::All))?;

        // Keep cursor row within the visible text area (height - 2 rows).
        let text_rows = height.saturating_sub(2);
        if self.cy < self.scroll_top {
            self.scroll_top = self.cy;
        } else if self.cy >= self.scroll_top + text_rows {
            self.scroll_top = self.cy + 1 - text_rows;
        }

        // Render text with line numbers, tabs expanded for display.
        for idx in 0..text_rows {
            let line_idx = self.scroll_top + idx;
            if line_idx < self.lines.len() {
                let line_num = format!("{:>3} ", line_idx + 1);
                let render_line = expand_tabs(&self.lines[line_idx], self.tab_size);
                let text: String = render_line.chars().take(width.saturating_sub(5)).collect();

                queue!(
                    stdout,
                    cursor::MoveTo(0, idx as u16),
                    SetAttribute(Attribute::Dim),
                    Print(&line_num),
                    SetAttribute(Attribute::Reset),
                    cursor::MoveTo(4, idx as u16),
                    Print(&text),
                )?;
            }
        }

        // Statusline
        let file_disp = self.filename.clone().unwrap_or_else(|| "[New Buffer]".to_string());
        let status_text = format!(
            " LUMINOUSITY | {} | {} | Ln {}, Col {} ",
            self.mode.label(),
            file_disp,
            self.cy + 1,
            self.cx + 1
        );
        let status_row = height.saturating_sub(2) as u16;
        let padded_status = format!(
            "{:<width$}",
            truncate(&status_text, width.saturating_sub(1)),
            width = width.saturating_sub(1)
        );
        queue!(
            stdout,
            cursor::MoveTo(0, status_row),
            SetAttribute(Attribute::Reverse),
            Print(&padded_status),
            SetAttribute(Attribute::Reset),
        )?;

        // Output / message bar
        let output_row = height.saturating_sub(1) as u16;
        let msg = truncate(&self.status_msg, width.saturating_sub(1));
        queue!(
            stdout,
            cursor::MoveTo(0, output_row),
            SetAttribute(Attribute::Bold),
            Print(&msg),
            SetAttribute(Attribute::Reset),
        )?;

        // Cursor position
        let screen_y = (self.cy - self.scroll_top) as u16;
        let screen_x = std::cmp::min(self.cx + 4, width.saturating_sub(1)) as u16;
        queue!(stdout, cursor::MoveTo(screen_x, screen_y), cursor::Show)?;

        stdout.flush()
    }
}

fn expand_tabs(line: &str, tab_size: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut col = 0usize;
    for c in line.chars() {
        if c == '\t' {
            let spaces = tab_size - (col % tab_size);
            out.push_str(&" ".repeat(spaces));
            col += spaces;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

fn main() -> io::Result<()> {
    let filepath = env::args().nth(1);
    let mut editor = Editor::new(filepath);
    editor.run()
}
