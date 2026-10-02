//! Text-mode file browser (`orchid --tui`).
//!
//! Lists one local folder at a time and previews small text files. It does
//! not start the desktop window, open network mounts, or use the viewers.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local};
use orchid_i18n::{default_language, LocaleId, LocaleManager};
use orchid_storage::{ConfigLoader, OrchidPaths};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

const LIST_CAP: usize = 5000;
const PREVIEW_CAP: u64 = 256 * 1024;
const PREVIEW_LINES: usize = 2000;

/// Where `--tui` should start. An empty `start` uses the current directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiLaunch {
    /// Path passed after `--tui`, if any.
    pub start: Option<PathBuf>,
}

/// `Some` when argv asks for the text-mode browser.
#[must_use]
pub fn tui_launch<I, S>(args: I) -> Option<TuiLaunch>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<OsString> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect();
    for (index, arg) in args.iter().enumerate() {
        if let Some(text) = arg.to_str() {
            if let Some(rest) = text.strip_prefix("--tui=") {
                let start = if rest.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(rest))
                };
                return Some(TuiLaunch { start });
            }
        }
        if arg == "--tui" {
            let start = args.get(index + 1).and_then(|next| {
                let looks_like_flag = next.to_str().is_some_and(|text| text.starts_with('-'));
                if looks_like_flag {
                    None
                } else {
                    Some(PathBuf::from(next))
                }
            });
            return Some(TuiLaunch { start });
        }
    }
    None
}

/// Run the browser until the user quits.
///
/// # Errors
///
/// Returns an error when the start path cannot be read or the terminal
/// cannot enter the alternate screen.
pub fn run_tui(launch: TuiLaunch) -> Result<(), String> {
    ensure_console();
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let dir = resolve_start(launch.start.as_deref(), &cwd)?;
    let mut model = Model::open(dir, Labels::load())?;
    let mut terminal = ratatui::try_init().map_err(|e| e.to_string())?;
    let _restore = TerminalGuard;
    loop {
        terminal
            .draw(|frame| draw(frame, &mut model))
            .map_err(|e| e.to_string())?;
        if !event::poll(Duration::from_millis(200)).map_err(|e| e.to_string())? {
            continue;
        }
        let Ok(event) = event::read() else {
            continue;
        };
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if model.on_key(key.code, key.modifiers) {
            break;
        }
    }
    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

fn ensure_console() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleWindow() -> *mut std::ffi::c_void;
            fn AttachConsole(dw_process_id: u32) -> i32;
            fn AllocConsole() -> i32;
            fn SetConsoleOutputCP(code_page: u32) -> i32;
        }
        unsafe {
            if GetConsoleWindow().is_null() && AttachConsole(u32::MAX) == 0 {
                let _ = AllocConsole();
            }
            let _ = SetConsoleOutputCP(65001);
        }
    }
}

struct Labels {
    app: String,
    help: String,
    empty: String,
    binary: String,
    read_error: String,
    preview_help: String,
    too_big: String,
    filter: String,
    truncated: String,
    col_name: String,
    col_size: String,
    col_modified: String,
}

impl Labels {
    fn english() -> Self {
        Self {
            app: "Orchid files".to_string(),
            help: "Up/Down move   Enter open   Backspace up   / filter   h dotfiles   q quit"
                .to_string(),
            empty: "Empty folder".to_string(),
            binary: "Not a text file. Open it in the desktop app.".to_string(),
            read_error: "Could not read this folder.".to_string(),
            preview_help: "Up/Down scroll   Esc back".to_string(),
            too_big: "That file is larger than 256 KiB.".to_string(),
            filter: "filter".to_string(),
            truncated: "Showing the first 5000 names.".to_string(),
            col_name: "Name".to_string(),
            col_size: "Size".to_string(),
            col_modified: "Modified".to_string(),
        }
    }

    fn load() -> Self {
        let locale = ui_locale();
        let Ok(manager) = LocaleManager::new(locale, None) else {
            return Self::english();
        };
        Self {
            app: manager.tr("tui-app"),
            help: manager.tr("tui-help"),
            empty: manager.tr("tui-empty"),
            binary: manager.tr("tui-binary"),
            read_error: manager.tr("tui-read-error"),
            preview_help: manager.tr("tui-preview-help"),
            too_big: manager.tr("tui-too-big"),
            filter: manager.tr("tui-filter"),
            truncated: manager.tr("tui-truncated"),
            col_name: manager.tr("tui-col-name"),
            col_size: manager.tr("tui-col-size"),
            col_modified: manager.tr("tui-col-modified"),
        }
    }
}

