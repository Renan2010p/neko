//! Godot's CodeEdit: a multi-line monospace text editor with a line number
//! gutter, syntax colors (from a highlighter the app provides), current line
//! highlight, auto-indent, brace completion, code completion, comment toggling
//! and undo/redo.
//!
//! The app keeps a [`CodeHandle`] (shared with the control) to read and change
//! the text; [`crate::UiEvent::TextChanged`] reports edits.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use neko_platform::{Key, Modifiers};
use neko_render::epaint::{ClippedShape, Color32, CornerRadius, Shape};

use crate::control::{Control, Image, Kind};
use crate::{Id, Pos2, Rect, Ui, UiEvent, pos2, vec2};

/// Colored ranges of one line: (first char, end char, color).
pub type Spans = Vec<(usize, usize, Color32)>;

/// `(line, state at line start) -> (spans, state at line end)`. The state
/// carries multi-line constructs (e.g. a `"""` string) to the next line.
pub type Highlighter = Rc<dyn Fn(&str, u32) -> (Spans, u32)>;

/// The kind of a code completion entry (Godot's `CodeCompletionKind`).
/// It picks the icon and the order in the popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Class,
    Enum,
    Function,
    Member,
    Variable,
    Constant,
    Signal,
    Keyword,
    Annotation,
    /// A `res://` path (Godot's CODE_COMPLETION_KIND_FILE_PATH).
    FilePath,
    /// A `$Node/Path` (Godot's CODE_COMPLETION_KIND_NODE_PATH).
    NodePath,
    Plain,
}

/// One entry of the code completion popup (Godot's
/// `ScriptLanguage::CodeCompletionOption`).
#[derive(Clone, Debug)]
pub struct CompletionOption {
    /// Text shown in the list.
    pub display: String,
    /// Text inserted on Enter/Tab/click; the prefix is replaced.
    pub insert: String,
    pub kind: CompletionKind,
    pub icon: Option<Image>,
    /// Godot's `CodeCompletionLocation`: 0 is local, 1 an inherited member.
    pub location: u8,
}

impl CompletionOption {
    pub fn new(display: impl Into<String>, kind: CompletionKind) -> Self {
        let display = display.into();
        Self { insert: display.clone(), display, kind, icon: None, location: 0 }
    }

    pub fn insert(mut self, text: impl Into<String>) -> Self {
        self.insert = text.into();
        self
    }

    pub fn icon(mut self, icon: Option<Image>) -> Self {
        self.icon = icon;
        self
    }

    pub fn location(mut self, location: u8) -> Self {
        self.location = location;
        self
    }
}

/// A position in the text: line and column, both 0-based, in chars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextPos {
    pub line: usize,
    pub col: usize,
}

impl TextPos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// CodeEdit theme colors (Godot's `text_editor/theme/highlighting/*`).
#[derive(Clone, Copy, Debug)]
pub struct CodeColors {
    pub background: Color32,
    pub text: Color32,
    pub line_number: Color32,
    pub current_line: Color32,
    pub selection: Color32,
    pub caret: Color32,
    pub mark: Color32,
    pub word_highlighted: Color32,
}

impl CodeColors {
    /// Godot's defaults for a dark editor theme.
    pub fn from_theme(t: &crate::Theme) -> Self {
        let a = |c: Color32, f: f32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * f) as u8);
        let white = |alpha: f32| Color32::from_rgba_unmultiplied(255, 255, 255, (alpha * 255.0) as u8);
        Self {
            background: t.code_background,
            text: t.font_color,
            line_number: a(t.font_color, 0.5),
            current_line: white(0.07),
            selection: Color32::from_rgba_unmultiplied(t.accent.r(), t.accent.g(), t.accent.b(), 102),
            caret: Color32::WHITE,
            mark: Color32::from_rgba_unmultiplied(255, 120, 107, 77),
            word_highlighted: white(0.07),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum EditKind {
    Insert,
    Delete,
    Other,
}

struct Snap {
    lines: Vec<String>,
    caret: TextPos,
    anchor: Option<TextPos>,
}

/// (first char, char count) runs matched by the typed prefix.
type MatchRuns = Vec<(usize, usize)>;
/// Godot's lexicographic ranking (fewer segments, prefix at 0, exact case,
/// location, first match index).
type CompletionRank = (usize, u8, usize, u8, usize);

/// One filtered completion entry, with the display characters to highlight.
struct CompletionMatch {
    option: CompletionOption,
    ranges: MatchRuns,
    score: CompletionRank,
}

/// The open code completion popup (Godot's `code_completion_*` in `CodeEdit`).
struct Completion {
    /// Every option the app provided for the current context.
    sources: Vec<CompletionOption>,
    /// Where the typed prefix starts; confirming replaces from here.
    base: TextPos,
    /// Ctrl+Space shows the list even when the context would not.
    forced: bool,
    matches: Vec<CompletionMatch>,
    selected: usize,
    /// Top visible row and row count, from the last paint.
    scroll: usize,
    lines: usize,
    rect: Rect,
    scroll_rect: Rect,
    hovered: Option<usize>,
}

/// Measurements from the last paint, used by input handling.
#[derive(Clone, Copy, Default)]
pub(crate) struct Metrics {
    pub char_w: f32,
    pub line_h: f32,
    /// Left edge of column 0, before horizontal scrolling.
    pub text_left: f32,
    pub top: f32,
    pub visible_lines: usize,
}

pub struct CodeState {
    lines: Vec<String>,
    caret: TextPos,
    /// Other end of the selection, if any.
    anchor: Option<TextPos>,
    scroll_x: f32,
    scroll_y: f32,
    /// Visual column kept while moving up and down.
    want_vcol: Option<usize>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    last_edit: Option<(EditKind, Instant)>,
    version: u64,
    highlighter: Option<Highlighter>,
    hl_cache: Vec<Spans>,
    hl_version: Option<u64>,
    /// A line drawn with the error mark (Godot's `mark_color`), 0-based.
    marked_line: Option<usize>,
    pub colors: Option<CodeColors>,
    pub indent_size: usize,
    pub indent_with_spaces: bool,
    pub auto_brace: bool,
    pub font_size: f32,
    pub line_spacing: f32,
    clipboard_out: Option<String>,
    ensure_visible: bool,
    center_caret: bool,
    /// The open completion popup, if any.
    completion: Option<Completion>,
    /// Whether the last edit inserted text (Godot only auto-completes then).
    last_was_insert: bool,
    pub(crate) metrics: Metrics,
}

impl Default for CodeState {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            caret: TextPos::default(),
            anchor: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            want_vcol: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            version: 0,
            highlighter: None,
            hl_cache: Vec::new(),
            hl_version: None,
            marked_line: None,
            colors: None,
            // Godot's defaults: tabs, size 4, auto brace completion on,
            // code font size 14 and line spacing 4.
            indent_size: 4,
            indent_with_spaces: false,
            auto_brace: true,
            font_size: 14.0,
            line_spacing: 4.0,
            clipboard_out: None,
            ensure_visible: false,
            center_caret: false,
            completion: None,
            last_was_insert: false,
            metrics: Metrics::default(),
        }
    }
}

const UNDO_LIMIT: usize = 200;
const MERGE_TIME: Duration = Duration::from_millis(1200);
/// Godot's completion style box is empty, so rows fill its rect exactly.
const COMPLETION_PADDING: f32 = 0.0;
/// Godot's `completion_lines` theme constant.
const COMPLETION_LINES: usize = 7;

fn byte_of(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map_or(s.len(), |(b, _)| b)
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn closing_of(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    }
}

/// A single lowercase char, enough for the ASCII identifiers GDScript uses.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Godot's fuzzy match: the prefix chars in order, greedily extended into
/// contiguous runs. One candidate per occurrence of the first char; the
/// caller keeps the best one.
fn subsequence_runs(needle: &[char], hay: &[char], start: usize) -> Option<Vec<(usize, usize)>> {
    let mut pos = start;
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (k, &want) in needle.iter().enumerate() {
        if k > 0
            && pos < hay.len()
            && lower(hay[pos]) == lower(want)
            && ranges.last().is_some_and(|&(at, len)| at + len == pos)
        {
            if let Some(last) = ranges.last_mut() {
                last.1 += 1;
            }
            pos += 1;
            continue;
        }
        while pos < hay.len() && lower(hay[pos]) != lower(want) {
            pos += 1;
        }
        if pos >= hay.len() {
            return None;
        }
        ranges.push((pos, 1));
        pos += 1;
    }
    Some(ranges)
}

