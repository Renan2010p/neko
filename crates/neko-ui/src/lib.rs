//! # neko-ui
//!
//! Neko's own GUI, modeled on Godot's: a retained tree of [`Control`] nodes,
//! containers that lay out their children (the algorithms follow Godot's
//! `BoxContainer::_resort` and `Container::fit_child_in_rect`), a [`Theme`]
//! of StyleBoxes, and events the app reacts to (Godot's signals).
//!
//! The output is tessellated meshes for any [`neko_render::Renderer`], so the
//! same UI draws with wgpu or the CPU renderer, in the editor or in a game.

mod code_edit;
mod control;
mod geom;
mod paint;
pub mod splash;
mod windows;

pub use windows::WindowInfo;
mod text;
mod theme;
mod widgets;

use std::time::{Duration, Instant};

pub use control::{Align, Anchors, Control, DrawFn, Id, Image, Kind, Menu, MenuItem, MouseFilter, SizeFlags, Tab, TextEdit, TreeRow};
pub use widgets::CanvasInput;
pub use code_edit::{CodeColors, CodeHandle, CodeState, CompletionKind, CompletionOption, Highlighter, Spans, TextPos};
pub use geom::{Pos2, Rect, Sides, Vec2, pos2, vec2};
pub use neko_render::epaint::emath::Align2;
pub use neko_render::epaint::{Color32, ColorImage, CornerRadius, FontFamily, FontId, Mesh, Shape, Stroke, StrokeKind, TextureId};
pub use paint::Painter;
pub use text::{TextServer, bold, default_fonts};
pub use theme::{StateStyles, StyleBox, Theme};

use neko_platform::{CursorIcon, Event, Key, Modifiers, MouseButton, WheelUnit};
use neko_render::epaint::textures::TexturesDelta;
use neko_render::epaint::{
    ClippedPrimitive, ClippedShape, Primitive, TessellationOptions, Tessellator,
};

/// Texture ids are unique per process, so several `Ui`s (project manager,
/// editor, splash) can share one renderer without overwriting each other.
static NEXT_TEXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0x4E45_4B4F_0000);