fn ui_locale() -> LocaleId {
    let Ok(paths) = OrchidPaths::resolve() else {
        return default_language();
    };
    if !paths.config_file.exists() {
        return default_language();
    }
    let Ok(cfg) = ConfigLoader::load(&paths.config_file) else {
        return default_language();
    };
    LocaleId::parse(&cfg.locale.language).unwrap_or_else(|_| default_language())
}

struct Entry {
    name: String,
    dir: bool,
    symlink: bool,
    size: Option<u64>,
    modified: Option<SystemTime>,
}

struct Listing {
    rows: Vec<Entry>,
    truncated: bool,
}

struct Preview {
    lines: Vec<String>,
    scroll: usize,
}

enum PreviewLoad {
    Text(Vec<String>),
    Binary,
    TooBig,
    Failed,
}

struct Model {
    dir: PathBuf,
    rows: Vec<Entry>,
    cursor: usize,
    filter: String,
    filter_lower: String,
    filtering: bool,
    show_hidden: bool,
    status: String,
    preview: Option<Preview>,
    view_rows: usize,
    labels: Labels,
}

impl Model {
    fn open(dir: PathBuf, labels: Labels) -> Result<Self, String> {
        let listing = read_rows(&dir).map_err(|_| labels.read_error.clone())?;
        let mut model = Self {
            dir: dir.clone(),
            rows: Vec::new(),
            cursor: 0,
            filter: String::new(),
            filter_lower: String::new(),
            filtering: false,
            show_hidden: true,
            status: String::new(),
            preview: None,
            view_rows: 20,
            labels,
        };
        model.apply_listing(dir, listing);
        Ok(model)
    }

    fn apply_listing(&mut self, dir: PathBuf, listing: Listing) {
        self.dir = dir;
        self.rows = listing.rows;
        self.cursor = 0;
        self.filter.clear();
        self.filter_lower.clear();
        self.filtering = false;
        self.preview = None;
        self.status = if listing.truncated {
            self.labels.truncated.clone()
        } else {
            String::new()
        };
    }

    fn reload(&mut self) {
        match read_rows(&self.dir) {
            Ok(listing) => {
                let dir = self.dir.clone();
                self.apply_listing(dir, listing);
            }
            Err(_) => self.status = self.labels.read_error.clone(),
        }
    }