/// The best subsequence runs for `prefix` in `display`, and Godot's ranking.
fn fuzzy_match(prefix: &[char], display: &[char], location: u8) -> Option<(MatchRuns, CompletionRank)> {
    if prefix.is_empty() {
        return Some((Vec::new(), (0, 1, 0, location, 0)));
    }
    let mut best: Option<(MatchRuns, CompletionRank)> = None;
    for (i, &c) in display.iter().enumerate() {
        if lower(c) != lower(prefix[0]) {
            continue;
        }
        let Some(runs) = subsequence_runs(prefix, display, i) else { continue };
        // Rank like Godot's CodeCompletionOption::get_option_characteristics.
        let mut bad_case = 0;
        let mut pi = 0;
        for &(at, len) in &runs {
            for j in 0..len {
                if display[at + j] != prefix[pi] {
                    bad_case += 1;
                }
                pi += 1;
            }
        }
        let first = runs[0].0;
        let score = (runs.len(), u8::from(first != 0), bad_case, location, first);
        if best.as_ref().is_none_or(|(_, b)| score < *b) {
            best = Some((runs, score));
        }
    }
    best
}

impl CodeState {
    // ------------------------------------------------------------ queries

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn line(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", String::as_str)
    }

    fn line_len(&self, i: usize) -> usize {
        char_len(self.line(i))
    }

    fn char_at(&self, p: TextPos) -> Option<char> {
        self.line(p.line).chars().nth(p.col)
    }

    fn char_before(&self, p: TextPos) -> Option<char> {
        if p.col == 0 { None } else { self.line(p.line).chars().nth(p.col - 1) }
    }