pub(crate) fn next_texture_id() -> TextureId {
    TextureId::User(NEXT_TEXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

pub(crate) const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
const CHECK_SIZE: f32 = 16.0;
const ICON_SIZE: f32 = 16.0;

/// Something happened that the app may react to (Godot's signals).
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    /// A button or clickable control was clicked.
    Pressed(Id),
    DoubleClicked(Id),
    /// A toggle button, check box, radio or switch changed.
    Toggled(Id, bool),
    TextChanged(Id),
    /// Enter was pressed in a LineEdit.
    TextSubmitted(Id),
    ItemSelected(Id, usize),
    /// A key no focused control used (shortcuts, Escape for dialogs...).
    Key { key: Key, modifiers: Modifiers },
    /// A PopupMenu item was chosen (its `action`).
    Menu(u32),
    TabSelected(Id, usize),
    TabClosed(Id, usize),
    TabAdded(Id),
    /// A SpinBox changed (while dragging or after typing).
    ValueChanged(Id, f64),
    TreeSelected(Id, u64),
    /// Double-click or Enter on a row.
    TreeActivated(Id, u64),
    /// A row's icon button was clicked: (tree, row key, button index).
    TreeButton(Id, u64, usize),
    /// The fold arrow was clicked; the app updates the rows.
    TreeToggled(Id, u64, bool),
    /// Right-click on a row (or on empty space when the key is `None`).
    TreeContext(Id, Option<u64>, Pos2),
    TreeDropped { tree: Id, dragged: u64, target: u64 },
    /// In-place rename finished with Enter or by clicking elsewhere.
    TreeEdited(Id, u64, String),
    Canvas(Id, CanvasInput),
    RightClicked(Id, Pos2),
    PopupClosed(Id),
}

/// What the app hands to the renderer. Clear `textures` after rendering.
pub struct UiOutput {
    pub primitives: Vec<ClippedPrimitive>,
    pub textures: TexturesDelta,
    pub cursor: CursorIcon,
    /// When the UI wants to be drawn again without input (tooltips).
    pub repaint_after: Option<Duration>,
}

struct Slot {
    generation: u32,
    node: Option<Control>,
    min: Vec2,
}

pub(crate) struct Dropdown {
    owner: Id,
    rect: Rect,
    hovered: Option<usize>,
}

pub struct Ui {
    slots: Vec<Slot>,
    free: Vec<u32>,
    root: Id,
    pub theme: Theme,
    text: TextServer,
    ppp: f32,
    screen: Rect,
    mouse: Option<Pos2>,
    modifiers: Modifiers,
    hovered: Option<Id>,
    hover_since: Instant,
    pressed: Option<Id>,
    focus: Option<Id>,
    last_click: Option<(Id, Instant)>,
    dropdown: Option<Dropdown>,
    events: Vec<UiEvent>,
    menus: Vec<widgets::OpenMenu>,
    menubar_open: Option<(Id, usize)>,
    drag: Option<widgets::Drag>,
    /// Sub-rects of composite widgets (menu titles, tabs) from the last paint.
    hit_rects: std::collections::HashMap<Id, Vec<Rect>>,
    pending_textures: Vec<(TextureId, neko_render::epaint::ImageDelta)>,
    font_texture: TextureId,
    /// Dialogs become OS windows (see `windows.rs`).
    native_windows: bool,
    windows: Vec<windows::WinReg>,
    ctxs: std::collections::HashMap<Id, windows::Ctx>,
    /// The native window whose context is swapped in.
    active_window: Option<Id>,
    /// The native window that produced the latest events.
    event_window: Option<Id>,
}

impl Ui {
    pub fn new(theme: Theme) -> Self {
        let mut ui = Self {
            slots: Vec::new(),
            free: Vec::new(),
            root: Id(0, 0),
            theme,
            text: TextServer::new(),
            ppp: 1.0,
            screen: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            mouse: None,
            modifiers: Modifiers::default(),
            hovered: None,
            hover_since: Instant::now(),
            pressed: None,
            focus: None,
            last_click: None,
            dropdown: None,
            events: Vec::new(),
            menus: Vec::new(),
            menubar_open: None,
            drag: None,
            hit_rects: Default::default(),
            pending_textures: Vec::new(),
            font_texture: next_texture_id(),
            native_windows: false,
            windows: Vec::new(),
            ctxs: Default::default(),
            active_window: None,
            event_window: None,
        };
        let mut root = Control::control();
        root.mouse = MouseFilter::Ignore;
        ui.root = ui.alloc(root);
        ui
    }

    // ------------------------------------------------------------------ tree

    fn alloc(&mut self, c: Control) -> Id {
        if let Some(i) = self.free.pop() {
            let s = &mut self.slots[i as usize];
            s.node = Some(c);
            Id(i, s.generation)
        } else {
            self.slots.push(Slot { generation: 0, node: Some(c), min: Vec2::ZERO });
            Id(self.slots.len() as u32 - 1, 0)
        }
    }

    pub fn root(&self) -> Id {
        self.root
    }

    pub fn get(&self, id: Id) -> Option<&Control> {
        self.slots.get(id.0 as usize).filter(|s| s.generation == id.1).and_then(|s| s.node.as_ref())
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Control> {
        self.slots.get_mut(id.0 as usize).filter(|s| s.generation == id.1).and_then(|s| s.node.as_mut())
    }

    fn min_of(&self, id: Id) -> Vec2 {
        self.slots.get(id.0 as usize).map_or(Vec2::ZERO, |s| s.min)
    }

    /// Add `c` as the last child of `parent`.
    pub fn add(&mut self, parent: Id, mut c: Control) -> Id {
        c.parent = Some(parent);
        let id = self.alloc(c);
        if let Some(p) = self.get_mut(parent) {
            p.children.push(id);
        }
        id
    }

    pub fn children(&self, id: Id) -> Vec<Id> {
        self.get(id).map(|c| c.children.clone()).unwrap_or_default()
    }

    /// Remove a control and its subtree.
    pub fn remove(&mut self, id: Id) {
        if id == self.root {
            return;
        }
        if let Some(p) = self.get(id).and_then(|c| c.parent)
            && let Some(p) = self.get_mut(p)
        {
            p.children.retain(|&c| c != id);
        }
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let slot = &mut self.slots[n.0 as usize];
            if slot.generation != n.1 {
                continue;
            }
            if let Some(node) = slot.node.take() {
                stack.extend(node.children);
                slot.generation = slot.generation.wrapping_add(1);
                self.free.push(n.0);
            }
        }
        for s in [&mut self.hovered, &mut self.pressed, &mut self.focus] {
            if s.is_some_and(|x| self.slots[x.0 as usize].generation != x.1) {
                *s = None;
            }
        }
    }

    pub fn clear_children(&mut self, id: Id) {
        for c in self.children(id) {
            self.remove(c);
        }
    }

    // ------------------------------------------------------------- accessors

    pub fn set_visible(&mut self, id: Id, v: bool) {
        if let Some(c) = self.get_mut(id) {
            c.visible = v;
        }
        if !v && self.dropdown.as_ref().is_some_and(|d| d.owner == id) {
            self.dropdown = None;
        }
    }

    pub fn is_visible(&self, id: Id) -> bool {
        self.get(id).is_some_and(|c| c.visible)
    }

    pub fn set_disabled(&mut self, id: Id, d: bool) {
        if let Some(c) = self.get_mut(id) {
            c.disabled = d;
        }
    }

    pub fn set_style(&mut self, id: Id, s: Option<StyleBox>) {
        if let Some(c) = self.get_mut(id) {
            c.style = s;
        }
    }

    pub fn set_font_color(&mut self, id: Id, color: Option<Color32>) {
        if let Some(c) = self.get_mut(id) {
            c.font_color = color;
        }
    }

    pub fn set_tooltip(&mut self, id: Id, t: Option<String>) {
        if let Some(c) = self.get_mut(id) {
            c.tooltip = t;
        }
    }

    /// Text of a Label, Button, CheckBox or LineEdit.
    pub fn text(&self, id: Id) -> &str {
        match self.get(id).map(|c| &c.kind) {
            Some(Kind::Label { text, .. } | Kind::Button { text, .. } | Kind::CheckBox { text, .. } | Kind::LineEdit { text, .. }) => text,
            _ => "",
        }
    }

    pub fn set_text(&mut self, id: Id, new: impl Into<String>) {
        let new = new.into();
        if let Some(c) = self.get_mut(id) {
            match &mut c.kind {
                Kind::Label { text, .. } | Kind::Button { text, .. } | Kind::CheckBox { text, .. } => *text = new,
                Kind::LineEdit { text, cursor, select_all, .. } => {
                    *cursor = new.chars().count();
                    *text = new;
                    *select_all = false;
                }
                _ => {}
            }
        }
    }

    pub fn is_checked(&self, id: Id) -> bool {
        match self.get(id).map(|c| &c.kind) {
            Some(Kind::CheckBox { checked, .. }) => *checked,
            Some(Kind::Button { pressed, .. }) => *pressed,
            _ => false,
        }
    }

    pub fn set_checked(&mut self, id: Id, on: bool) {
        if let Some(c) = self.get_mut(id) {
            match &mut c.kind {
                Kind::CheckBox { checked, .. } => *checked = on,
                Kind::Button { pressed, .. } => *pressed = on,
                _ => {}
            }
        }
    }

    pub fn selected(&self, id: Id) -> usize {
        match self.get(id).map(|c| &c.kind) {
            Some(Kind::OptionButton { selected, .. }) => *selected,
            _ => 0,
        }
    }

    /// Replace an OptionButton's items and selection.
    pub fn set_items(&mut self, id: Id, new: Vec<String>, sel: usize) {
        if let Some(Control { kind: Kind::OptionButton { selected, items }, .. }) = self.get_mut(id) {
            *items = new;
            *selected = sel.min(items.len().saturating_sub(1));
        }
    }

    pub fn set_selected(&mut self, id: Id, i: usize) {
        if let Some(Control { kind: Kind::OptionButton { selected, items }, .. }) = self.get_mut(id) {
            *selected = i.min(items.len().saturating_sub(1));
        }
    }

    pub fn focus(&mut self, id: Id) {
        if let Some(Control { kind: Kind::LineEdit { select_all, text, .. }, .. }) = self.get_mut(id) {
            *select_all = !text.is_empty();
        }
        self.focus = Some(id);
    }

    pub fn focused(&self) -> Option<Id> {
        self.focus
    }

    pub fn rect(&self, id: Id) -> Rect {
        self.get(id).map_or(Rect::NOTHING, |c| c.rect)
    }

    pub fn scroll_to_top(&mut self, id: Id) {
        if let Some(Control { kind: Kind::Scroll { offset, .. }, .. }) = self.get_mut(id) {
            *offset = 0.0;
        }
    }

    pub fn take_events(&mut self) -> Vec<UiEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn text_server(&mut self) -> &mut TextServer {
        &mut self.text
    }

    /// True while a LineEdit has keyboard focus.
    pub fn wants_text_input(&self) -> bool {
        self.focus.and_then(|f| self.get(f)).is_some_and(|c| matches!(c.kind, Kind::LineEdit { .. } | Kind::CodeEdit(_)))
    }

    // --------------------------------------------------------------- layout

    fn font_size(&self, c: &Control) -> f32 {
        c.font_size.unwrap_or(self.theme.font_size)
    }

    fn sep(&self, c: &Control) -> f32 {
        match c.kind {
            Kind::HBox { separation } | Kind::VBox { separation } => separation.unwrap_or(self.theme.separation),
            _ => 0.0,
        }
    }

    fn button_styles(&self, c: &Control) -> StateStyles {
        if let Some(s) = c.button_style {
            return s;
        }
        match c.kind {
            Kind::Button { primary: true, .. } => self.theme.button_primary,
            Kind::Button { flat: true, .. } => self.theme.button_flat,
            _ => self.theme.button,
        }
    }

    fn visible_children(&self, id: Id) -> Vec<Id> {
        let skip_native = id == self.root && self.native_windows;
        self.get(id).map(|c| c.children.iter().copied().filter(|&k| self.is_visible(k) && !(skip_native && self.is_native(k))).collect()).unwrap_or_default()
    }

    /// Bottom-up minimum sizes (Godot's `get_combined_minimum_size`).
    fn compute_min(&mut self, id: Id) -> Vec2 {
        let kids = self.visible_children(id);
        let kid_mins: Vec<Vec2> = kids.iter().map(|&k| self.compute_min(k)).collect();
        let Some(c) = self.get(id) else { return Vec2::ZERO };
        let c = c.clone();
        let fs = self.font_size(&c);
        let max_kid = kid_mins.iter().fold(Vec2::ZERO, |a, b| a.max(*b));
        let content = match &c.kind {
            Kind::Control | Kind::Panel | Kind::Modal | Kind::ColorRect { .. } | Kind::Custom { .. } => Vec2::ZERO,
            Kind::PanelContainer => max_kid + c.style.unwrap_or(self.theme.panel).padding.size(),
            Kind::Margin(m) => max_kid + m.size(),
            Kind::Center => max_kid,
            Kind::Scroll { .. } => vec2(max_kid.x, 0.0),
            Kind::HBox { .. } | Kind::VBox { .. } => {
                let vertical = matches!(c.kind, Kind::VBox { .. });
                let sep = self.sep(&c) * kid_mins.len().saturating_sub(1) as f32;
                if vertical {
                    vec2(max_kid.x, kid_mins.iter().map(|m| m.y).sum::<f32>() + sep)
                } else {
                    vec2(kid_mins.iter().map(|m| m.x).sum::<f32>() + sep, max_kid.y)
                }
            }
            Kind::Label { text, ellipsis, wrap, .. } => {
                if *wrap {
                    let w = if c.rect.width() > 1.0 { c.rect.width() } else { c.min_size.x.max(200.0) };
                    let g = self.text.layout_wrapped(text, fs, Color32::WHITE, w, usize::MAX, false);
                    vec2(0.0, g.size().y)
                } else {
                    let g = self.text.layout(text, fs, Color32::WHITE);
                    if *ellipsis { vec2(0.0, g.size().y) } else { g.size() }
                }
            }
            Kind::Button { text, icon, .. } => {
                let g = self.text.layout(text, fs, Color32::WHITE);
                let mut w = g.size().x;
                if icon.is_some() {
                    w += ICON_SIZE + if text.is_empty() { 0.0 } else { 6.0 };
                }
                let h = g.size().y.max(if icon.is_some() { ICON_SIZE } else { 0.0 });
                vec2(w, h) + self.button_styles(&c).normal.padding.size()
            }
            Kind::CheckBox { text, switch, .. } => {
                let g = self.text.layout(text, fs, Color32::WHITE);
                let box_w = if *switch { 30.0 } else { CHECK_SIZE };
                let gap = if text.is_empty() { 0.0 } else { 8.0 };
                vec2(box_w + gap + g.size().x, g.size().y.max(CHECK_SIZE)) + vec2(0.0, 4.0)
            }
            Kind::LineEdit { .. } => {
                let h = self.text.row_height(fs);
                vec2(60.0, h) + self.theme.line_edit.padding.size()
            }
            Kind::OptionButton { items, .. } => {
                let bold = c.bold;
                let w = items.iter().map(|i| if bold { self.text.layout_bold(i, fs, Color32::WHITE) } else { self.text.layout(i, fs, Color32::WHITE) }.size().x).fold(0.0, f32::max);
                let h = self.text.row_height(fs);
                vec2(w + 22.0, h) + self.button_styles(&c).normal.padding.size()
            }
            Kind::MenuBar { .. }
            | Kind::TabBar { .. }
            | Kind::Split { .. }
            | Kind::SpinBox { .. }
            | Kind::ColorButton { .. }
            | Kind::Image { .. }
            | Kind::Canvas { .. }
            | Kind::Tree { .. }
            | Kind::Popup { .. } => self.widget_min(&c, &kid_mins),
            Kind::CodeEdit(_) => vec2(0.0, 0.0),
            Kind::Separator { vertical } => {
                if *vertical {
                    vec2(self.theme.separation * 2.0 + 1.0, 0.0)
                } else {
                    vec2(0.0, self.theme.separation * 2.0 + 1.0)
                }
            }
        };
        let min = content.max(c.min_size);
        self.slots[id.0 as usize].min = min;
        min
    }

    /// Godot's `Container::fit_child_in_rect`.
    fn fit(&mut self, child: Id, r: Rect) {
        let min = self.min_of(child);
        let Some(c) = self.get(child) else { return };
        let (h, v) = (c.h, c.v);
        let mut out = r;
        if !h.fill {
            let w = min.x.min(r.width()).max(min.x);
            let x = match h.shrink {
                Align::Start => r.left(),
                Align::Center => (r.left() + (r.width() - w) / 2.0).floor(),
                Align::End => r.right() - w,
            };
            out = Rect::from_min_size(pos2(x, out.top()), vec2(w, out.height()));
        }
        if !v.fill {
            let hgt = min.y.min(r.height()).max(min.y);
            let y = match v.shrink {
                Align::Start => r.top(),
                Align::Center => (r.top() + (r.height() - hgt) / 2.0).floor(),
                Align::End => r.bottom() - hgt,
            };
            out = Rect::from_min_size(pos2(out.left(), y), vec2(out.width(), hgt));
        }
        self.arrange(child, out);
    }

    /// Place a child that is not inside a container by its anchors.
    fn anchored(&mut self, child: Id, parent: Rect) {
        let min = self.min_of(child);
        let a = self.get(child).and_then(|c| c.anchors).unwrap_or(Anchors::FULL_RECT);
        let axis = |lo: f32, hi: f32, start: f32, len: f32, off_lo: f32, off_hi: f32, min: f32| -> (f32, f32) {
            if lo == hi {
                // Anchored to a point: keep the minimum size, aligned by the anchor.
                let p = start + len * lo + off_lo;
                (p - min * lo, p - min * lo + min)
            } else {
                (start + len * lo + off_lo, start + len * hi + off_hi)
            }
        };
        let (x0, x1) = axis(a.left, a.right, parent.left(), parent.width(), a.offsets.left, a.offsets.right, min.x);
        let (y0, y1) = axis(a.top, a.bottom, parent.top(), parent.height(), a.offsets.top, a.offsets.bottom, min.y);
        let r = Rect::from_min_max(pos2(x0, y0), pos2(x1.max(x0 + min.x), y1.max(y0 + min.y)));
        self.arrange(child, r);
    }

    fn arrange(&mut self, id: Id, rect: Rect) {
        let Some(c) = self.get_mut(id) else { return };
        c.rect = rect;
        let c = c.clone();
        let kids = self.visible_children(id);
        match &c.kind {
            Kind::PanelContainer => {
                let inner = c.style.unwrap_or(self.theme.panel).padding.shrink(rect);
                for k in kids {
                    self.fit(k, inner);
                }
            }
            Kind::Margin(m) => {
                let inner = m.shrink(rect);
                for k in kids {
                    self.fit(k, inner);
                }
            }
            Kind::Center => {
                for k in kids {
                    let m = self.min_of(k);
                    self.arrange(k, Rect::from_center_size(rect.center(), m.min(rect.size()).max(m)));
                }
            }
            Kind::Modal => {
                for k in kids {
                    if self.get(k).is_some_and(|c| c.anchors.is_some()) {
                        self.anchored(k, rect);
                    } else {
                        let m = self.min_of(k).min(rect.size() - vec2(24.0, 24.0));
                        let r = Rect::from_center_size(rect.center(), m);
                        self.arrange(k, r);
                    }
                }
            }
            Kind::Scroll { offset, .. } => {
                if let Some(&k) = kids.first() {
                    let m = self.min_of(k);
                    let content = m.y.max(rect.height());
                    let max_off = (content - rect.height()).max(0.0);
                    let off = offset.clamp(0.0, max_off);
                    if let Some(Control { kind: Kind::Scroll { offset, content: cont }, .. }) = self.get_mut(id) {
                        *offset = off;
                        *cont = content;
                    }
                    self.arrange(k, Rect::from_min_size(pos2(rect.left(), rect.top() - off), vec2(rect.width(), content)));
                }
            }
            Kind::HBox { .. } | Kind::VBox { .. } => self.resort_box(id, &c, &kids, rect),
            Kind::Split { vertical, offset } => self.arrange_split(id, rect, *vertical, *offset, &kids),
            Kind::Popup { at } => {
                for k in kids {
                    let m = self.min_of(k);
                    let x = at.x.min(rect.right() - m.x).max(rect.left());
                    let y = at.y.min(rect.bottom() - m.y).max(rect.top());
                    self.arrange(k, Rect::from_min_size(pos2(x, y), m));
                }
            }
            _ => {
                for k in kids {
                    self.anchored(k, rect);
                }
            }
        }
    }

    /// Godot's `BoxContainer::_resort`: expanding children share the stretch
    /// space by ratio; any that would end up below their minimum stop stretching.
    fn resort_box(&mut self, _id: Id, c: &Control, kids: &[Id], rect: Rect) {
        if kids.is_empty() {
            return;
        }
        let vertical = matches!(c.kind, Kind::VBox { .. });
        let sep = self.sep(c);
        let main = |v: Vec2| if vertical { v.y } else { v.x };
        let mut min_sizes = Vec::with_capacity(kids.len());
        let mut stretch = Vec::with_capacity(kids.len());
        let mut ratios = Vec::with_capacity(kids.len());
        let mut stretch_min = 0.0;
        let mut stretch_avail = 0.0;
        let mut ratio_total = 0.0;
        for &k in kids {
            let m = main(self.min_of(k));
            let flags = self.get(k).map(|c| if vertical { c.v } else { c.h }).unwrap_or(SizeFlags::FILL);
            stretch_min += m;
            if flags.expand {
                stretch_avail += m;
                ratio_total += flags.ratio;
            }
            min_sizes.push(m);
            stretch.push(flags.expand);
            ratios.push(flags.ratio);
        }
        let stretch_max = main(rect.size()) - sep * (kids.len() - 1) as f32;
        stretch_avail += (stretch_max - stretch_min).max(0.0);
        let mut finals = min_sizes.clone();
        while ratio_total > 0.0 {
            let mut refit_ok = true;
            for i in 0..kids.len() {
                if !stretch[i] {
                    continue;
                }
                let size = stretch_avail * ratios[i] / ratio_total;
                if size < min_sizes[i] {
                    stretch[i] = false;
                    ratio_total -= ratios[i];
                    stretch_avail -= min_sizes[i];
                    finals[i] = min_sizes[i];
                    refit_ok = false;
                    break;
                }
                finals[i] = size.floor();
            }
            if refit_ok {
                break;
            }
        }
        let mut ofs = if vertical { rect.top() } else { rect.left() };
        for (i, &k) in kids.iter().enumerate() {
            let r = if vertical {
                Rect::from_min_size(pos2(rect.left(), ofs), vec2(rect.width(), finals[i]))
            } else {
                Rect::from_min_size(pos2(ofs, rect.top()), vec2(finals[i], rect.height()))
            };
            self.fit(k, r);
            ofs += finals[i] + sep;
        }
    }

    // ---------------------------------------------------------------- input

    /// The topmost visible modal, which captures all input.
    fn top_modal(&self) -> Option<Id> {
        self.get(self.root)?.children.iter().rev().copied().find(|&k| self.get(k).is_some_and(|c| c.visible && matches!(c.kind, Kind::Modal | Kind::Popup { .. })) && !self.is_native(k))
    }

    fn hit(&self, pos: Pos2) -> Option<Id> {
        // Inside a native window, hits start at its panel.
        if let Some(w) = self.active_window {
            let panel = self.get(w)?.children.first().copied()?;
            return self.hit_in(panel, pos, self.screen);
        }
        let start = self.top_modal().unwrap_or(self.root);
        self.hit_in(start, pos, self.screen)
    }

    fn hit_in(&self, id: Id, pos: Pos2, clip: Rect) -> Option<Id> {
        let c = self.get(id)?;
        if !c.visible || !clip.contains(pos) {
            return None;
        }
        let clip = if matches!(c.kind, Kind::Scroll { .. }) { clip.intersect(c.rect) } else { clip };
        if self.split_divider(c).is_some_and(|d| d.contains(pos)) {
            return Some(id);
        }
        // Without an open modal, ignore closed modals entirely.
        for &k in c.children.iter().rev() {
            if id == self.root && self.get(k).is_some_and(|c| matches!(c.kind, Kind::Modal | Kind::Popup { .. })) && Some(k) != self.top_modal() {
                continue;
            }
            if id == self.root && self.is_native(k) {
                continue;
            }
            if let Some(h) = self.hit_in(k, pos, clip) {
                return Some(h);
            }
        }
        (c.rect.contains(pos) && c.mouse == MouseFilter::Stop).then_some(id)
    }

    fn ancestor_scroll(&self, mut id: Option<Id>) -> Option<Id> {
        while let Some(i) = id {
            let c = self.get(i)?;
            if matches!(c.kind, Kind::Scroll { .. }) {
                return Some(i);
            }
            id = c.parent;
        }
        None
    }

    /// Feed a platform event. Mouse positions are physical pixels.
    pub fn on_event(&mut self, event: &Event) {
        if self.active_window.is_none() {
            self.event_window = None;
            // Like Godot's exclusive dialogs: the main window takes no input
            // while a native dialog window is open.
            if self.any_native_open() && !matches!(event, Event::MouseMoved { .. } | Event::MouseLeft | Event::ModifiersChanged(_) | Event::Resized { .. } | Event::ScaleFactorChanged(_) | Event::Focused(_)) {
                return;
            }
        }
        match event {
            Event::MouseMoved { x, y } => {
                let p = pos2(x / self.ppp, y / self.ppp);
                self.mouse = Some(p);
                if self.menus_hover(p) {
                    return;
                }
                if self.drag_move(p) {
                    self.update_hover();
                    return;
                }
                if let Some(d) = &mut self.dropdown {
                    let row = self.text.row_height(self.theme.font_size) + 8.0;
                    d.hovered = d.rect.contains(p).then(|| ((p.y - d.rect.top() - 4.0) / row).floor().max(0.0) as usize);
                }
                self.update_hover();
                self.canvas_hover(p);
            }
            Event::MouseLeft => {
                self.mouse = None;
                self.hovered = None;
            }
            Event::ModifiersChanged(m) => self.modifiers = *m,
            Event::MouseButton { button: MouseButton::Left, pressed: true } => self.press(),
            Event::MouseButton { button: MouseButton::Left, pressed: false } => self.release(),
            Event::MouseButton { button, pressed: true } => {
                if let Some(p) = self.mouse {
                    self.other_press(*button, p);
                }
            }
            Event::MouseButton { button, pressed: false } => {
                if let Some(p) = self.mouse {
                    self.other_release(*button, p);
                }
            }
            Event::MouseWheel { delta_x, delta_y, unit } => {
                let lines = match unit {
                    WheelUnit::Lines => vec2(*delta_x, *delta_y),
                    WheelUnit::Pixels => vec2(*delta_x, *delta_y) / (40.0 * self.ppp),
                };
                if self.menu_open() || self.widget_wheel(lines) {
                    return;
                }
                let dy = match unit {
                    WheelUnit::Lines => delta_y * 40.0,
                    WheelUnit::Pixels => delta_y / self.ppp,
                };
                if let Some(s) = self.ancestor_scroll(self.hovered)
                    && let Some(Control { kind: Kind::Scroll { offset, .. }, .. }) = self.get_mut(s)
                {
                    *offset -= dy;
                }
            }
            Event::Key { key, pressed: true, modifiers, .. } => {
                self.modifiers = *modifiers;
                if *key == Key::Escape && self.menu_open() {
                    self.close_menus();
                    return;
                }
                if self.code_key(*key, *modifiers) || self.widget_key(*key, *modifiers) {
                    return;
                }
                if !self.edit_key(*key, *modifiers) {
                    if *key == Key::Escape && self.dropdown.is_some() {
                        self.dropdown = None;
                    } else {
                        self.events.push(UiEvent::Key { key: *key, modifiers: *modifiers });
                    }
                }
            }
            Event::Text(t) => {
                if !self.code_text(t) {
                    self.insert_text(t)
                }
            }
            _ => {}
        }
    }

    fn update_hover(&mut self) {
        let h = if self.dropdown.is_some() { None } else { self.mouse.and_then(|p| self.hit(p)) };
        if h != self.hovered {
            self.hovered = h;
            self.hover_since = Instant::now();
        }
    }

    fn press(&mut self) {
        let Some(p) = self.mouse else { return };
        if let Some(d) = self.dropdown.take() {
            if d.rect.contains(p)
                && let Some(i) = d.hovered
            {
                let count = match self.get(d.owner).map(|c| &c.kind) {
                    Some(Kind::OptionButton { items, .. }) => items.len(),
                    _ => 0,
                };
                if i < count {
                    self.set_selected(d.owner, i);
                    self.events.push(UiEvent::ItemSelected(d.owner, i));
                }
            }
            self.update_hover();
            return;
        }
        if self.menus_press(p) {
            self.update_hover();
            return;
        }
        let target = self.hit(p);
        if let Some(t) = target
            && self.widget_press(t, p)
        {
            self.pressed = None;
            return;
        }
        self.commit_edits(target);
        self.pressed = target;
        let editable = target.and_then(|t| self.get(t)).is_some_and(|c| matches!(c.kind, Kind::LineEdit { .. }) && !c.disabled);
        if editable {
            let t = target.expect("checked");
            self.focus = Some(t);
            self.place_cursor(t, p);
        } else {
            self.focus = None;
        }
    }

    fn release(&mut self) {
        if let Some(m) = self.mouse
            && self.drag_release(m)
        {
            self.pressed = None;
            return;
        }
        let Some(p) = self.pressed.take() else { return };
        if self.mouse.and_then(|m| self.hit(m)) != Some(p) {
            return;
        }
        if self.get(p).is_some_and(|c| c.disabled) {
            return;
        }
        self.click(p);
    }

    fn click(&mut self, id: Id) {
        let now = Instant::now();
        let double = self.last_click.is_some_and(|(l, t)| l == id && now - t < DOUBLE_CLICK);
        self.last_click = if double { None } else { Some((id, now)) };
        let Some(c) = self.get(id).cloned() else { return };
        let parent = c.parent;
        match c.kind.clone() {
            Kind::Button { toggle: true, pressed, .. } => {
                self.set_checked(id, !pressed);
                self.events.push(UiEvent::Toggled(id, !pressed));
                self.events.push(UiEvent::Pressed(id));
            }
            Kind::Button { .. } | Kind::ColorButton { .. } => self.events.push(UiEvent::Pressed(id)),
            Kind::CheckBox { radio: true, checked, .. } => {
                if !checked {
                    for s in parent.map(|p| self.children(p)).unwrap_or_default() {
                        if s != id && matches!(self.get(s).map(|c| &c.kind), Some(Kind::CheckBox { radio: true, .. })) {
                            self.set_checked(s, false);
                        }
                    }
                    self.set_checked(id, true);
                    self.events.push(UiEvent::Toggled(id, true));
                }
            }
            Kind::CheckBox { checked, .. } => {
                self.set_checked(id, !checked);
                self.events.push(UiEvent::Toggled(id, !checked));
            }
            Kind::OptionButton { items, .. } => {
                let r = c.rect;
                let row = self.text.row_height(self.theme.font_size) + 8.0;
                let h = items.len() as f32 * row + 8.0;
                let below = Rect::from_min_size(pos2(r.left(), r.bottom() + 2.0), vec2(r.width().max(140.0), h));
                let rect = if below.bottom() > self.screen.bottom() { below.translate(vec2(0.0, -(h + r.height() + 4.0))) } else { below };
                self.dropdown = Some(Dropdown { owner: id, rect, hovered: None });
            }
            _ if c.clickable => {
                self.events.push(UiEvent::Pressed(id));
                if double {
                    self.events.push(UiEvent::DoubleClicked(id));
                }
            }
            _ => {}
        }
        if double && !c.clickable && matches!(c.kind, Kind::Button { .. }) {
            self.events.push(UiEvent::DoubleClicked(id));
        }
    }

    fn place_cursor(&mut self, id: Id, p: Pos2) {
        let Some(c) = self.get(id) else { return };
        let Kind::LineEdit { text, monospace, .. } = &c.kind else { return };
        let (text, mono) = (text.clone(), *monospace);
        let fs = self.font_size(c);
        let x = p.x - self.theme.line_edit.padding.shrink(c.rect).left();
        let mut best = text.chars().count();
        let mut prev = 0.0;
        for (i, (byte, _)) in text.char_indices().enumerate() {
            let w = self.measure(&text[..byte], fs, mono);
            if x < (prev + w) / 2.0 {
                best = i.saturating_sub(1);
                break;
            }
            prev = w;
            best = i;
        }
        if x >= self.measure(&text, fs, mono) {
            best = text.chars().count();
        }
        if let Some(Control { kind: Kind::LineEdit { cursor, select_all, .. }, .. }) = self.get_mut(id) {
            *cursor = best;
            *select_all = false;
        }
    }

    fn measure(&mut self, s: &str, size: f32, mono: bool) -> f32 {
        if mono { self.text.layout_mono(s, size, Color32::WHITE).size().x } else { self.text.layout(s, size, Color32::WHITE).size().x }
    }

    fn insert_text(&mut self, t: &str) {
        let Some(f) = self.focus else { return };
        if self.get(f).is_none_or(|c| c.disabled) {
            return;
        }
        let is_line = matches!(self.get(f).map(|c| &c.kind), Some(Kind::LineEdit { .. }));
        let Some((text, cursor, select_all)) = self.edit_of(f) else { return };
        if *select_all {
            text.clear();
            *cursor = 0;
            *select_all = false;
        }
        let byte = text.char_indices().nth(*cursor).map_or(text.len(), |(b, _)| b);
        text.insert_str(byte, t);
        *cursor += t.chars().count();
        if is_line {
            self.events.push(UiEvent::TextChanged(f));
        }
    }

    /// Editing keys for the focused LineEdit. Returns true if used.
    fn edit_key(&mut self, key: Key, m: Modifiers) -> bool {
        let Some(f) = self.focus else { return false };
        let ids_in_order = self.line_edits();
        let is_line = matches!(self.get(f).map(|c| &c.kind), Some(Kind::LineEdit { .. }));
        let Some((text, cursor, select_all)) = self.edit_of(f) else { return false };
        let len = text.chars().count();
        let byte = |t: &str, i: usize| t.char_indices().nth(i).map_or(t.len(), |(b, _)| b);
        let mut changed = false;
        match key {
            Key::Char('a') if m.ctrl || m.logo => *select_all = true,
            _ if m.ctrl || m.logo => return false,
            Key::Backspace => {
                if *select_all {
                    text.clear();
                    *cursor = 0;
                    *select_all = false;
                    changed = true;
                } else if *cursor > 0 {
                    let (a, b) = (byte(text, *cursor - 1), byte(text, *cursor));
                    text.replace_range(a..b, "");
                    *cursor -= 1;
                    changed = true;
                }
            }
            Key::Delete => {
                if *select_all {
                    text.clear();
                    *cursor = 0;
                    *select_all = false;
                    changed = true;
                } else if *cursor < len {
                    let (a, b) = (byte(text, *cursor), byte(text, *cursor + 1));
                    text.replace_range(a..b, "");
                    changed = true;
                }
            }
            Key::ArrowLeft => {
                *select_all = false;
                *cursor = cursor.saturating_sub(1);
            }
            Key::ArrowRight => {
                *select_all = false;
                *cursor = (*cursor + 1).min(len);
            }
            Key::Home => {
                *select_all = false;
                *cursor = 0;
            }
            Key::End => {
                *select_all = false;
                *cursor = len;
            }
            Key::Enter => {
                if is_line {
                    self.events.push(UiEvent::TextSubmitted(f));
                }
                return true;
            }
            Key::Escape => {
                self.focus = None;
                return false; // still let the app close a dialog
            }
            Key::Tab => {
                if let Some(i) = ids_in_order.iter().position(|&x| x == f) {
                    let next = ids_in_order[(i + if m.shift { ids_in_order.len() - 1 } else { 1 }) % ids_in_order.len()];
                    self.focus(next);
                }
            }
            _ => return matches!(key, Key::Char(_) | Key::Space),
        }
        if changed && is_line {
            self.events.push(UiEvent::TextChanged(f));
        }
        true
    }

    /// Visible, enabled LineEdits in tree order (inside the top modal if any).
    fn line_edits(&self) -> Vec<Id> {
        let mut out = Vec::new();
        let mut stack = vec![self.top_modal().unwrap_or(self.root)];
        while let Some(id) = stack.pop() {
            let Some(c) = self.get(id) else { continue };
            if !c.visible {
                continue;
            }
            if matches!(c.kind, Kind::LineEdit { .. }) && !c.disabled {
                out.push(id);
            }
            stack.extend(c.children.iter().rev());
        }
        out
    }

    // -------------------------------------------------------------- painting

    /// Lay out, paint and tessellate one frame.
    pub fn frame(&mut self, size_px: (u32, u32), pixels_per_point: f32, max_texture_side: usize) -> UiOutput {
        self.ppp = pixels_per_point.max(0.25);
        self.screen = Rect::from_min_size(Pos2::ZERO, vec2(size_px.0 as f32, size_px.1 as f32) / self.ppp);
        self.text.begin_frame(self.ppp, max_texture_side);
        // Twice, so wrapped labels see their final width.
        for _ in 0..2 {
            self.compute_min(self.root);
            let root = self.root;
            self.arrange(root, self.screen);
        }
        self.update_hover();

        let mut shapes = Vec::new();
        if self.theme.background.a() > 0 {
            shapes.push(ClippedShape::new(self.screen, Shape::rect_filled(self.screen, CornerRadius::ZERO, self.theme.background)));
        }
        let root = self.root;
        let top = self.top_modal();
        self.paint(root, self.screen, &mut shapes, top);
        self.finish_frame(shapes, true)
    }

    /// Menus, tooltip, tessellation and cursor; `textures` also flushes the
    /// pending texture uploads (only the main window does, before the others).
    pub(crate) fn finish_frame(&mut self, mut shapes: Vec<ClippedShape>, textures: bool) -> UiOutput {
        self.paint_dropdown(&mut shapes);
        self.paint_menus(&mut shapes);
        let repaint_after = if self.menu_open() || self.drag.is_some() { None } else { self.paint_tooltip(&mut shapes) };
        let mut textures_out = TexturesDelta::default();
        if textures {
            if let Some(delta) = self.text.take_delta() {
                textures_out.push(self.font_texture, delta);
            }
            for (id, delta) in self.pending_textures.drain(..) {
                textures_out.push(id, delta);
            }
        }
        let atlas = self.text.atlas();
        let mut tess = Tessellator::new(self.ppp, TessellationOptions::default(), atlas.size(), atlas.prepared_discs());
        let mut primitives = tess.tessellate_shapes(shapes);
        for p in &mut primitives {
            if let Primitive::Mesh(m) = &mut p.primitive
                && m.texture_id == TextureId::default()
            {
                m.texture_id = self.font_texture;
            }
        }
        let cursor = match self.hovered.and_then(|h| self.get(h)) {
            Some(c) if c.disabled => CursorIcon::Default,
            Some(Control { kind: Kind::LineEdit { .. } | Kind::CodeEdit(_), .. }) => CursorIcon::Text,
            Some(Control { kind: Kind::Split { vertical: false, .. }, .. }) => CursorIcon::ResizeHorizontal,
            Some(Control { kind: Kind::Split { vertical: true, .. }, .. }) => CursorIcon::ResizeVertical,
            Some(Control { kind: Kind::SpinBox { edit: None, .. }, .. }) => CursorIcon::ResizeHorizontal,
            Some(Control { kind: Kind::SpinBox { .. }, .. }) => CursorIcon::Text,
            Some(c) if c.clickable || matches!(c.kind, Kind::Button { .. } | Kind::CheckBox { .. } | Kind::OptionButton { .. }) => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        };
        UiOutput { primitives, textures: textures_out, cursor, repaint_after }
    }

    fn paint(&mut self, id: Id, clip: Rect, out: &mut Vec<ClippedShape>, top_modal: Option<Id>) {
        let Some(c) = self.get(id) else { return };
        if !c.visible {
            return;
        }
        if id != self.root && matches!(c.kind, Kind::Modal) && Some(id) != top_modal && top_modal.is_some() {
            // Lower modals still draw (dialogs over dialogs), just without input.
        }
        let c = c.clone();
        let r = c.rect;
        let hovered = self.hovered == Some(id) && !c.disabled;
        let pressed = hovered && self.pressed == Some(id);
        let fs = self.font_size(&c);
        let t = self.theme.clone();
        let font_color = c.font_color.unwrap_or(if c.disabled { t.font_disabled_color } else if hovered { t.font_hover_color } else { t.font_color });
        let push = |out: &mut Vec<ClippedShape>, shape: Shape| out.push(ClippedShape::new(clip, shape));
        let draw_box = |out: &mut Vec<ClippedShape>, s: &StyleBox, r: Rect| {
            if s.bg != Color32::TRANSPARENT {
                out.push(ClippedShape::new(clip, Shape::rect_filled(r, s.corner(), s.bg)));
            }
            if s.border.width > 0.0 {
                out.push(ClippedShape::new(clip, Shape::rect_stroke(r, s.corner(), s.border, StrokeKind::Inside)));
            }
        };
        match &c.kind {
            Kind::Panel => draw_box(out, &c.style.unwrap_or(t.panel), r),
            Kind::PanelContainer => {
                draw_box(out, &c.style.unwrap_or(t.panel), r);
                if c.clickable && hovered {
                    push(out, Shape::rect_filled(r, CornerRadius::same(t.corner_radius), t.item_hover));
                }
            }
            Kind::Modal => push(out, Shape::rect_filled(r, CornerRadius::ZERO, t.modal_dim)),
            Kind::ColorRect { color, radius } => push(out, Shape::rect_filled(r, CornerRadius::same(*radius), *color)),
            Kind::Separator { vertical } => {
                let s = Stroke::new(1.0, t.separator);
                let pts = if *vertical { [pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom())] } else { [pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)] };
                push(out, Shape::line_segment(pts, s));
            }
            Kind::Label { text, align, ellipsis, wrap } => {
                let g = if *wrap {
                    self.text.layout_wrapped(text, fs, font_color, r.width(), usize::MAX, false)
                } else if *ellipsis {
                    self.text.layout_wrapped(text, fs, font_color, r.width(), 1, true)
                } else {
                    self.text.layout(text, fs, font_color)
                };
                let x = match align {
                    Align::Start => r.left(),
                    Align::Center => r.center().x - g.size().x / 2.0,
                    Align::End => r.right() - g.size().x,
                };
                push(out, Shape::galley(pos2(x, r.center().y - g.size().y / 2.0), g, font_color));
            }
            Kind::Button { text, align, toggle, pressed: on, primary, flat, icon } => {
                let st = self.button_styles(&c);
                let style = if c.disabled {
                    st.disabled
                } else if pressed || (*toggle && *on) {
                    st.pressed
                } else if hovered {
                    st.hover
                } else {
                    st.normal
                };
                draw_box(out, &style, r);
                let color = if c.disabled {
                    t.font_disabled_color
                } else if *primary {
                    t.accent_text
                } else if *toggle && *on && *flat {
                    t.accent
                } else {
                    c.font_color.unwrap_or(if hovered { t.font_hover_color } else { t.font_color })
                };
                let inner = style.padding.shrink(r);
                let g = self.text.layout(text, fs, color);
                let icon_w = if icon.is_some() { ICON_SIZE + if text.is_empty() { 0.0 } else { 6.0 } } else { 0.0 };
                let w = icon_w + g.size().x;
                let x = match align {
                    Align::Start => inner.left(),
                    Align::Center => inner.center().x - w / 2.0,
                    Align::End => inner.right() - w,
                };
                if let Some(draw) = icon {
                    let ir = Rect::from_center_size(pos2(x + ICON_SIZE / 2.0, r.center().y), Vec2::splat(ICON_SIZE));
                    let mut p = Painter { shapes: out, clip, text: &mut self.text };
                    draw(&mut p, ir, hovered, *toggle && *on);
                }
                push(out, Shape::galley(pos2(x + icon_w, r.center().y - g.size().y / 2.0), g, color));
            }
            Kind::CheckBox { text, checked, radio, switch } => {
                let box_w = if *switch { 30.0 } else { CHECK_SIZE };
                let b = Rect::from_min_size(pos2(r.left(), r.center().y - if *switch { 8.0 } else { CHECK_SIZE / 2.0 }), vec2(box_w, if *switch { 16.0 } else { CHECK_SIZE }));
                let on_color = if c.disabled { t.font_disabled_color } else { t.accent };
                if *switch {
                    push(out, Shape::rect_filled(b, CornerRadius::same(8), if *checked { on_color } else { t.check_box }));
                    push(out, Shape::rect_stroke(b, CornerRadius::same(8), Stroke::new(1.0, t.check_box_border.gamma_multiply(0.5)), StrokeKind::Inside));
                    let kx = if *checked { b.right() - 8.0 } else { b.left() + 8.0 };
                    push(out, Shape::circle_filled(pos2(kx, b.center().y), 6.0, if *checked { t.accent_text } else { t.font_color }));
                } else if *radio {
                    push(out, Shape::circle_filled(b.center(), 7.5, t.check_box));
                    push(out, Shape::circle_stroke(b.center(), 7.0, Stroke::new(1.0, if hovered { t.font_hover_color } else { t.check_box_border })));
                    if *checked {
                        push(out, Shape::circle_filled(b.center(), 4.0, on_color));
                    }
                } else {
                    push(out, Shape::rect_filled(b, CornerRadius::same(3), if *checked { on_color } else { t.check_box }));
                    push(out, Shape::rect_stroke(b, CornerRadius::same(3), Stroke::new(1.0, if *checked { on_color } else if hovered { t.font_hover_color } else { t.check_box_border }), StrokeKind::Inside));
                    if *checked {
                        let s = Stroke::new(2.0, t.accent_text);
                        let (a, m, e) = (b.min + vec2(3.5, 8.0), b.min + vec2(6.5, 11.0), b.min + vec2(12.5, 4.5));
                        push(out, Shape::line_segment([a, m], s));
                        push(out, Shape::line_segment([m, e], s));
                    }
                }
                if !text.is_empty() {
                    let g = self.text.layout(text, fs, font_color);
                    push(out, Shape::galley(pos2(b.right() + 8.0, r.center().y - g.size().y / 2.0), g, font_color));
                }
            }
            Kind::LineEdit { text, placeholder, cursor, select_all, monospace } => {
                let focused = self.focus == Some(id);
                let style = if focused { t.line_edit_focus } else { t.line_edit };
                draw_box(out, &style, r);
                let inner = style.padding.shrink(r);
                let iclip = clip.intersect(inner.expand(1.0));
                let (shown, color) = if text.is_empty() { (placeholder.as_str(), t.font_disabled_color) } else { (text.as_str(), if c.disabled { t.font_disabled_color } else { t.font_color }) };
                let g = if *monospace { self.text.layout_mono(shown, fs, color) } else { self.text.layout(shown, fs, color) };
                let y = r.center().y - g.size().y / 2.0;
                let byte = text.char_indices().nth(*cursor).map_or(text.len(), |(b, _)| b);
                let caret_x = self.measure(&text[..byte], fs, *monospace);
                // Scroll the text so the caret stays visible.
                let shift = (caret_x - inner.width() + 2.0).max(0.0);
                if focused && *select_all && !text.is_empty() {
                    let sel = Rect::from_min_size(pos2(inner.left() - shift, y), vec2(g.size().x, g.size().y));
                    out.push(ClippedShape::new(iclip, Shape::rect_filled(sel, CornerRadius::same(2), t.accent.gamma_multiply(0.35))));
                }
                out.push(ClippedShape::new(iclip, Shape::galley(pos2(inner.left() - shift, y), g.clone(), color)));
                if focused && !*select_all {
                    let x = inner.left() + caret_x - shift;
                    out.push(ClippedShape::new(iclip, Shape::line_segment([pos2(x, y), pos2(x, y + g.size().y.max(fs))], Stroke::new(1.5, t.accent))));
                }
            }
            Kind::OptionButton { items, selected } => {
                let st = self.button_styles(&c);
                let open = self.dropdown.as_ref().is_some_and(|d| d.owner == id);
                draw_box(out, &if c.disabled { st.disabled } else if pressed || open { st.pressed } else if hovered { st.hover } else { st.normal }, r);
                let inner = st.normal.padding.shrink(r);
                let label = items.get(*selected).cloned().unwrap_or_default();
                let g = if c.bold { self.text.layout_bold(&label, fs, font_color) } else { self.text.layout(&label, fs, font_color) };
                push(out, Shape::galley(pos2(inner.left(), r.center().y - g.size().y / 2.0), g, font_color));
                let a = pos2(inner.right() - 6.0, r.center().y);
                push(out, Shape::convex_polygon(vec![a + vec2(-4.5, -2.5), a + vec2(4.5, -2.5), a + vec2(0.0, 3.0)], font_color, Stroke::NONE));
            }
            Kind::MenuBar { .. }
            | Kind::TabBar { .. }
            | Kind::Split { .. }
            | Kind::SpinBox { .. }
            | Kind::ColorButton { .. }
            | Kind::Image { .. }
            | Kind::Canvas { .. }
            | Kind::Tree { .. } => self.paint_widget(id, &c, clip, out, hovered),
            Kind::Custom { draw } => {
                let mut p = Painter { shapes: out, clip, text: &mut self.text };
                draw(&mut p, r, hovered, pressed);
            }
            Kind::CodeEdit(h) => {
                let h = h.clone();
                self.paint_code(id, &c, &h, clip, out);
            }
            _ => {}
        }
        let child_clip = if matches!(c.kind, Kind::Scroll { .. }) { clip.intersect(r) } else { clip };
        for &k in &c.children {
            if id == self.root && self.get(k).is_some_and(|m| matches!(m.kind, Kind::Modal)) && !self.get(k).is_some_and(|m| m.visible) {
                continue;
            }
            if id == self.root && self.is_native(k) {
                continue;
            }
            self.paint(k, child_clip, out, top_modal);
        }
        if let Kind::Scroll { offset, content } = c.kind
            && content > r.height() + 0.5
        {
            let frac = r.height() / content;
            let h = (r.height() * frac).max(24.0);
            let y = r.top() + (r.height() - h) * (offset / (content - r.height()));
            let bar = Rect::from_min_size(pos2(r.right() - 5.0, y), vec2(3.0, h));
            out.push(ClippedShape::new(clip, Shape::rect_filled(bar, CornerRadius::same(2), t.font_color.gamma_multiply(0.35))));
        }
    }

    fn paint_dropdown(&mut self, out: &mut Vec<ClippedShape>) {
        let Some(d) = &self.dropdown else { return };
        let (rect, hovered, owner) = (d.rect, d.hovered, d.owner);
        let Some(Control { kind: Kind::OptionButton { items, selected }, .. }) = self.get(owner) else { return };
        let (items, selected) = (items.clone(), *selected);
        let t = self.theme.clone();
        let screen = self.screen;
        out.push(ClippedShape::new(screen, Shape::rect_filled(rect, t.popup_menu.corner(), t.popup_menu.bg)));
        out.push(ClippedShape::new(screen, Shape::rect_stroke(rect, t.popup_menu.corner(), t.popup_menu.border, StrokeKind::Inside)));
        let row = self.text.row_height(t.font_size) + 8.0;
        for (i, item) in items.iter().enumerate() {
            let r = Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 4.0 + i as f32 * row), vec2(rect.width() - 8.0, row));
            if hovered == Some(i) {
                out.push(ClippedShape::new(screen, Shape::rect_filled(r, CornerRadius::same(3), t.popup_hover)));
            }
            let color = if i == selected { t.accent } else { t.font_color };
            let g = self.text.layout(item, t.font_size, color);
            out.push(ClippedShape::new(screen, Shape::galley(pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, color)));
        }
    }

    fn paint_tooltip(&mut self, out: &mut Vec<ClippedShape>) -> Option<Duration> {
        let h = self.hovered?;
        let tip = self.get(h)?.tooltip.clone()?;
        let mouse = self.mouse?;
        let waited = self.hover_since.elapsed();
        if waited < TOOLTIP_DELAY {
            return Some(TOOLTIP_DELAY - waited);
        }
        let t = self.theme.tooltip;
        let fs = self.theme.font_size - 1.0;
        let color = self.theme.font_hover_color;
        let g = self.text.layout_wrapped(&tip, fs, color, 360.0, usize::MAX, false);
        let size = g.size() + t.padding.size();
        let mut pos = mouse + vec2(12.0, 18.0);
        pos.x = pos.x.min(self.screen.right() - size.x - 4.0);
        if pos.y + size.y > self.screen.bottom() {
            pos.y = mouse.y - size.y - 6.0;
        }
        let r = Rect::from_min_size(pos, size);
        let screen = self.screen;
        out.push(ClippedShape::new(screen, Shape::rect_filled(r, t.corner(), t.bg)));
        out.push(ClippedShape::new(screen, Shape::rect_stroke(r, t.corner(), t.border, StrokeKind::Inside)));
        out.push(ClippedShape::new(screen, Shape::galley(r.min + vec2(t.padding.left, t.padding.top), g, color)));
        None
    }

    /// Draw text with the UI's fonts (for custom draw helpers outside controls).
    pub fn measure_text(&mut self, text: &str, size: f32) -> Vec2 {
        self.text.layout(text, size, Color32::WHITE).size()
    }
}