    fn visible(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.row_visible(entry))
            .map(|(index, _)| index)
            .collect()
    }

    fn row_visible(&self, entry: &Entry) -> bool {
        if entry.name == ".." {
            return true;
        }
        if !self.show_hidden && entry.name.starts_with('.') {
            return false;
        }
        self.filter_lower.is_empty() || entry.name.to_lowercase().contains(&self.filter_lower)
    }

    fn clamp_cursor(&mut self) {
        let len = self.visible().len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        let len = self.visible().len();
        if len == 0 {
            self.cursor = 0;
            return;
        }
        let next = (self.cursor as isize + delta).clamp(0, len as isize - 1);
        self.cursor = next as usize;
    }

    fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
            return true;
        }
        if self.preview.is_some() {
            return self.on_preview_key(code);
        }
        match code {
            KeyCode::Char('q') if !self.filtering => true,
            KeyCode::Up | KeyCode::Char('k') if !self.filtering => {
                self.move_cursor(-1);
                false
            }
            KeyCode::Down | KeyCode::Char('j') if !self.filtering => {
                self.move_cursor(1);
                false
            }
            KeyCode::PageUp => {
                let page = self.view_rows.max(1) as isize;
                self.move_cursor(-page);
                false
            }
            KeyCode::PageDown => {
                let page = self.view_rows.max(1) as isize;
                self.move_cursor(page);
                false
            }
            KeyCode::Home => {
                self.cursor = 0;
                false
            }
            KeyCode::End => {
                self.cursor = self.visible().len().saturating_sub(1);
                false
            }
            KeyCode::Enter => {
                self.open_selected();
                false
            }
            KeyCode::Backspace if self.filtering => {
                self.filter.pop();
                self.filter_lower = self.filter.to_lowercase();
                self.clamp_cursor();
                if self.filter.is_empty() {
                    self.filtering = false;
                }
                false
            }
            KeyCode::Backspace | KeyCode::Left => {
                self.go_parent();
                false
            }
            KeyCode::Char('/') => {
                self.filtering = true;
                false
            }
            KeyCode::Esc if self.filtering => {
                self.filtering = false;
                self.filter.clear();
                self.filter_lower.clear();
                self.clamp_cursor();
                false
            }
            KeyCode::Esc => false,
            KeyCode::Char('h') if !self.filtering => {
                self.show_hidden = !self.show_hidden;
                self.clamp_cursor();
                false
            }
            KeyCode::Char('r') if !self.filtering => {
                self.reload();
                false
            }
            KeyCode::Char(ch) if self.filtering && !mods.contains(KeyModifiers::CONTROL) => {
                self.filter.push(ch);
                self.filter_lower = self.filter.to_lowercase();
                self.clamp_cursor();
                false
            }
            _ => false,
        }
    }

    fn on_preview_key(&mut self, code: KeyCode) -> bool {
        let page = self.view_rows.max(1);
        match code {
            KeyCode::Char('q') => true,
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => {
                self.preview = None;
                false
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(preview) = self.preview.as_mut() {
                    preview.scroll = preview.scroll.saturating_sub(1);
                }
                false
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(preview) = self.preview.as_mut() {
                    let max = preview.lines.len().saturating_sub(page);
                    preview.scroll = (preview.scroll + 1).min(max);
                }
                false
            }
            KeyCode::PageUp => {
                if let Some(preview) = self.preview.as_mut() {
                    preview.scroll = preview.scroll.saturating_sub(page);
                }
                false
            }
            KeyCode::PageDown => {
                if let Some(preview) = self.preview.as_mut() {
                    let max = preview.lines.len().saturating_sub(page);
                    preview.scroll = (preview.scroll + page).min(max);
                }
                false
            }
            _ => false,
        }
    }

    fn open_selected(&mut self) {
        let visible = self.visible();
        let Some(index) = visible.get(self.cursor).copied() else {
            return;
        };
        let name = self.rows[index].name.clone();
        let is_dir = self.rows[index].dir;
        if name == ".." {
            self.go_parent();
            return;
        }
        let path = self.dir.join(&name);
        if is_dir {
            match read_rows(&path) {
                Ok(listing) => self.apply_listing(path, listing),
                Err(_) => self.status = self.labels.read_error.clone(),
            }
            return;
        }
        match load_preview(&path) {
            PreviewLoad::Text(lines) => {
                self.status.clear();
                self.preview = Some(Preview { lines, scroll: 0 });
            }
            PreviewLoad::Binary => self.status = self.labels.binary.clone(),
            PreviewLoad::TooBig => self.status = self.labels.too_big.clone(),
            PreviewLoad::Failed => self.status = self.labels.read_error.clone(),
        }
    }

    fn go_parent(&mut self) {
        let Some(parent) = parent_dir(&self.dir) else {
            return;
        };
        match read_rows(&parent) {
            Ok(listing) => self.apply_listing(parent, listing),
            Err(_) => self.status = self.labels.read_error.clone(),
        }
    }

    fn select_name(&mut self, name: &str) {
        let visible = self.visible();
        if let Some(pos) = visible
            .iter()
            .position(|index| self.rows[*index].name == name)
        {
            self.cursor = pos;
        }
    }
}