    /// The ordered, non-empty selection.
    pub fn selection(&self) -> Option<(TextPos, TextPos)> {
        let a = self.anchor?;
        if a == self.caret {
            return None;
        }
        Some(if a < self.caret { (a, self.caret) } else { (self.caret, a) })
    }

    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection()?;
        Some(self.range_text(a, b))
    }

    fn range_text(&self, a: TextPos, b: TextPos) -> String {
        if a.line == b.line {
            let l = self.line(a.line);
            return l[byte_of(l, a.col)..byte_of(l, b.col)].to_string();
        }
        let mut out = String::new();
        let first = self.line(a.line);
        out.push_str(&first[byte_of(first, a.col)..]);
        for i in a.line + 1..b.line {
            out.push('\n');
            out.push_str(self.line(i));
        }
        out.push('\n');
        let last = self.line(b.line);
        out.push_str(&last[..byte_of(last, b.col)]);
        out
    }

    /// Visual column of `col` on `line`, with tabs expanded.
    fn vcol(&self, line: usize, col: usize) -> usize {
        let mut v = 0;
        for c in self.line(line).chars().take(col) {
            v = if c == '\t' { (v / self.indent_size + 1) * self.indent_size } else { v + 1 };
        }
        v
    }

    fn col_from_vcol(&self, line: usize, target: usize) -> usize {
        let mut v = 0;
        for (i, c) in self.line(line).chars().enumerate() {
            let next = if c == '\t' { (v / self.indent_size + 1) * self.indent_size } else { v + 1 };
            if target < next {
                // Closer to this char's start or its end?
                return if target - v <= (next - v) / 2 { i } else { i + 1 };
            }
            v = next;
        }
        self.line_len(line)
    }

    fn clamp(&self, p: TextPos) -> TextPos {
        let line = p.line.min(self.lines.len() - 1);
        TextPos::new(line, p.col.min(self.line_len(line)))
    }

    fn indent_unit(&self) -> String {
        if self.indent_with_spaces { " ".repeat(self.indent_size) } else { "\t".into() }
    }

    fn leading_ws(&self, line: usize) -> String {
        self.line(line).chars().take_while(|c| *c == ' ' || *c == '\t').collect()
    }

    // ------------------------------------------------------------ editing core

    fn snapshot(&mut self, kind: EditKind) {
        let now = Instant::now();
        let merge = kind != EditKind::Other && self.last_edit.is_some_and(|(k, t)| k == kind && now - t < MERGE_TIME);
        if !merge {
            self.undo.push(Snap { lines: self.lines.clone(), caret: self.caret, anchor: self.anchor });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some((kind, now));
        self.last_was_insert = false;
    }

    fn changed(&mut self) {
        self.version += 1;
        self.ensure_visible = true;
        self.want_vcol = None;
    }

    fn remove_range(&mut self, a: TextPos, b: TextPos) {
        if a.line == b.line {
            let l = &mut self.lines[a.line];
            let (x, y) = (byte_of(l, a.col), byte_of(l, b.col));
            l.replace_range(x..y, "");
        } else {
            let tail = {
                let last = &self.lines[b.line];
                last[byte_of(last, b.col)..].to_string()
            };
            let first = &mut self.lines[a.line];
            let x = byte_of(first, a.col);
            first.truncate(x);
            first.push_str(&tail);
            self.lines.drain(a.line + 1..=b.line);
        }
        self.caret = a;
        self.anchor = None;
    }

    fn delete_selection(&mut self) -> bool {
        match self.selection() {
            Some((a, b)) => {
                self.remove_range(a, b);
                true
            }
            None => {
                self.anchor = None;
                false
            }
        }
    }

    /// Insert `text` (may contain newlines) at the caret.
    fn insert_raw(&mut self, text: &str) {
        let p = self.caret;
        let line = &self.lines[p.line];
        let at = byte_of(line, p.col);
        let tail = line[at..].to_string();
        let head = line[..at].to_string();
        let parts: Vec<&str> = text.split('\n').collect();
        if parts.len() == 1 {
            self.lines[p.line] = format!("{head}{text}{tail}");
            self.caret = TextPos::new(p.line, p.col + char_len(text));
        } else {
            let last = parts.len() - 1;
            self.lines[p.line] = format!("{head}{}", parts[0]);
            let mut new_lines: Vec<String> = parts[1..last].iter().map(|s| s.to_string()).collect();
            new_lines.push(format!("{}{tail}", parts[last]));
            let at = p.line + 1;
            self.lines.splice(at..at, new_lines);
            self.caret = TextPos::new(p.line + last, char_len(parts[last]));
        }
        self.anchor = None;
    }

    // ------------------------------------------------------- code completion

    /// Replace the app-provided completion options. `base` is where the typed
    /// prefix starts (Godot's `code_completion_base`); `forced` (Ctrl+Space)
    /// keeps the list open even without a prefix.
    pub fn update_completion(&mut self, options: Vec<CompletionOption>, base: TextPos, forced: bool) {
        if options.is_empty() {
            self.completion = None;
            return;
        }
        // Godot keys options by display text, the last one winning.
        let mut dedup: std::collections::HashMap<String, CompletionOption> = std::collections::HashMap::new();
        for o in options {
            if !o.display.is_empty() {
                dedup.insert(o.display.clone(), o);
            }
        }
        let mut sources: Vec<CompletionOption> = dedup.into_values().collect();
        sources.sort_by_key(|o| o.display.to_lowercase());
        self.completion = Some(Completion {
            sources,
            base: self.clamp(base),
            forced,
            matches: Vec::new(),
            selected: 0,
            scroll: 0,
            lines: 0,
            rect: Rect::NOTHING,
            scroll_rect: Rect::NOTHING,
            hovered: None,
        });
        self.refilter_completion();
    }

    pub fn cancel_completion(&mut self) {
        self.completion = None;
    }

    pub fn completion_active(&self) -> bool {
        self.completion.as_ref().is_some_and(|c| !c.matches.is_empty())
    }

    /// The text between the popup base and the caret.
    fn completion_prefix(&self) -> String {
        let Some(c) = &self.completion else { return String::new() };
        if c.base.line != self.caret.line || c.base.col > self.caret.col {
            return String::new();
        }
        let line = self.line(self.caret.line);
        line[byte_of(line, c.base.col)..byte_of(line, self.caret.col)].to_string()
    }

    fn refilter_completion(&mut self) {
        // The caret moved before the prefix (or to another line): the context
        // the app provided no longer applies.
        if let Some(c) = &self.completion
            && (c.base.line != self.caret.line || c.base.col > self.caret.col)
        {
            self.completion = None;
            return;
        }
        let prefix = self.completion_prefix();
        let Some(c) = &mut self.completion else { return };
        let pre: Vec<char> = prefix.chars().collect();
        let mut matches = Vec::new();
        for option in &c.sources {
            let display: Vec<char> = option.display.chars().collect();
            if let Some((ranges, score)) = fuzzy_match(&pre, &display, option.location) {
                matches.push(CompletionMatch { option: option.clone(), ranges, score });
            }
        }
        matches.sort_by(|a, b| {
            a.score.cmp(&b.score).then_with(|| {
                // Natural, case-insensitive order like Godot's last tie-break.
                a.option.display.to_lowercase().cmp(&b.option.display.to_lowercase())
            })
        });
        // A single perfect match has nothing to choose.
        if matches.len() == 1 && matches[0].option.display == prefix && !c.forced {
            self.completion = None;
            return;
        }
        if matches.is_empty() {
            self.completion = None;
            return;
        }
        if c.selected >= matches.len() {
            c.selected = 0;
        }
        c.matches = matches;
    }

    /// Enter/Tab/click: replace the prefix with the selected option.
    fn confirm_completion(&mut self) {
        let Some(c) = self.completion.take() else { return };
        let Some(m) = c.matches.get(c.selected) else { return };
        let (base, insert) = (c.base, m.option.insert.clone());
        self.snapshot(EditKind::Other);
        if base.line == self.caret.line && base.col <= self.caret.col {
            let line = &mut self.lines[base.line];
            let (a, b) = (byte_of(line, base.col), byte_of(line, self.caret.col));
            line.replace_range(a..b, "");
            self.caret = TextPos::new(base.line, base.col);
        }
        self.anchor = None;
        self.insert_raw(&insert);
        // Godot's auto brace completion: `foo(` also gets its `)`.
        if self.auto_brace
            && insert.ends_with('(')
            && self.char_at(self.caret).is_none_or(|c| !matches!(c, ')' | ']' | '}'))
        {
            self.insert_raw(")");
            self.caret.col -= 1;
        }
        self.last_was_insert = true;
        self.changed();
    }

    /// Mouse press in the popup: select the row, double click confirms.
    /// Returns true when the popup took the click.
    pub(crate) fn completion_click(&mut self, p: Pos2, double: bool) -> bool {
        let Some(c) = &mut self.completion else { return false };
        if c.scroll_rect.contains(p) {
            if !c.matches.is_empty() {
                let f = ((p.y - c.scroll_rect.top()) / c.scroll_rect.height()).clamp(0.0, 1.0);
                c.selected = (f * (c.matches.len() - 1) as f32).round() as usize;
            }
            return true;
        }
        if !c.rect.contains(p) {
            return false;
        }
        let row = ((p.y - c.rect.top() - COMPLETION_PADDING) / self.metrics.line_h.max(1.0)).floor();
        if row >= 0.0 {
            let i = c.scroll + row as usize;
            if i < c.matches.len() {
                c.selected = i;
            }
        }
        if double {
            self.confirm_completion();
        }
        true
    }

    /// Wheel over the popup: move the selected row. Returns true if used.
    pub(crate) fn completion_wheel(&mut self, p: Pos2, delta: f32) -> bool {
        let Some(c) = &mut self.completion else { return false };
        if !c.rect.contains(p) {
            return false;
        }
        let n = c.matches.len();
        if n == 0 {
            return true;
        }
        let step = delta.round() as isize;
        c.selected = (c.selected as isize + step).clamp(0, n as isize - 1) as usize;
        true
    }

    // ------------------------------------------------------------ public edits

    pub fn set_text(&mut self, text: &str) {
        self.lines = text.replace("\r\n", "\n").split('\n').map(str::to_string).collect();
        self.caret = TextPos::default();
        self.anchor = None;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        self.completion = None;
        self.last_was_insert = false;
        self.version += 1;
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else { return false };
        self.redo.push(Snap { lines: std::mem::replace(&mut self.lines, s.lines), caret: self.caret, anchor: self.anchor });
        self.caret = self.clamp(s.caret);
        self.anchor = s.anchor.map(|a| self.clamp(a));
        self.last_edit = None;
        self.completion = None;
        self.last_was_insert = false;
        self.changed();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else { return false };
        self.undo.push(Snap { lines: std::mem::replace(&mut self.lines, s.lines), caret: self.caret, anchor: self.anchor });
        self.caret = self.clamp(s.caret);
        self.anchor = s.anchor.map(|a| self.clamp(a));
        self.last_edit = None;
        self.completion = None;
        self.last_was_insert = false;
        self.changed();
        true
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(TextPos::default());
        let last = self.lines.len() - 1;
        self.caret = TextPos::new(last, self.line_len(last));
    }

    pub fn select(&mut self, from: TextPos, to: TextPos) {
        self.anchor = Some(self.clamp(from));
        self.caret = self.clamp(to);
        self.ensure_visible = true;
        self.center_caret = true;
        self.completion = None;
    }

    /// Copy the selection, or the whole line without one (Godot does the same).
    pub fn copy(&mut self) {
        let text = self.selected_text().unwrap_or_else(|| format!("{}\n", self.line(self.caret.line)));
        self.clipboard_out = Some(text);
    }

    pub fn cut(&mut self) {
        self.copy();
        self.snapshot(EditKind::Other);
        self.last_was_insert = false;
        if !self.delete_selection() {
            self.delete_lines();
            return;
        }
        self.changed();
    }

    /// Paste or typed text: replaces the selection.
    pub fn paste(&mut self, text: &str) {
        self.snapshot(EditKind::Other);
        self.delete_selection();
        self.insert_raw(&text.replace("\r\n", "\n").replace('\r', "\n"));
        self.last_was_insert = true;
        self.changed();
        if self.completion.is_some() {
            self.refilter_completion();
        }
    }

    /// Lines covered by the selection, or the caret's line.
    fn selected_lines(&self) -> (usize, usize) {
        match self.selection() {
            // A selection ending at column 0 does not include that line.
            Some((a, b)) => (a.line, if b.col == 0 && b.line > a.line { b.line - 1 } else { b.line }),
            None => (self.caret.line, self.caret.line),
        }
    }

    /// Godot's "Toggle Comment" (Ctrl+K): `#` at column 0 of each line.
    pub fn toggle_comment(&mut self) {
        let (a, b) = self.selected_lines();
        let all = (a..=b).filter(|&i| !self.line(i).trim().is_empty()).all(|i| self.line(i).starts_with('#'));
        self.snapshot(EditKind::Other);
        for i in a..=b {
            if all {
                if self.lines[i].starts_with('#') {
                    self.lines[i].remove(0);
                    self.shift_cols(i, -1);
                }
            } else {
                self.lines[i].insert(0, '#');
                self.shift_cols(i, 1);
            }
        }
        self.changed();
    }

    /// Move the caret and anchor on `line` by `d` columns (after inserting or removing at column 0).
    fn shift_cols(&mut self, line: usize, d: isize) {
        let fix = |p: &mut TextPos, len: usize| {
            if p.line == line {
                p.col = (p.col as isize + d).max(0) as usize;
                p.col = p.col.min(len);
            }
        };
        let len = self.line_len(line);
        fix(&mut self.caret, len);
        if let Some(a) = &mut self.anchor {
            fix(a, len);
        }
    }

    pub fn indent(&mut self) {
        let (a, b) = self.selected_lines();
        let unit = self.indent_unit();
        let n = char_len(&unit) as isize;
        self.snapshot(EditKind::Other);
        for i in a..=b {
            self.lines[i].insert_str(0, &unit);
            self.shift_cols(i, n);
        }
        self.changed();
    }

    pub fn unindent(&mut self) {
        let (a, b) = self.selected_lines();
        self.snapshot(EditKind::Other);
        for i in a..=b {
            let l = &self.lines[i];
            let n = if l.starts_with('\t') {
                1
            } else {
                l.chars().take(self.indent_size).take_while(|c| *c == ' ').count()
            };
            if n > 0 {
                self.lines[i].replace_range(0..n, "");
                self.shift_cols(i, -(n as isize));
            }
        }
        self.changed();
    }

    /// Godot's "Delete Line" (Ctrl+Shift+K).
    pub fn delete_lines(&mut self) {
        let (a, b) = self.selected_lines();
        self.snapshot(EditKind::Other);
        if self.lines.len() == b - a + 1 {
            self.lines = vec![String::new()];
            self.caret = TextPos::default();
        } else {
            self.lines.drain(a..=b);
            let line = a.min(self.lines.len() - 1);
            self.caret = TextPos::new(line, self.caret.col.min(self.line_len(line)));
        }
        self.anchor = None;
        self.changed();
    }

    /// Godot's "Duplicate Selection" (Ctrl+Shift+D): the selection, or the lines.
    pub fn duplicate(&mut self) {
        self.snapshot(EditKind::Other);
        if let Some((a, b)) = self.selection() {
            let text = self.range_text(a, b);
            self.caret = b;
            self.anchor = None;
            self.insert_raw(&text);
            self.anchor = Some(b);
        } else {
            let (a, b) = self.selected_lines();
            let copy: Vec<String> = self.lines[a..=b].to_vec();
            let n = copy.len();
            self.lines.splice(b + 1..b + 1, copy);
            self.caret.line += n;
        }
        self.changed();
    }

    /// Godot's "Move Up/Down" (Alt+Up / Alt+Down).
    pub fn move_lines(&mut self, down: bool) {
        let (a, b) = self.selected_lines();
        if (!down && a == 0) || (down && b + 1 >= self.lines.len()) {
            return;
        }
        self.snapshot(EditKind::Other);
        if down {
            let l = self.lines.remove(b + 1);
            self.lines.insert(a, l);
        } else {
            let l = self.lines.remove(a - 1);
            self.lines.insert(b, l);
        }
        let d: isize = if down { 1 } else { -1 };
        self.caret.line = (self.caret.line as isize + d) as usize;
        if let Some(an) = &mut self.anchor {
            an.line = (an.line as isize + d) as usize;
        }
        self.changed();
    }

    /// Find `needle` after (or before) the caret; selects it. Wraps around.
    pub fn find(&mut self, needle: &str, forward: bool, match_case: bool) -> bool {
        if needle.is_empty() {
            return false;
        }
        let fold = |s: &str| if match_case { s.to_string() } else { s.to_lowercase() };
        let needle_f = fold(needle);
        let n = self.lines.len();
        let (start, end) = self.selection().unwrap_or((self.caret, self.caret));
        let from = if forward { end } else { start };
        for step in 0..=n {
            let i = if forward { (from.line + step) % n } else { (from.line + n - step % n) % n };
            let hay = fold(self.line(i));
            let found = if forward {
                let skip = if step == 0 { byte_of(&hay, from.col) } else { 0 };
                hay[skip..].find(&needle_f).map(|b| b + skip)
            } else {
                let lim = if step == 0 { byte_of(&hay, from.col) } else { hay.len() };
                hay[..lim].rfind(&needle_f)
            };
            if let Some(b) = found {
                let col = char_len(&hay[..b]);
                self.select(TextPos::new(i, col), TextPos::new(i, col + char_len(needle)));
                return true;
            }
        }
        false
    }

    /// How many times `needle` occurs.
    pub fn count(&self, needle: &str, match_case: bool) -> usize {
        if needle.is_empty() {
            return 0;
        }
        let n = if match_case { needle.to_string() } else { needle.to_lowercase() };
        self.lines.iter().map(|l| if match_case { l.matches(&n).count() } else { l.to_lowercase().matches(&n).count() }).sum()
    }

    pub fn go_to_line(&mut self, line: usize) {
        let line = line.min(self.lines.len() - 1);
        let col = char_len(&self.leading_ws(line));
        self.caret = TextPos::new(line, col);
        self.anchor = None;
        self.ensure_visible = true;
        self.center_caret = true;
    }

    // ------------------------------------------------------------ typing

    /// A typed character (Event::Text), with brace completion.
    fn type_char(&mut self, c: char) {
        let next = self.char_at(self.caret);
        if self.auto_brace && self.selection().is_none() {
            // Typing the closing char that was auto-inserted moves over it.
            if matches!(c, ')' | ']' | '}' | '"' | '\'') && next == Some(c) {
                self.caret.col += 1;
                self.ensure_visible = true;
                return;
            }
        }
        self.snapshot(if c.is_whitespace() { EditKind::Other } else { EditKind::Insert });
        self.last_was_insert = true;
        if self.auto_brace
            && let Some(close) = closing_of(c)
        {
            if let Some((a, b)) = self.selection() {
                // Wrap the selection.
                let inner = self.range_text(a, b);
                self.remove_range(a, b);
                self.insert_raw(&format!("{c}{inner}{close}"));
                self.anchor = Some(TextPos::new(a.line, a.col + 1));
                self.caret.col -= 1;
                self.changed();
                return;
            }
            let quote = c == '"' || c == '\'';
            let prev = self.char_before(self.caret);
            let next_ok = next.is_none_or(|n| n.is_whitespace() || matches!(n, ')' | ']' | '}' | ',' | ':' | ';'));
            if next_ok && !(quote && prev.is_some_and(is_word)) {
                self.insert_raw(&format!("{c}{close}"));
                self.caret.col -= 1;
                self.changed();
                return;
            }
        }
        self.delete_selection();
        self.insert_raw(c.encode_utf8(&mut [0; 4]));
        self.changed();
    }

    /// Enter: keep the indentation, add a level after `:` or an open bracket.
    fn newline(&mut self) {
        self.snapshot(EditKind::Other);
        self.last_was_insert = true;
        self.delete_selection();
        let p = self.caret;
        let indent = self.leading_ws(p.line);
        let line = self.line(p.line).to_string();
        let before = &line[..byte_of(&line, p.col)];
        let code = before.split('#').next().unwrap_or("").trim_end();
        let prev = before.trim_end().chars().last();
        let next = self.char_at(p);
        let unit = self.indent_unit();
        if let (Some(o), Some(n)) = (prev, next)
            && closing_of(o) == Some(n)
            && matches!(o, '(' | '[' | '{')
        {
            // Between brackets: the closing one goes to its own line.
            self.insert_raw(&format!("\n{indent}{unit}\n{indent}"));
            self.caret = TextPos::new(p.line + 1, char_len(&indent) + char_len(&unit));
        } else if code.ends_with(':') || code.ends_with('(') || code.ends_with('[') || code.ends_with('{') {
            self.insert_raw(&format!("\n{indent}{unit}"));
        } else {
            self.insert_raw(&format!("\n{indent}"));
        }
        self.changed();
    }

    fn backspace(&mut self, word: bool) {
        if self.selection().is_some() {
            self.snapshot(EditKind::Other);
            self.delete_selection();
            self.changed();
            return;
        }
        let p = self.caret;
        if p.col == 0 && p.line == 0 {
            return;
        }
        self.snapshot(EditKind::Delete);
        let from = if p.col == 0 {
            TextPos::new(p.line - 1, self.line_len(p.line - 1))
        } else if word {
            TextPos::new(p.line, self.word_left(p).col)
        } else {
            TextPos::new(p.line, p.col - 1)
        };
        let mut to = p;
        if !word
            && self.auto_brace
            && let (Some(o), Some(n)) = (self.char_before(p), self.char_at(p))
            && closing_of(o) == Some(n)
        {
            to.col += 1;
        }
        self.remove_range(from, to);
        self.changed();
    }

    fn delete_forward(&mut self, word: bool) {
        if self.selection().is_some() {
            self.snapshot(EditKind::Other);
            self.delete_selection();
            self.changed();
            return;
        }
        let p = self.caret;
        let len = self.line_len(p.line);
        if p.col >= len && p.line + 1 >= self.lines.len() {
            return;
        }
        self.snapshot(EditKind::Delete);
        let to = if p.col >= len { TextPos::new(p.line + 1, 0) } else if word { self.word_right(p) } else { TextPos::new(p.line, p.col + 1) };
        self.remove_range(p, to);
        self.changed();
    }

    // ------------------------------------------------------------ movement

    fn word_left(&self, p: TextPos) -> TextPos {
        if p.col == 0 {
            return if p.line == 0 { p } else { TextPos::new(p.line - 1, self.line_len(p.line - 1)) };
        }
        let chars: Vec<char> = self.line(p.line).chars().collect();
        let mut i = p.col;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 && is_word(chars[i - 1]) {
            while i > 0 && is_word(chars[i - 1]) {
                i -= 1;
            }
        } else {
            i = i.saturating_sub(1);
        }
        TextPos::new(p.line, i)
    }

    fn word_right(&self, p: TextPos) -> TextPos {
        let chars: Vec<char> = self.line(p.line).chars().collect();
        if p.col >= chars.len() {
            return if p.line + 1 < self.lines.len() { TextPos::new(p.line + 1, 0) } else { p };
        }
        let mut i = p.col;
        if is_word(chars[i]) {
            while i < chars.len() && is_word(chars[i]) {
                i += 1;
            }
        } else if !chars[i].is_whitespace() {
            i += 1;
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        TextPos::new(p.line, i)
    }

    /// The word around `p` (double click).
    fn word_at(&self, p: TextPos) -> (TextPos, TextPos) {
        let chars: Vec<char> = self.line(p.line).chars().collect();
        let mut a = p.col.min(chars.len());
        let mut b = a;
        while a > 0 && is_word(chars[a - 1]) {
            a -= 1;
        }
        while b < chars.len() && is_word(chars[b]) {
            b += 1;
        }
        if a == b && b < chars.len() {
            b += 1;
        }
        (TextPos::new(p.line, a), TextPos::new(p.line, b))
    }

    fn move_to(&mut self, to: TextPos, select: bool, keep_vcol: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = self.clamp(to);
        if !keep_vcol {
            self.want_vcol = None;
        }
        self.ensure_visible = true;
        self.last_edit = None;
        self.completion = None;
    }

    fn vertical(&mut self, lines: isize, select: bool) {
        let want = self.want_vcol.unwrap_or_else(|| self.vcol(self.caret.line, self.caret.col));
        let target = self.caret.line as isize + lines;
        let to = if target < 0 {
            TextPos::new(0, 0)
        } else if target as usize >= self.lines.len() {
            let l = self.lines.len() - 1;
            TextPos::new(l, self.line_len(l))
        } else {
            let l = target as usize;
            TextPos::new(l, self.col_from_vcol(l, want))
        };
        self.move_to(to, select, true);
        self.want_vcol = Some(want);
    }

    /// Editing and navigation keys. Returns (used, text changed).
    pub(crate) fn key(&mut self, key: Key, m: Modifiers) -> (bool, bool) {
        let cmd = m.ctrl || m.logo;
        let v0 = self.version;
        let p = self.caret;
        let sel = m.shift;
        // The completion popup owns Up/Down/Page/Enter/Tab/Escape while open.
        if self.completion.as_ref().is_some_and(|c| !c.matches.is_empty()) {
            let n = self.completion.as_ref().map_or(0, |c| c.matches.len());
            match key {
                Key::ArrowUp if !m.alt => {
                    if let Some(c) = &mut self.completion {
                        c.selected = if c.selected > 0 { c.selected - 1 } else { n - 1 };
                    }
                    return (true, false);
                }
                Key::ArrowDown if !m.alt => {
                    if let Some(c) = &mut self.completion {
                        c.selected = if c.selected + 1 < n { c.selected + 1 } else { 0 };
                    }
                    return (true, false);
                }
                Key::PageUp => {
                    if let Some(c) = &mut self.completion {
                        c.selected = c.selected.saturating_sub(COMPLETION_LINES);
                    }
                    return (true, false);
                }
                Key::PageDown => {
                    if let Some(c) = &mut self.completion {
                        c.selected = (c.selected + COMPLETION_LINES).min(n - 1);
                    }
                    return (true, false);
                }
                Key::Enter => {
                    self.confirm_completion();
                    return (true, self.version != v0);
                }
                Key::Tab if !m.shift && !cmd => {
                    self.confirm_completion();
                    return (true, self.version != v0);
                }
                Key::Escape => {
                    self.completion = None;
                    return (true, false);
                }
                _ => {}
            }
        }
        match key {
            Key::ArrowLeft if m.alt => return (false, false),
            Key::ArrowLeft => {
                if !sel && let Some((a, _)) = self.selection() {
                    self.move_to(a, false, false);
                } else if cmd {
                    self.move_to(self.word_left(p), sel, false);
                } else if p.col > 0 {
                    self.move_to(TextPos::new(p.line, p.col - 1), sel, false);
                } else if p.line > 0 {
                    self.move_to(TextPos::new(p.line - 1, self.line_len(p.line - 1)), sel, false);
                }
            }
            Key::ArrowRight if m.alt => return (false, false),
            Key::ArrowRight => {
                if !sel && let Some((_, b)) = self.selection() {
                    self.move_to(b, false, false);
                } else if cmd {
                    self.move_to(self.word_right(p), sel, false);
                } else if p.col < self.line_len(p.line) {
                    self.move_to(TextPos::new(p.line, p.col + 1), sel, false);
                } else if p.line + 1 < self.lines.len() {
                    self.move_to(TextPos::new(p.line + 1, 0), sel, false);
                }
            }
            Key::ArrowUp if m.alt => self.move_lines(false),
            Key::ArrowDown if m.alt => self.move_lines(true),
            Key::ArrowUp => self.vertical(-1, sel),
            Key::ArrowDown => self.vertical(1, sel),
            Key::PageUp => self.vertical(-(self.metrics.visible_lines.max(2) as isize - 1), sel),
            Key::PageDown => self.vertical(self.metrics.visible_lines.max(2) as isize - 1, sel),
            Key::Home if cmd => self.move_to(TextPos::default(), sel, false),
            Key::End if cmd => {
                let l = self.lines.len() - 1;
                self.move_to(TextPos::new(l, self.line_len(l)), sel, false);
            }
            Key::Home => {
                // Smart home: first non-blank column, then column 0.
                let first = char_len(&self.leading_ws(p.line));
                let col = if p.col == first { 0 } else { first };
                self.move_to(TextPos::new(p.line, col), sel, false);
            }
            Key::End => self.move_to(TextPos::new(p.line, self.line_len(p.line)), sel, false),
            Key::Backspace => self.backspace(cmd),
            Key::Delete => self.delete_forward(cmd),
            Key::Enter => self.newline(),
            Key::Tab if m.shift => self.unindent(),
            Key::Tab if !cmd => {
                let multi = self.selection().is_some_and(|(a, b)| a.line != b.line);
                if multi {
                    self.indent();
                } else {
                    self.snapshot(EditKind::Other);
                    self.delete_selection();
                    let unit = self.indent_unit();
                    self.insert_raw(&unit);
                    self.changed();
                }
            }
            Key::Char('a') if cmd => self.select_all(),
            Key::Char('c') if cmd && !m.shift && !m.alt => self.copy(),
            Key::Char('x') if cmd && !m.shift => self.cut(),
            Key::Char('z') if cmd && m.shift => {
                self.redo();
            }
            Key::Char('z') if cmd => {
                self.undo();
            }
            Key::Char('y') if cmd => {
                self.redo();
            }
            Key::Char('k') if cmd && m.shift => self.delete_lines(),
            Key::Char('k') if cmd => self.toggle_comment(),
            Key::Char('d') if cmd && m.shift => self.duplicate(),
            // Paste arrives as Event::Text from the app.
            Key::Char('v') if cmd => return (false, false),
            // Other shortcuts belong to the app (save, find...).
            _ if cmd || m.alt => return (false, false),
            Key::Char(_) | Key::Space => {}
            Key::Escape | Key::F(_) | Key::Insert | Key::Unknown => return (false, false),
            Key::Tab => return (false, false),
        }
        // Typing/backspacing filters the open list (Godot's Backspace path).
        if self.version != v0 && self.completion.is_some() {
            self.refilter_completion();
        }
        (true, self.version != v0)
    }

    /// Typed or pasted text.
    pub(crate) fn text_input(&mut self, t: &str) -> bool {
        let v0 = self.version;
        // Enter and Tab are handled as keys; a lone control char here is their echo.
        if t.chars().count() == 1 {
            let c = t.chars().next().expect("one char");
            if c.is_control() {
                return false;
            }
            self.type_char(c);
        } else {
            let clean: String = t.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t' || *c == '\r').collect();
            self.paste(&clean);
        }
        if self.version != v0 && self.completion.is_some() {
            self.refilter_completion();
        }
        self.version != v0
    }

    // ------------------------------------------------------------ mouse

    pub(crate) fn pos_at(&self, p: Pos2) -> TextPos {
        let m = self.metrics;
        if m.line_h <= 0.0 {
            return self.caret;
        }
        let y = p.y - m.top + self.scroll_y;
        let line = if y < 0.0 { 0 } else { ((y / m.line_h) as usize).min(self.lines.len() - 1) };
        let x = (p.x - m.text_left + self.scroll_x).max(0.0);
        let v = (x / m.char_w + 0.5) as usize;
        TextPos::new(line, self.col_from_vcol(line, v))
    }

    pub(crate) fn click(&mut self, p: Pos2, extend: bool, double: bool) {
        self.completion = None;
        let at = self.pos_at(p);
        if double {
            let (a, b) = self.word_at(at);
            self.anchor = Some(a);
            self.caret = b;
        } else {
            self.move_to(at, extend, false);
        }
        self.last_edit = None;
    }

    pub(crate) fn drag_to(&mut self, p: Pos2) {
        self.completion = None;
        let at = self.pos_at(p);
        if self.anchor.is_none() {
            self.anchor = Some(self.caret);
        }
        self.caret = at;
        self.ensure_visible = true;
    }

    pub(crate) fn wheel(&mut self, dx: f32, dy: f32) {
        let m = self.metrics;
        self.scroll_y -= dy * m.line_h * 3.0;
        self.scroll_x -= dx * m.char_w * 6.0;
        self.clamp_scroll(None);
    }

    fn clamp_scroll(&mut self, view: Option<(f32, f32)>) {
        let m = self.metrics;
        let (w, h) = view.unwrap_or((f32::MAX, m.visible_lines as f32 * m.line_h));
        let max_y = (self.lines.len() as f32 * m.line_h - h + m.line_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_y);
        let longest = (0..self.lines.len()).map(|i| self.vcol(i, usize::MAX)).max().unwrap_or(0);
        let max_x = if w == f32::MAX { f32::MAX } else { (longest as f32 * m.char_w - w + m.char_w * 4.0).max(0.0) };
        self.scroll_x = self.scroll_x.clamp(0.0, max_x);
    }

    // ------------------------------------------------------------ highlighting

    fn highlight(&mut self) {
        if self.hl_version == Some(self.version) {
            return;
        }
        self.hl_version = Some(self.version);
        self.hl_cache.clear();
        let Some(h) = self.highlighter.clone() else { return };
        let mut state = 0;
        for l in &self.lines {
            let (spans, out) = h(l, state);
            self.hl_cache.push(spans);
            state = out;
        }
    }
}

/// The app's side of a CodeEdit, shared with the control.
#[derive(Clone, Default)]
pub struct CodeHandle(pub(crate) Rc<RefCell<CodeState>>);

impl CodeHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read or change the editor state directly.
    pub fn with<R>(&self, f: impl FnOnce(&mut CodeState) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }

    pub fn text(&self) -> String {
        self.0.borrow().text()
    }

    /// Replace the text (clears undo history and moves to the top).
    pub fn set_text(&self, text: &str) {
        self.0.borrow_mut().set_text(text);
    }

    /// Changes on every edit; compare to detect modifications.
    pub fn version(&self) -> u64 {
        self.0.borrow().version
    }

    /// Caret line and column, 0-based.
    pub fn caret(&self) -> TextPos {
        self.0.borrow().caret
    }

    pub fn line_count(&self) -> usize {
        self.0.borrow().lines.len()
    }

    /// One line of the text, without its newline.
    pub fn line_text(&self, line: usize) -> String {
        self.0.borrow().line(line).to_string()
    }

    /// Whether the last edit inserted text (Godot auto-completes only then).
    pub fn last_was_insert(&self) -> bool {
        self.0.borrow().last_was_insert
    }

    /// Show the completion popup for `options`; `base` is where the typed
    /// prefix starts. `forced` is Ctrl+Space.
    pub fn update_completion(&self, options: Vec<CompletionOption>, base: TextPos, forced: bool) {
        self.0.borrow_mut().update_completion(options, base, forced);
    }

    pub fn cancel_completion(&self) {
        self.0.borrow_mut().cancel_completion();
    }

    pub fn completion_active(&self) -> bool {
        self.0.borrow().completion_active()
    }

    pub fn set_highlighter(&self, h: Option<Highlighter>) {
        let mut s = self.0.borrow_mut();
        s.highlighter = h;
        s.hl_version = None;
    }

    pub fn set_marked_line(&self, line: Option<usize>) {
        self.0.borrow_mut().marked_line = line;
    }

    pub fn go_to_line(&self, line: usize) {
        self.0.borrow_mut().go_to_line(line);
    }

    /// Text copied or cut since the last call, for the system clipboard.
    pub fn take_clipboard(&self) -> Option<String> {
        self.0.borrow_mut().clipboard_out.take()
    }
}