/// Convenience for anchors in custom layouts.
pub fn anchor_align(a: Align) -> Align2 {
    match a {
        Align::Start => Align2::LEFT_CENTER,
        Align::Center => Align2::CENTER_CENTER,
        Align::End => Align2::RIGHT_CENTER,
    }
}

#[doc(hidden)]
pub fn _font(size: f32) -> FontId {
    FontId::proportional(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(ui: &mut Ui, w: f32, h: f32) {
        let mut out = ui.frame((w as u32, h as u32), 1.0, 2048);
        out.textures.clear();
    }

    #[test]
    fn hbox_shares_space_like_godot() {
        let mut ui = Ui::new(Theme::neko_dark());
        let row = ui.add(ui.root(), Control::hbox().separation(10.0));
        let fixed = ui.add(row, Control::control().min(100.0, 20.0));
        let a = ui.add(row, Control::control().min(10.0, 20.0).expand_h());
        let b = ui.add(row, Control::control().min(10.0, 20.0).expand_h().ratio(3.0));
        layout(&mut ui, 520.0, 100.0);
        // 520 - 2*10 separation - 100 fixed = 400 stretch space, split 1:3.
        assert_eq!(ui.rect(fixed).width(), 100.0);
        assert_eq!(ui.rect(a).width(), 100.0);
        assert_eq!(ui.rect(b).width(), 300.0);
        assert_eq!(ui.rect(b).left(), 100.0 + 10.0 + 100.0 + 10.0);
    }

    #[test]
    fn stretch_respects_minimum() {
        let mut ui = Ui::new(Theme::neko_dark());
        let row = ui.add(ui.root(), Control::hbox().separation(0.0));
        let big = ui.add(row, Control::control().min(300.0, 10.0).expand_h());
        let small = ui.add(row, Control::control().min(0.0, 10.0).expand_h());
        layout(&mut ui, 400.0, 50.0);
        // An even split (200/200) would shrink `big` below 300, so it keeps 300.
        assert_eq!(ui.rect(big).width(), 300.0);
        assert_eq!(ui.rect(small).width(), 100.0);
    }

    #[test]
    fn click_and_radio_events() {
        let mut ui = Ui::new(Theme::neko_dark());
        let col = ui.add(ui.root(), Control::vbox());
        let btn = ui.add(col, Control::button("Create"));
        let r1 = ui.add(col, Control::radio("wgpu", true));
        let r2 = ui.add(col, Control::radio("soft", false));
        layout(&mut ui, 300.0, 300.0);
        let click = |ui: &mut Ui, id: Id| {
            let c = ui.rect(id).center();
            ui.on_event(&Event::MouseMoved { x: c.x, y: c.y });
            ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: true });
            ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: false });
        };
        click(&mut ui, btn);
        click(&mut ui, r2);
        let ev = ui.take_events();
        assert!(ev.contains(&UiEvent::Pressed(btn)));
        assert!(ev.contains(&UiEvent::Toggled(r2, true)));
        assert!(!ui.is_checked(r1) && ui.is_checked(r2));
    }

    #[test]
    fn line_edit_typing() {
        let mut ui = Ui::new(Theme::neko_dark());
        let le = ui.add(ui.root(), Control::line_edit("cat"));
        layout(&mut ui, 300.0, 40.0);
        ui.focus(le);
        ui.on_event(&Event::Text("dog".into())); // select-all on focus replaces
        ui.on_event(&Event::Key { key: Key::Backspace, pressed: true, repeat: false, modifiers: Modifiers::default() });
        ui.on_event(&Event::Text("g!".into()));
        assert_eq!(ui.text(le), "dog!");
        assert!(ui.take_events().contains(&UiEvent::TextChanged(le)));
    }

    fn mouse(ui: &mut Ui, p: Pos2) {
        ui.on_event(&Event::MouseMoved { x: p.x, y: p.y });
    }

    fn click_at(ui: &mut Ui, p: Pos2) {
        mouse(ui, p);
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: true });
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: false });
    }

    #[test]
    fn menubar_submenu_action() {
        let mut ui = Ui::new(Theme::neko_dark());
        let menus = vec![Menu {
            title: "Scene".into(),
            items: vec![
                MenuItem::item("New Scene", 1).shortcut("Ctrl+N"),
                MenuItem::separator(),
                MenuItem::submenu("Export As...", vec![MenuItem::item("MeshLibrary...", 7)]),
            ],
        }];
        let bar = ui.add(ui.root(), Control::menu_bar(menus));
        layout(&mut ui, 800.0, 600.0);
        let title = ui.hit_rects[&bar][0];
        click_at(&mut ui, title.center());
        assert!(ui.menu_open());
        let sub_row = ui.menus[0].rows[2];
        mouse(&mut ui, sub_row.center());
        assert_eq!(ui.menus.len(), 2, "hovering opens the submenu");
        let item = ui.menus[1].rows[0];
        mouse(&mut ui, item.center());
        click_at(&mut ui, item.center());
        assert!(ui.take_events().contains(&UiEvent::Menu(7)));
        assert!(!ui.menu_open());
    }

    #[test]
    fn tree_select_toggle_drop_and_rename() {
        let mut ui = Ui::new(Theme::neko_dark());
        let tree = ui.add(ui.root(), Control::tree());
        let mut a = TreeRow::new(1, 0, "Main");
        a.has_children = true;
        let mut b = TreeRow::new(2, 1, "Player");
        b.draggable = true;
        let mut c = TreeRow::new(3, 1, "Camera");
        c.draggable = true;
        ui.set_rows(tree, vec![a, b, c]);
        layout(&mut ui, 300.0, 300.0);
        let r = ui.rect(tree);
        let row = |i: f32| pos2(r.left() + 120.0, r.top() + widgets::TREE_ROW * (i + 0.5));
        click_at(&mut ui, row(1.0));
        assert_eq!(ui.tree_selected(tree), Some(2));
        ui.on_event(&Event::Key { key: Key::Enter, pressed: true, repeat: false, modifiers: Modifiers::default() });
        assert!(ui.take_events().contains(&UiEvent::TreeActivated(tree, 2)));
        click_at(&mut ui, pos2(r.left() + 8.0, row(0.0).y));
        click_at(&mut ui, row(2.0)); // select another row so the next press is not a double-click
        // Drag Player onto Camera.
        mouse(&mut ui, row(1.0));
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: true });
        mouse(&mut ui, row(1.6));
        mouse(&mut ui, row(2.0));
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: false });
        let ev = ui.take_events();
        assert!(ev.contains(&UiEvent::TreeSelected(tree, 2)));
        assert!(ev.contains(&UiEvent::TreeToggled(tree, 1, false)));
        assert!(ev.contains(&UiEvent::TreeDropped { tree, dragged: 2, target: 3 }));
        ui.edit_tree_row(tree, 2);
        ui.on_event(&Event::Text("Hero".into()));
        ui.on_event(&Event::Key { key: Key::Enter, pressed: true, repeat: false, modifiers: Modifiers::default() });
        assert!(ui.take_events().contains(&UiEvent::TreeEdited(tree, 2, "Hero".into())));
    }

    #[test]
    fn spin_drag_and_type() {
        let mut ui = Ui::new(Theme::neko_dark());
        let spin = ui.add(ui.root(), Control::spin_box(10.0, 1.0).anchors(Anchors::CENTER).min(120.0, 0.0));
        layout(&mut ui, 400.0, 100.0);
        let c = ui.rect(spin).center();
        mouse(&mut ui, c);
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: true });
        mouse(&mut ui, c + vec2(5.0, 0.0));
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: false });
        assert_eq!(ui.value(spin), 15.0);
        click_at(&mut ui, c);
        ui.on_event(&Event::Text("2.5".into()));
        ui.on_event(&Event::Key { key: Key::Enter, pressed: true, repeat: false, modifiers: Modifiers::default() });
        assert_eq!(ui.value(spin), 2.5);
        assert!(ui.take_events().contains(&UiEvent::ValueChanged(spin, 2.5)));
    }

    #[test]
    fn split_divider_drags() {
        let mut ui = Ui::new(Theme::neko_dark());
        let split = ui.add(ui.root(), Control::h_split(200.0).anchors(Anchors::FULL_RECT));
        let left = ui.add(split, Control::panel());
        ui.add(split, Control::panel());
        layout(&mut ui, 800.0, 400.0);
        assert_eq!(ui.rect(left).width(), 200.0);
        mouse(&mut ui, pos2(204.0, 100.0));
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: true });
        mouse(&mut ui, pos2(304.0, 100.0));
        ui.on_event(&Event::MouseButton { button: MouseButton::Left, pressed: false });
        layout(&mut ui, 800.0, 400.0);
        assert_eq!(ui.rect(left).width(), 300.0);
    }
}