fn draw(frame: &mut Frame, model: &mut Model) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    model.view_rows = chunks[1].height as usize;
    let visible = model.visible();
    let shown = visible.len();
    let here = if shown == 0 { 0 } else { model.cursor + 1 };
    let header = format!(
        "{}  {}  {here}/{shown}",
        model.labels.app,
        model.dir.display()
    );
    frame.render_widget(Paragraph::new(header), chunks[0]);

    if let Some(preview) = model.preview.as_ref() {
        let text = preview.lines.join("\n");
        let scroll = u16::try_from(preview.scroll).unwrap_or(u16::MAX);
        frame.render_widget(Paragraph::new(text).scroll((scroll, 0)), chunks[1]);
    } else if visible.is_empty() {
        frame.render_widget(Paragraph::new(model.labels.empty.as_str()), chunks[1]);
    } else {
        let rows: Vec<Row> = visible
            .iter()
            .map(|index| {
                let entry = &model.rows[*index];
                Row::new([
                    Cell::from(display_name(entry)),
                    Cell::from(entry.size.map(format_bytes).unwrap_or_default()),
                    Cell::from(entry.modified.map(format_modified).unwrap_or_default()),
                ])
            })
            .collect();
        let header = Row::new([
            Cell::from(model.labels.col_name.as_str()),
            Cell::from(model.labels.col_size.as_str()),
            Cell::from(model.labels.col_modified.as_str()),
        ])
        .style(Style::new().add_modifier(Modifier::BOLD));
        let table = Table::new(
            rows,
            [
                Constraint::Min(20),
                Constraint::Length(10),
                Constraint::Length(16),
            ],
        )
        .header(header)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut state = TableState::default();
        state.select(Some(model.cursor.min(shown.saturating_sub(1))));
        frame.render_stateful_widget(table, chunks[1], &mut state);
    }

    let help = if model.preview.is_some() {
        model.labels.preview_help.as_str()
    } else {
        model.labels.help.as_str()
    };
    let mut lines = vec![Line::from(help)];
    if model.filtering {
        lines.push(Line::from(format!(
            "{}: {}",
            model.labels.filter, model.filter
        )));
    } else if !model.status.is_empty() {
        lines.push(Line::from(model.status.as_str()));
    }
    frame.render_widget(Paragraph::new(lines), chunks[2]);
}

fn display_name(entry: &Entry) -> String {
    if entry.name == ".." {
        return "..".to_string();
    }
    if entry.dir {
        format!("{}/", entry.name)
    } else if entry.symlink {
        format!("{}@", entry.name)
    } else {
        entry.name.clone()
    }
}