impl Control {
    /// Godot's CodeEdit, editing the text in `handle`.
    pub fn code_edit(handle: &CodeHandle) -> Self {
        Control::new(Kind::CodeEdit(handle.clone())).expand()
    }
}

// ---------------------------------------------------------------- Ui glue

impl Ui {
    fn focused_code(&self) -> Option<(Id, CodeHandle)> {
        let f = self.focus?;
        match &self.get(f)?.kind {
            Kind::CodeEdit(h) if !self.get(f)?.disabled => Some((f, h.clone())),
            _ => None,
        }
    }

    pub(crate) fn code_key(&mut self, key: Key, m: Modifiers) -> bool {
        let Some((id, h)) = self.focused_code() else { return false };
        let (used, changed) = h.0.borrow_mut().key(key, m);
        if changed {
            self.events.push(UiEvent::TextChanged(id));
        }
        used
    }

    pub(crate) fn code_text(&mut self, t: &str) -> bool {
        let Some((id, h)) = self.focused_code() else { return false };
        if h.0.borrow_mut().text_input(t) {
            self.events.push(UiEvent::TextChanged(id));
        }
        true
    }

    /// Mouse press on a CodeEdit: focus, place the caret, start a drag
    /// selection. Returns true when the completion popup used the click.
    pub(crate) fn code_press(&mut self, id: Id, h: &CodeHandle, p: Pos2, double: bool) -> bool {
        self.focus = Some(id);
        if h.0.borrow_mut().completion_click(p, double) {
            return true;
        }
        let extend = self.modifiers.shift;
        h.0.borrow_mut().click(p, extend, double);
        false
    }

    pub(crate) fn paint_code(&mut self, id: Id, c: &Control, h: &CodeHandle, clip: Rect, out: &mut Vec<ClippedShape>) {
        let r = c.rect;
        let focused = self.focus == Some(id);
        let mut s = h.0.borrow_mut();
        let colors = s.colors.unwrap_or_else(|| CodeColors::from_theme(&self.theme));
        let fs = s.font_size;
        let m_g = self.text.layout_mono("M", fs, Color32::WHITE);
        let char_w = m_g.size().x.max(1.0);
        let line_h = m_g.size().y + s.line_spacing;
        let digits = s.lines.len().to_string().len().max(3);
        let gutter = digits as f32 * char_w + 20.0;
        let pad_top = 4.0;
        let text_left = r.left() + gutter + 6.0;
        let text_w = (r.right() - text_left - 10.0).max(1.0);
        s.metrics = Metrics { char_w, line_h, text_left, top: r.top() + pad_top, visible_lines: ((r.height() - pad_top) / line_h).floor().max(1.0) as usize };

        // Keep the caret on screen after it moved.
        if s.ensure_visible {
            s.ensure_visible = false;
            let cy = s.caret.line as f32 * line_h;
            let view_h = r.height() - pad_top - line_h;
            if s.center_caret {
                s.center_caret = false;
                if cy < s.scroll_y || cy > s.scroll_y + view_h {
                    s.scroll_y = cy - view_h / 2.0;
                }
            } else if cy < s.scroll_y {
                s.scroll_y = cy;
            } else if cy > s.scroll_y + view_h {
                s.scroll_y = cy - view_h;
            }
            let cx = s.vcol(s.caret.line, s.caret.col) as f32 * char_w;
            if cx < s.scroll_x {
                s.scroll_x = (cx - char_w * 4.0).max(0.0);
            } else if cx > s.scroll_x + text_w {
                s.scroll_x = cx - text_w + char_w * 4.0;
            }
        }
        s.clamp_scroll(Some((text_w, r.height() - pad_top)));
        s.highlight();

        let push = |out: &mut Vec<ClippedShape>, cl: Rect, shape: Shape| out.push(ClippedShape::new(cl, shape));
        let area = clip.intersect(r);
        push(out, area, Shape::rect_filled(r, CornerRadius::same(self.theme.corner_radius), colors.background));
        let text_clip = area.intersect(Rect::from_min_max(pos2(r.left() + gutter, r.top()), r.max));

        let first = (s.scroll_y / line_h).floor().max(0.0) as usize;
        let count = (r.height() / line_h).ceil() as usize + 1;
        let sel = s.selection();
        let caret = s.caret;
        let x0 = text_left - s.scroll_x;
        for i in first..(first + count).min(s.lines.len()) {
            let y = r.top() + pad_top + i as f32 * line_h - s.scroll_y;
            let row = Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), line_h));
            if s.marked_line == Some(i) {
                push(out, area, Shape::rect_filled(row, CornerRadius::ZERO, colors.mark));
            } else if i == caret.line && sel.is_none() {
                push(out, area, Shape::rect_filled(row, CornerRadius::ZERO, colors.current_line));
            }
            // Selection on this line.
            if let Some((a, b)) = sel
                && i >= a.line
                && i <= b.line
            {
                let va = if i == a.line { s.vcol(i, a.col) } else { 0 };
                let vb = if i == b.line { s.vcol(i, b.col) } else { s.vcol(i, usize::MAX) + 1 };
                if vb > va {
                    let sr = Rect::from_min_max(pos2(x0 + va as f32 * char_w, y), pos2(x0 + vb as f32 * char_w, y + line_h));
                    push(out, text_clip, Shape::rect_filled(sr, CornerRadius::ZERO, colors.selection));
                }
            }
            // Line number, right-aligned in the gutter.
            let num_color = if i == caret.line { colors.text } else { colors.line_number };
            let g = self.text.layout_mono(&(i + 1).to_string(), fs, num_color);
            let nx = r.left() + gutter - 8.0 - g.size().x;
            push(out, area, Shape::galley(pos2(nx, y + s.line_spacing / 2.0), g, num_color));

            // Text, in runs of one color, tabs expanded.
            let spans = s.hl_cache.get(i);
            let mut run = String::new();
            let mut run_v = 0usize;
            let mut run_color = colors.text;
            let mut v = 0usize;
            let color_at = |ci: usize| spans.and_then(|sp| sp.iter().rev().find(|(a, b, _)| ci >= *a && ci < *b)).map_or(colors.text, |(_, _, c)| *c);
            let line = s.line(i).to_string();
            let max_v = ((s.scroll_x + text_w) / char_w) as usize + 2;
            let min_v = (s.scroll_x / char_w) as usize;
            let flush = |run: &mut String, run_v: usize, color: Color32, out: &mut Vec<ClippedShape>, text: &mut crate::TextServer| {
                if run.is_empty() {
                    return;
                }
                let end_v = run_v + char_len(run);
                if end_v >= min_v && run_v <= max_v {
                    let g = text.layout_mono(run, fs, color);
                    out.push(ClippedShape::new(text_clip, Shape::galley(pos2(x0 + run_v as f32 * char_w, y + s.line_spacing / 2.0), g, color)));
                }
                run.clear();
            };
            let mut prev_punct = false;
            for (ci, ch) in line.chars().enumerate() {
                let col = color_at(ci);
                // Punctuation is drawn on its own so the font cannot join it into
                // coding ligatures ("->" as an arrow): Godot disables them by default.
                let punct = ch.is_ascii_punctuation();
                if col != run_color || ch == '\t' || punct || prev_punct {
                    flush(&mut run, run_v, run_color, out, &mut self.text);
                    run_color = col;
                    run_v = v;
                }
                prev_punct = punct;
                if ch == '\t' {
                    v = (v / s.indent_size + 1) * s.indent_size;
                    run_v = v;
                } else {
                    if run.is_empty() {
                        run_v = v;
                    }
                    run.push(ch);
                    v += 1;
                }
                if v > max_v {
                    break;
                }
            }
            flush(&mut run, run_v, run_color, out, &mut self.text);

            if focused && i == caret.line {
                let cx = x0 + s.vcol(i, caret.col) as f32 * char_w;
                let cr = Rect::from_min_size(pos2(cx, y + 1.0), vec2(2.0, line_h - 2.0));
                push(out, text_clip, Shape::rect_filled(cr, CornerRadius::ZERO, colors.caret));
            }
        }

        // Vertical scroll bar thumb (Godot's VScrollBar, without dragging).
        let content = s.lines.len() as f32 * line_h + r.height() - line_h;
        if content > r.height() + 1.0 {
            let frac = r.height() / content;
            let th = (r.height() * frac).max(24.0);
            let ty = r.top() + (r.height() - th) * (s.scroll_y / (content - r.height()).max(1.0));
            let thumb = Rect::from_min_size(pos2(r.right() - 7.0, ty), vec2(5.0, th));
            push(out, area, Shape::rect_filled(thumb, CornerRadius::same(3), Color32::from_rgba_unmultiplied(255, 255, 255, 40)));
        }

        // Code completion (Godot draws it inside the CodeEdit, so it is
        // clipped to the editor area and flips above the caret near the bottom).
        if s.completion.as_ref().is_some_and(|c| !c.matches.is_empty()) {
            let caret_pos = s.caret;
            let base = s.completion.as_ref().map(|c| c.base).unwrap_or(caret_pos);
            let base_v = if base.line == caret_pos.line { s.vcol(base.line, base.col) } else { s.vcol(caret_pos.line, caret_pos.col) };
            let caret_y = r.top() + pad_top + caret_pos.line as f32 * line_h - s.scroll_y;
            let below = caret_y + line_h + s.line_spacing / 2.0;
            let scroll_x = s.scroll_x;
            let mouse = self.mouse;

            let total = s.completion.as_ref().map_or(0, |c| c.matches.len());
            let lines = total.min(COMPLETION_LINES);
            let icon_w = line_h;
            let scroll_w = if total > lines { 6.0 } else { 0.0 };
            let mut longest = 0.0_f32;
            if let Some(c) = &s.completion {
                for m in &c.matches {
                    longest = longest.max(self.text.layout_mono(&m.option.display, fs, colors.text).size().x);
                }
            }
            let width = (longest.min(char_w * 56.0) + icon_w + 6.0 + COMPLETION_PADDING * 2.0).min(r.width()) + scroll_w;
            let height = lines as f32 * line_h + COMPLETION_PADDING * 2.0;
            let x = (text_left - scroll_x + base_v as f32 * char_w).min(r.right() - width - 2.0).max(r.left() + 2.0);
            let y = if below + height <= r.bottom() - 2.0 { below } else { (caret_y - height).max(r.top() + 2.0) };
            let popup_rect = Rect::from_min_size(pos2(x, y), vec2(width, height));
            let first = {
                let selected = s.completion.as_ref().map_or(0, |c| c.selected);
                selected.saturating_sub(lines / 2).min(total.saturating_sub(lines))
            };
            let hovered = mouse
                .filter(|m| popup_rect.contains(*m))
                .map(|m| ((m.y - popup_rect.top() - COMPLETION_PADDING) / line_h).floor())
                .filter(|row| *row >= 0.0)
                .map(|row| first + row as usize)
                .filter(|i| *i < total);

            // Godot draws `completion_background_color` as a plain rect, with
            // no style box border or rounded corners.
            push(out, area, Shape::rect_filled(popup_rect, CornerRadius::ZERO, Color32::from_rgb(43, 41, 51)));
            let row_w = width - COMPLETION_PADDING * 2.0 - scroll_w;
            for row in 0..lines {
                let idx = first + row;
                let m = &s.completion.as_ref().expect("checked").matches[idx];
                let row_rect = Rect::from_min_size(pos2(popup_rect.left() + COMPLETION_PADDING, popup_rect.top() + COMPLETION_PADDING + row as f32 * line_h), vec2(row_w, line_h));
                let selected = idx == s.completion.as_ref().map_or(0, |c| c.selected);
                if selected {
                    // Godot's CodeEdit `completion_selected_color` (0.26, 0.26, 0.27).
                    push(out, area, Shape::rect_filled(row_rect, CornerRadius::ZERO, Color32::from_rgb(66, 66, 69)));
                } else if hovered == Some(idx) {
                    push(out, area, Shape::rect_filled(row_rect, CornerRadius::ZERO, self.theme.item_hover));
                }
                // Highlight the typed characters inside the entry.
                let text_x = row_rect.left() + icon_w + 4.0;
                for &(at, len) in &m.ranges {
                    let hr = Rect::from_min_size(pos2(text_x + at as f32 * char_w, row_rect.top() + 1.0), vec2(len as f32 * char_w, line_h - 2.0));
                    push(out, area, Shape::rect_filled(hr, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(255, 255, 255, 33)));
                }
                if let Some(img) = m.option.icon {
                    let ir = Rect::from_center_size(pos2(row_rect.left() + icon_w / 2.0, row_rect.center().y), vec2(icon_w * 0.7, line_h * 0.7));
                    push(out, area, Shape::image(img.texture, ir, img.uv, img.tint));
                }
                let color = if selected { self.theme.font_focus_color } else { colors.text };
                let g = self.text.layout_mono(&m.option.display, fs, color);
                push(out, area, Shape::galley(pos2(text_x, row_rect.center().y - g.size().y / 2.0), g, color));
            }
            let scroll_rect = if scroll_w > 0.0 {
                let track = Rect::from_min_size(pos2(popup_rect.right() - scroll_w, popup_rect.top() + COMPLETION_PADDING), vec2(scroll_w, height - COMPLETION_PADDING * 2.0));
                let frac = lines as f32 / total as f32;
                let thumb_h = (track.height() * frac).max(10.0);
                let max_first = (total - lines).max(1) as f32;
                let ty = track.top() + (track.height() - thumb_h) * (first as f32 / max_first);
                let thumb = Rect::from_min_size(pos2(track.left(), ty), vec2(scroll_w, thumb_h));
                push(out, area, Shape::rect_filled(thumb, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(255, 255, 255, 70)));
                track
            } else {
                Rect::NOTHING
            };
            if let Some(c) = &mut s.completion {
                c.rect = popup_rect;
                c.scroll_rect = scroll_rect;
                c.scroll = first;
                c.lines = lines;
                c.hovered = hovered;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(text: &str) -> CodeState {
        let mut s = CodeState::default();
        s.set_text(text);
        s
    }

    fn type_str(s: &mut CodeState, t: &str) {
        for c in t.chars() {
            if c == '\n' {
                s.newline();
            } else {
                s.type_char(c);
            }
        }
    }

    #[test]
    fn auto_indent_after_colon_and_brace_pairs() {
        let mut s = state("");
        type_str(&mut s, "func _ready():\nprint(\"hi\")");
        assert_eq!(s.text(), "func _ready():\n\tprint(\"hi\")");
        // Typing the closing chars moved over the auto-inserted ones.
        assert_eq!(s.caret, TextPos::new(1, 12));
    }

    #[test]
    fn undo_redo_groups_typing() {
        let mut s = state("a");
        s.caret = TextPos::new(0, 1);
        type_str(&mut s, "bcd");
        assert_eq!(s.text(), "abcd");
        s.undo();
        assert_eq!(s.text(), "a");
        s.redo();
        assert_eq!(s.text(), "abcd");
    }

    #[test]
    fn comment_indent_and_lines() {
        let mut s = state("a\nb\nc");
        s.select(TextPos::new(0, 0), TextPos::new(1, 1));
        s.toggle_comment();
        assert_eq!(s.text(), "#a\n#b\nc");
        s.toggle_comment();
        assert_eq!(s.text(), "a\nb\nc");
        s.indent();
        assert_eq!(s.text(), "\ta\n\tb\nc");
        s.unindent();
        assert_eq!(s.text(), "a\nb\nc");
        s.anchor = None;
        s.caret = TextPos::new(2, 0);
        s.move_lines(false);
        assert_eq!(s.text(), "a\nc\nb");
        s.delete_lines();
        assert_eq!(s.text(), "a\nb");
    }

    #[test]
    fn find_wraps_and_tabs_expand() {
        let mut s = state("\tvar x = 1\nvar y = x");
        assert!(s.find("x", true, true));
        assert_eq!(s.selection(), Some((TextPos::new(0, 5), TextPos::new(0, 6))));
        assert!(s.find("x", true, true));
        assert_eq!(s.caret.line, 1);
        assert!(s.find("x", true, true));
        assert_eq!(s.caret.line, 0);
        assert_eq!(s.count("var", true), 2);
        assert_eq!(s.vcol(0, 1), 4);
        assert_eq!(s.col_from_vcol(0, 4), 1);
    }

    #[test]
    fn completion_filters_and_confirms() {
        let mut s = state("pr");
        s.caret = TextPos::new(0, 2);
        s.update_completion(
            vec![
                CompletionOption::new("print(…)", CompletionKind::Function).insert("print("),
                CompletionOption::new("printer", CompletionKind::Variable),
                CompletionOption::new("quit()", CompletionKind::Function).insert("quit()"),
            ],
            TextPos::new(0, 0),
            false,
        );
        assert!(s.completion_active());
        assert_eq!(s.completion.as_ref().unwrap().matches.len(), 2);
        s.confirm_completion();
        // The auto-inserted closing brace lands after the caret.
        assert_eq!(s.text(), "print()");
        assert_eq!(s.caret, TextPos::new(0, 6));
    }

    #[test]
    fn completion_opens_on_demand_and_closes_on_exact_match() {
        let mut s = state("");
        s.update_completion(vec![CompletionOption::new("print", CompletionKind::Function)], TextPos::default(), true);
        assert!(s.completion_active());
        // The same list on an exact prefix has nothing left to pick.
        let mut s = state("print");
        s.caret = TextPos::new(0, 5);
        s.update_completion(vec![CompletionOption::new("print", CompletionKind::Function)], TextPos::default(), false);
        assert!(!s.completion_active());
    }

    #[test]
    fn auto_brace_keeps_the_caret_inside_a_string() {
        let mut s = state("");
        for c in "preload(\"res://pla".chars() {
            s.type_char(c);
        }
        assert_eq!(s.text(), "preload(\"res://pla\")");
        assert_eq!(s.caret, TextPos::new(0, 18));
    }

    #[test]
    fn completion_keys_and_click() {
        let items = || vec![CompletionOption::new("pause", CompletionKind::Function), CompletionOption::new("player", CompletionKind::Variable), CompletionOption::new("print", CompletionKind::Function)];
        let mut s = state("");
        s.update_completion(items(), TextPos::default(), true);
        assert_eq!(s.completion.as_ref().unwrap().selected, 0);
        assert!(s.key(Key::ArrowDown, Modifiers::default()).0);
        assert_eq!(s.completion.as_ref().unwrap().selected, 1);
        let (used, changed) = s.key(Key::Enter, Modifiers::default());
        assert!(used && changed);
        assert_eq!(s.text(), "player");

        // A double click on a row confirms it; a single click only selects.
        let mut s = state("");
        s.update_completion(items(), TextPos::default(), true);
        s.metrics.line_h = 20.0;
        if let Some(c) = &mut s.completion {
            c.rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0));
            c.scroll = 0;
        }
        let row = |i: f32| pos2(30.0, COMPLETION_PADDING + i * 20.0 + 10.0);
        assert!(s.completion_click(row(2.0), false));
        assert_eq!(s.completion.as_ref().unwrap().selected, 2);
        assert!(s.completion_click(row(2.0), true));
        assert_eq!(s.text(), "print");
        assert!(!s.completion_active());
    }

    #[test]
    fn backspace_removes_pairs_and_joins_lines() {
        let mut s = state("");
        type_str(&mut s, "(");
        assert_eq!(s.text(), "()");
        s.backspace(false);
        assert_eq!(s.text(), "");
        let mut s = state("ab\ncd");
        s.caret = TextPos::new(1, 0);
        s.backspace(false);
        assert_eq!(s.text(), "abcd");
        assert_eq!(s.caret, TextPos::new(0, 2));
    }
}