fn read_rows(dir: &Path) -> io::Result<Listing> {
    let mut rows = Vec::new();
    let mut truncated = false;
    for item in fs::read_dir(dir)? {
        if rows.len() >= LIST_CAP {
            truncated = true;
            break;
        }
        let Ok(item) = item else {
            continue;
        };
        let name = item.file_name().to_string_lossy().into_owned();
        if name == "." || name == ".." {
            continue;
        }
        let file_type = item.file_type().ok();
        let meta = item.metadata().ok();
        let symlink = file_type.as_ref().is_some_and(|kind| kind.is_symlink());
        let is_dir = file_type.as_ref().is_some_and(|kind| kind.is_dir())
            || meta.as_ref().is_some_and(|meta| meta.is_dir());
        let size = if is_dir {
            None
        } else {
            meta.as_ref().map(|meta| meta.len())
        };
        let modified = meta.as_ref().and_then(|meta| meta.modified().ok());
        rows.push(Entry {
            name,
            dir: is_dir,
            symlink,
            size,
            modified,
        });
    }
    rows.sort_by(|left, right| {
        right
            .dir
            .cmp(&left.dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    if parent_dir(dir).is_some() {
        rows.insert(
            0,
            Entry {
                name: "..".to_string(),
                dir: true,
                symlink: false,
                size: None,
                modified: None,
            },
        );
    }
    Ok(Listing { rows, truncated })
}

fn parent_dir(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    if parent.as_os_str().is_empty() {
        None
    } else {
        Some(parent.to_path_buf())
    }
}

fn resolve_start(requested: Option<&Path>, cwd: &Path) -> Result<PathBuf, String> {
    let raw = match requested {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => cwd.join(path),
        None => cwd.to_path_buf(),
    };
    let meta = fs::metadata(&raw).map_err(|err| format!("{}: {err}", raw.display()))?;
    if meta.is_dir() {
        Ok(raw)
    } else {
        raw.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("{}: not a folder", raw.display()))
    }
}

fn load_preview(path: &Path) -> PreviewLoad {
    let Ok(meta) = fs::metadata(path) else {
        return PreviewLoad::Failed;
    };
    if meta.len() > PREVIEW_CAP {
        return PreviewLoad::TooBig;
    }
    let Ok(bytes) = fs::read(path) else {
        return PreviewLoad::Failed;
    };
    if !looks_like_text(&bytes) {
        return PreviewLoad::Binary;
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines = text
        .lines()
        .take(PREVIEW_LINES)
        .map(ToOwned::to_owned)
        .collect();
    PreviewLoad::Text(lines)
}

#[must_use]
fn looks_like_text(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(4096)];
    if sample.contains(&0) {
        return false;
    }
    let bad = sample
        .iter()
        .filter(|byte| {
            let byte = **byte;
            byte < 0x09 || (byte > 0x0d && byte < 0x20)
        })
        .count();
    bad.saturating_mul(20) < sample.len().max(1)
}

#[must_use]
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_modified(time: SystemTime) -> String {
    let local: DateTime<Local> = time.into();
    local.format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orchid-tui-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn launch_parses_flag_path_and_equals_form() {
        assert!(tui_launch(["orchid", "note.txt"]).is_none());
        let bare = tui_launch(["orchid", "--tui"]).unwrap();
        assert!(bare.start.is_none());
        let path = tui_launch(["orchid", "--tui", r"C:\Temp"]).unwrap();
        assert_eq!(path.start.unwrap(), PathBuf::from(r"C:\Temp"));
        let equals = tui_launch(["orchid", "--tui=notes"]).unwrap();
        assert_eq!(equals.start.unwrap(), PathBuf::from("notes"));
        let empty = tui_launch(["orchid", "--tui="]).unwrap();
        assert!(empty.start.is_none());
        let next_flag = tui_launch(["orchid", "--tui", "--other"]).unwrap();
        assert!(next_flag.start.is_none());
    }

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
    }

    #[test]
    fn text_sniff_rejects_nul_and_accepts_utf8() {
        assert!(looks_like_text(b"hello\n"));
        assert!(looks_like_text("привет".as_bytes()));
        assert!(!looks_like_text(&[0, 1, 2, 3]));
        assert!(looks_like_text(b""));
    }

    #[test]
    fn resolve_start_uses_a_file_parent() {
        let dir = scratch("resolve");
        let file = dir.join("note.txt");
        fs::write(&file, b"hello").unwrap();
        let opened = resolve_start(Some(&file), &dir).unwrap();
        assert_eq!(opened, dir);
        let missing = resolve_start(Some(Path::new("no-such-orchid-tui")), &dir);
        assert!(missing.is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn browser_enters_a_folder_and_previews_text() {
        let dir = scratch("browse");
        let sub = dir.join("Sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("a.txt"), b"hello tui").unwrap();
        fs::write(dir.join(".secret"), b"hide").unwrap();
        fs::write(dir.join("blob.bin"), [0, 1, 2, 3]).unwrap();
        let mut model = Model::open(dir.clone(), Labels::english()).unwrap();
        assert!(model.visible().iter().any(|i| model.rows[*i].name == ".."));
        model.show_hidden = false;
        assert!(!model
            .visible()
            .iter()
            .any(|i| model.rows[*i].name == ".secret"));
        model.show_hidden = true;
        model.filtering = true;
        assert!(!model.on_key(KeyCode::Char('s'), KeyModifiers::NONE));
        assert!(!model.on_key(KeyCode::Char('u'), KeyModifiers::NONE));
        model.select_name("Sub");
        model.open_selected();
        assert_eq!(model.dir, sub);
        model.select_name("a.txt");
        model.open_selected();
        let preview = model.preview.as_ref().unwrap();
        assert_eq!(preview.lines, vec!["hello tui".to_string()]);
        assert!(!model.on_key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(model.preview.is_none());
        model.go_parent();
        assert_eq!(model.dir, dir);
        model.select_name("blob.bin");
        model.open_selected();
        assert!(model.preview.is_none());
        assert_eq!(model.status, model.labels.binary);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_text_is_not_loaded() {
        let dir = scratch("big");
        let file = dir.join("big.txt");
        fs::write(&file, vec![b'a'; (PREVIEW_CAP as usize) + 1]).unwrap();
        let mut model = Model::open(dir.clone(), Labels::english()).unwrap();
        model.select_name("big.txt");
        model.open_selected();
        assert!(model.preview.is_none());
        assert_eq!(model.status, model.labels.too_big);
        let _ = fs::remove_dir_all(&dir);
    }
}
