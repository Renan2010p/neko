//! Editor-grade widgets modeled on Godot: MenuBar + PopupMenu, TabBar,
//! SplitContainer, EditorSpinSlider, ColorPickerButton, TextureRect, Tree,
//! PopupPanel and a Canvas for viewports.

use neko_platform::{Key, Modifiers, MouseButton};
use neko_render::epaint::textures::TextureOptions;
use neko_render::epaint::{ClippedShape, Color32, ColorImage, CornerRadius, ImageData, ImageDelta, Shape, Stroke, StrokeKind, TextureId};

use crate::control::{Control, Image, Kind, Menu, MenuItem, Tab, TextEdit, TreeRow};
use crate::geom::{Pos2, Rect, Vec2, pos2, vec2};
use crate::{Id, Painter, Ui, UiEvent};

pub(crate) const TREE_ROW: f32 = 26.0;
const TREE_INDENT: f32 = 16.0;
/// Width of a row's icon button.
const TREE_BUTTON: f32 = 22.0;
const MENU_ROW: f32 = 26.0;
const MENU_SEP: f32 = 9.0;
const SPLIT_GRAB: f32 = 8.0;
const TAB_CLOSE: f32 = 18.0;

/// Mouse input on a [`Kind::Canvas`], in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasInput {
    Press { button: MouseButton, pos: Pos2 },
    Release { button: MouseButton, pos: Pos2 },
    /// Movement while `button` is held. `delta` since the previous event.
    Drag { button: MouseButton, pos: Pos2, delta: Vec2 },
    Move { pos: Pos2 },
    /// Wheel lines or points (positive = away from the user).
    Wheel { pos: Pos2, delta: Vec2 },
}

pub(crate) struct OpenMenu {
    pub items: Vec<MenuItem>,
    pub rect: Rect,
    pub rows: Vec<Rect>,
    pub hovered: Option<usize>,
    /// Index of the item in the parent level that opened this submenu.
    pub from: Option<usize>,
}

pub(crate) enum Drag {
    Split { id: Id, start_offset: f32, start: Pos2 },
    Spin { id: Id, last_x: f32, moved: f32 },
    Canvas { id: Id, button: MouseButton, last: Pos2 },
    Tree { id: Id, key: u64, start: Pos2, active: bool },
    /// Selecting text in a CodeEdit.
    Code { id: Id },
}

fn fmt_value(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if decimals > 0 && s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

impl Ui {
    // ------------------------------------------------------------ app API

    /// Upload an image and get a texture id for [`Image`]s. Godot's ImageTexture.
    pub fn add_texture(&mut self, image: ColorImage) -> TextureId {
        let id = crate::next_texture_id();
        self.pending_textures.push((id, ImageDelta::full(ImageData::Color(image.into()), TextureOptions::LINEAR)));
        id
    }

    pub fn set_menus(&mut self, id: Id, new: Vec<Menu>) {
        if let Some(Control { kind: Kind::MenuBar { menus }, .. }) = self.get_mut(id) {
            *menus = new;
        }
    }

    /// Open a PopupMenu at `pos` (context menus). Clicks report [`UiEvent::Menu`].
    pub fn open_menu(&mut self, items: Vec<MenuItem>, pos: Pos2) {
        // Opened in response to a native window's event: show it there.
        if let Some(w) = self.menu_target() {
            self.with_window(w, |ui| ui.open_menu(items, pos));
            return;
        }
        self.menus.clear();
        self.menubar_open = None;
        let level = self.layout_menu(items, pos, None);
        self.menus.push(level);
    }

    pub fn menu_open(&self) -> bool {
        !self.menus.is_empty()
    }

    pub fn close_menus(&mut self) {
        self.menus.clear();
        self.menubar_open = None;
    }

    pub fn set_tabs(&mut self, id: Id, new: Vec<Tab>, cur: usize) {
        if let Some(Control { kind: Kind::TabBar { tabs, current, .. }, .. }) = self.get_mut(id) {
            *tabs = new;
            *current = cur;
        }
    }

    /// Move a SplitContainer's divider (the app restores a saved dock layout).
    pub fn set_split_offset(&mut self, id: Id, offset: f32) {
        if let Some(Control { kind: Kind::Split { offset: o, .. }, .. }) = self.get_mut(id) {
            *o = offset;
        }
    }

    pub fn value(&self, id: Id) -> f64 {
        match self.get(id).map(|c| &c.kind) {
            Some(Kind::SpinBox { value, .. }) => *value,
            _ => 0.0,
        }
    }

    pub fn set_value(&mut self, id: Id, v: f64) {
        if let Some(Control { kind: Kind::SpinBox { value, edit: None, min, max, .. }, .. }) = self.get_mut(id) {
            *value = v.clamp(*min, *max);
        }
    }

    pub fn color(&self, id: Id) -> Color32 {
        match self.get(id).map(|c| &c.kind) {
            Some(Kind::ColorButton { color }) => *color,
            _ => Color32::WHITE,
        }
    }

    pub fn set_color(&mut self, id: Id, c: Color32) {
        if let Some(Control { kind: Kind::ColorButton { color }, .. }) = self.get_mut(id) {
            *color = c;
        }
    }

    pub fn set_image(&mut self, id: Id, img: Image) {
        if let Some(Control { kind: Kind::Image { image }, min_size, .. }) = self.get_mut(id) {
            *image = img;
            *min_size = img.size;
        }
    }

    /// Replace a Tree's rows; selection and scroll are kept.
    pub fn set_rows(&mut self, id: Id, new: Vec<TreeRow>) {
        if let Some(Control { kind: Kind::Tree { rows, .. }, .. }) = self.get_mut(id) {
            *rows = new;
        }
    }

    pub fn tree_selected(&self, id: Id) -> Option<u64> {
        match self.get(id).map(|c| &c.kind) {
            Some(Kind::Tree { selected, .. }) => *selected,
            _ => None,
        }
    }

    pub fn set_tree_selected(&mut self, id: Id, key: Option<u64>) {
        if let Some(Control { kind: Kind::Tree { selected, .. }, .. }) = self.get_mut(id) {
            *selected = key;
        }
    }

    /// Start editing a row's text in place (Godot's Tree::edit_selected).
    pub fn edit_tree_row(&mut self, id: Id, key: u64) {
        let text = match self.get(id).map(|c| &c.kind) {
            Some(Kind::Tree { rows, .. }) => rows.iter().find(|r| r.key == key).map(|r| r.text.clone()),
            _ => None,
        };
        if let Some(text) = text
            && let Some(Control { kind: Kind::Tree { edit, .. }, .. }) = self.get_mut(id)
        {
            *edit = Some((key, TextEdit::new(text)));
            self.focus = Some(id);
        }
    }

    /// Draw a Canvas's content for the next frame. `rect` is its last layout rect.
    pub fn paint_canvas(&mut self, id: Id, f: impl FnOnce(&mut Painter, Rect)) {
        let rect = self.rect(id);
        if !rect.is_positive() {
            return; // not laid out yet (first frame) or hidden
        }
        let mut shapes = Vec::new();
        {
            let mut p = Painter { shapes: &mut shapes, clip: rect, text: &mut self.text };
            f(&mut p, rect);
        }
        if let Some(Control { kind: Kind::Canvas { shapes: s }, .. }) = self.get_mut(id) {
            *s = shapes.into_iter().map(|c| c.shape).collect();
        }
    }

    /// Show a Popup control with its child placed at `at`.
    pub fn open_popup(&mut self, id: Id, at: Pos2) {
        if let Some(c) = self.get_mut(id) {
            c.visible = true;
            if let Kind::Popup { at: a } = &mut c.kind {
                *a = at;
            }
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn mouse_pos(&self) -> Option<Pos2> {
        self.mouse
    }

    // ------------------------------------------------------------- layout

    pub(crate) fn widget_min(&mut self, c: &Control, kids: &[Vec2]) -> Vec2 {
        let fs = c.font_size.unwrap_or(self.theme.font_size);
        let row = self.text.row_height(fs);
        let field_h = row + self.theme.line_edit.padding.size().y;
        match &c.kind {
            Kind::MenuBar { menus } => {
                let w: f32 = menus.iter().map(|m| self.text.layout(&m.title, fs, Color32::WHITE).size().x + 16.0).sum();
                vec2(w, row + 10.0)
            }
            Kind::TabBar { .. } => vec2(0.0, row + 14.0),
            Kind::Split { vertical, .. } => {
                let a = kids.first().copied().unwrap_or_default();
                let b = kids.get(1).copied().unwrap_or_default();
                if *vertical { vec2(a.x.max(b.x), a.y + b.y + SPLIT_GRAB) } else { vec2(a.x + b.x + SPLIT_GRAB, a.y.max(b.y)) }
            }
            Kind::SpinBox { .. } => vec2(56.0, field_h),
            Kind::ColorButton { .. } => vec2(48.0, field_h),
            Kind::Image { image } => image.size,
            _ => Vec2::ZERO,
        }
    }

    pub(crate) fn arrange_split(&mut self, id: Id, rect: Rect, vertical: bool, offset: f32, kids: &[Id]) {
        let axis = |v: Vec2| if vertical { v.y } else { v.x };
        let total = axis(rect.size());
        let min_a = kids.first().map_or(0.0, |&k| axis(self.min_of(k)));
        let min_b = kids.get(1).map_or(0.0, |&k| axis(self.min_of(k)));
        let off = offset.clamp(min_a, (total - SPLIT_GRAB - min_b).max(min_a));
        if let Some(Control { kind: Kind::Split { offset, .. }, .. }) = self.get_mut(id) {
            *offset = off;
        }
        let (a, b) = if vertical {
            (Rect::from_min_size(rect.min, vec2(rect.width(), off)), Rect::from_min_max(pos2(rect.left(), rect.top() + off + SPLIT_GRAB), rect.max))
        } else {
            (Rect::from_min_size(rect.min, vec2(off, rect.height())), Rect::from_min_max(pos2(rect.left() + off + SPLIT_GRAB, rect.top()), rect.max))
        };
        if let Some(&k) = kids.first() {
            self.fit(k, a);
        }
        if let Some(&k) = kids.get(1) {
            self.fit(k, b);
        }
    }

    pub(crate) fn split_divider(&self, c: &Control) -> Option<Rect> {
        let Kind::Split { vertical, offset } = c.kind else { return None };
        let r = c.rect;
        Some(if vertical {
            Rect::from_min_size(pos2(r.left(), r.top() + offset), vec2(r.width(), SPLIT_GRAB))
        } else {
            Rect::from_min_size(pos2(r.left() + offset, r.top()), vec2(SPLIT_GRAB, r.height()))
        })
    }

    // ------------------------------------------------------------- menus

    fn layout_menu(&mut self, items: Vec<MenuItem>, at: Pos2, from: Option<usize>) -> OpenMenu {
        let fs = self.theme.font_size;
        let mut w: f32 = 180.0;
        for it in &items {
            if it.separator {
                continue;
            }
            let lw = self.text.layout(&it.label, fs, Color32::WHITE).size().x;
            let sw = if it.shortcut.is_empty() { 0.0 } else { self.text.layout(&it.shortcut, fs, Color32::WHITE).size().x + 32.0 };
            w = w.max(32.0 + lw + sw + 28.0);
        }
        let pad = self.theme.popup_menu.padding.top;
        let h: f32 = items.iter().map(|i| if i.separator { MENU_SEP } else { MENU_ROW }).sum::<f32>() + pad * 2.0;
        let mut rect = Rect::from_min_size(at, vec2(w, h));
        if rect.right() > self.screen.right() {
            rect = rect.translate(vec2(self.screen.right() - rect.right(), 0.0));
        }
        if rect.bottom() > self.screen.bottom() {
            rect = rect.translate(vec2(0.0, (self.screen.bottom() - rect.bottom()).max(self.screen.top() - rect.top())));
        }
        let mut y = rect.top() + pad;
        let rows = items
            .iter()
            .map(|i| {
                let hgt = if i.separator { MENU_SEP } else { MENU_ROW };
                let r = Rect::from_min_size(pos2(rect.left() + pad, y), vec2(rect.width() - pad * 2.0, hgt));
                y += hgt;
                r
            })
            .collect();
        OpenMenu { items, rect, rows, hovered: None, from }
    }

    fn open_menubar(&mut self, id: Id, index: usize) {
        let (items, title) = match self.get(id).map(|c| &c.kind) {
            Some(Kind::MenuBar { menus }) => match menus.get(index) {
                Some(m) => (m.items.clone(), self.hit_rects.get(&id).and_then(|r| r.get(index)).copied()),
                None => return,
            },
            _ => return,
        };
        let Some(title) = title else { return };
        self.menus.clear();
        let level = self.layout_menu(items, title.left_bottom(), None);
        self.menus.push(level);
        self.menubar_open = Some((id, index));
    }

    /// Mouse moved while menus are open. Returns true if the event was used.
    pub(crate) fn menus_hover(&mut self, p: Pos2) -> bool {
        if self.menus.is_empty() {
            return false;
        }
        // Switch menus by hovering another MenuBar title, like Godot.
        if let Some((bar, open)) = self.menubar_open
            && let Some(i) = self.hit_rects.get(&bar).and_then(|rs| rs.iter().position(|r| r.contains(p)))
        {
            if i != open {
                self.open_menubar(bar, i);
            }
            return true;
        }
        let Some(level) = self.menus.iter().rposition(|m| m.rect.contains(p)) else {
            if let Some(m) = self.menus.last_mut() {
                m.hovered = None;
            }
            return true;
        };
        let row = self.menus[level].rows.iter().position(|r| r.contains(p));
        let item = row.and_then(|i| self.menus[level].items.get(i).cloned());
        let usable = item.as_ref().is_some_and(|it| !it.separator && it.enabled);
        self.menus[level].hovered = if usable { row } else { None };
        let opened_here = self.menus.get(level + 1).and_then(|m| m.from);
        if let (Some(i), Some(it)) = (row, item)
            && usable
            && !it.submenu.is_empty()
        {
            if opened_here != Some(i) {
                self.menus.truncate(level + 1);
                let r = self.menus[level].rows[i];
                let sub = self.layout_menu(it.submenu.clone(), pos2(r.right() + 4.0, r.top() - 4.0), Some(i));
                self.menus.push(sub);
            }
        } else if row.is_some() {
            self.menus.truncate(level + 1);
        }
        true
    }

    /// Mouse pressed while menus are open. Returns true if consumed.
    pub(crate) fn menus_press(&mut self, p: Pos2) -> bool {
        if self.menus.is_empty() {
            return false;
        }
        if let Some((bar, open)) = self.menubar_open
            && self.hit_rects.get(&bar).and_then(|rs| rs.get(open)).is_some_and(|r| r.contains(p))
        {
            self.close_menus();
            return true;
        }
        if let Some(level) = self.menus.iter().rposition(|m| m.rect.contains(p)) {
            if let Some(i) = self.menus[level].rows.iter().position(|r| r.contains(p)) {
                let it = self.menus[level].items[i].clone();
                if !it.separator && it.enabled && it.submenu.is_empty() {
                    self.events.push(UiEvent::Menu(it.action));
                    self.close_menus();
                }
            }
            return true;
        }
        self.close_menus();
        true
    }

    pub(crate) fn paint_menus(&mut self, out: &mut Vec<ClippedShape>) {
        let t = self.theme.clone();
        let screen = self.screen;
        let fs = t.font_size;
        // Snapshot the open levels so text layout can borrow `self` mutably.
        type Level = (Vec<MenuItem>, Rect, Vec<Rect>, Option<usize>);
        let levels: Vec<Level> = self.menus.iter().map(|m| (m.items.clone(), m.rect, m.rows.clone(), m.hovered)).collect();
        let open_children: Vec<Option<usize>> = (0..levels.len()).map(|l| self.menus.get(l + 1).and_then(|m| m.from)).collect();
        for (l, (items, rect, rows, hovered)) in levels.into_iter().enumerate() {
            out.push(ClippedShape::new(screen, Shape::rect_filled(rect.translate(vec2(0.0, 3.0)).expand(2.0), t.popup_menu.corner(), Color32::from_black_alpha(60))));
            out.push(ClippedShape::new(screen, Shape::rect_filled(rect, t.popup_menu.corner(), t.popup_menu.bg)));
            out.push(ClippedShape::new(screen, Shape::rect_stroke(rect, t.popup_menu.corner(), t.popup_menu.border, StrokeKind::Inside)));
            for (i, (it, r)) in items.iter().zip(rows).enumerate() {
                if it.separator {
                    out.push(ClippedShape::new(screen, Shape::line_segment([pos2(r.left() + 4.0, r.center().y), pos2(r.right() - 4.0, r.center().y)], Stroke::new(1.0, t.separator))));
                    continue;
                }
                let active = hovered == Some(i) || open_children[l] == Some(i);
                if active {
                    out.push(ClippedShape::new(screen, Shape::rect_filled(r, CornerRadius::same(3), t.popup_hover)));
                }
                let color = if !it.enabled { t.font_disabled_color } else if active { t.font_focus_color } else { t.font_color };
                if let Some(on) = it.checked {
                    let b = Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), Vec2::splat(12.0));
                    if on {
                        let s = Stroke::new(2.0, if it.enabled { t.accent } else { t.font_disabled_color });
                        out.push(ClippedShape::new(screen, Shape::line_segment([b.min + vec2(1.5, 6.5), b.min + vec2(4.5, 9.5)], s)));
                        out.push(ClippedShape::new(screen, Shape::line_segment([b.min + vec2(4.5, 9.5), b.min + vec2(10.5, 2.5)], s)));
                    } else {
                        out.push(ClippedShape::new(screen, Shape::rect_stroke(b, CornerRadius::same(2), Stroke::new(1.0, t.font_disabled_color), StrokeKind::Inside)));
                    }
                } else if let Some(img) = it.icon {
                    let ir = Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), Vec2::splat(16.0));
                    out.push(ClippedShape::new(screen, Shape::image(img.texture, ir, img.uv, if it.enabled { img.tint } else { img.tint.gamma_multiply(0.4) })));
                }
                let g = self.text.layout(&it.label, fs, color);
                out.push(ClippedShape::new(screen, Shape::galley(pos2(r.left() + 30.0, r.center().y - g.size().y / 2.0), g, color)));
                if !it.shortcut.is_empty() {
                    let sc = if it.enabled { t.font_muted_color } else { t.font_disabled_color };
                    let g = self.text.layout(&it.shortcut, fs, sc);
                    out.push(ClippedShape::new(screen, Shape::galley(pos2(r.right() - 10.0 - g.size().x, r.center().y - g.size().y / 2.0), g, sc)));
                }
                if !it.submenu.is_empty() {
                    let a = pos2(r.right() - 12.0, r.center().y);
                    out.push(ClippedShape::new(screen, Shape::convex_polygon(vec![a + vec2(-2.5, -4.5), a + vec2(2.5, 0.0), a + vec2(-2.5, 4.5)], color, Stroke::NONE)));
                }
            }
        }
    }

    // --------------------------------------------------------- pointer input

    /// Left press on one of the new widgets. Returns true if handled.
    pub(crate) fn widget_press(&mut self, id: Id, p: Pos2) -> bool {
        let Some(c) = self.get(id).cloned() else { return false };
        if c.disabled {
            return false;
        }
        match &c.kind {
            Kind::CodeEdit(h) => {
                let now = std::time::Instant::now();
                let double = self.last_click.is_some_and(|(l, t)| l == id && now - t < crate::DOUBLE_CLICK);
                self.last_click = if double { None } else { Some((id, now)) };
                let h = h.clone();
                if !self.code_press(id, &h, p, double) {
                    self.drag = Some(Drag::Code { id });
                }
                true
            }
            Kind::MenuBar { .. } => {
                if let Some(i) = self.hit_rects.get(&id).and_then(|rs| rs.iter().position(|r| r.contains(p))) {
                    self.open_menubar(id, i);
                }
                true
            }
            Kind::TabBar { tabs, closable, add_button, .. } => {
                let rects = self.hit_rects.get(&id).cloned().unwrap_or_default();
                if let Some(i) = rects.iter().take(tabs.len()).position(|r| r.contains(p)) {
                    let r = rects[i];
                    if *closable && p.x > r.right() - TAB_CLOSE - 4.0 {
                        self.events.push(UiEvent::TabClosed(id, i));
                    } else {
                        if let Some(Control { kind: Kind::TabBar { current, .. }, .. }) = self.get_mut(id) {
                            *current = i;
                        }
                        self.events.push(UiEvent::TabSelected(id, i));
                    }
                } else if *add_button && rects.get(tabs.len()).is_some_and(|r| r.contains(p)) {
                    self.events.push(UiEvent::TabAdded(id));
                }
                true
            }
            Kind::Split { offset, .. } => {
                if self.split_divider(&c).is_some_and(|d| d.contains(p)) {
                    self.drag = Some(Drag::Split { id, start_offset: *offset, start: p });
                }
                true
            }
            Kind::SpinBox { edit: None, .. } => {
                self.commit_edits(Some(id));
                self.drag = Some(Drag::Spin { id, last_x: p.x, moved: 0.0 });
                true
            }
            Kind::SpinBox { edit: Some(_), .. } => true,
            Kind::Canvas { .. } => {
                self.commit_edits(None);
                self.focus = Some(id);
                self.drag = Some(Drag::Canvas { id, button: MouseButton::Left, last: p });
                self.events.push(UiEvent::Canvas(id, CanvasInput::Press { button: MouseButton::Left, pos: p }));
                true
            }
            Kind::Tree { rows, offset, .. } => {
                self.commit_edits(Some(id));
                self.focus = Some(id);
                let i = ((p.y - c.rect.top() + offset) / TREE_ROW).floor();
                if i < 0.0 {
                    return true;
                }
                let Some(row) = rows.get(i as usize).cloned() else {
                    self.set_tree_selected(id, None);
                    return true;
                };
                // Row buttons sit at the right edge, last button rightmost.
                let n = row.buttons.len();
                if n > 0 && p.x >= c.rect.right() - 6.0 - n as f32 * TREE_BUTTON {
                    let b = ((c.rect.right() - 6.0 - p.x) / TREE_BUTTON).floor() as usize;
                    if b < n {
                        self.events.push(UiEvent::TreeButton(id, row.key, n - 1 - b));
                        return true;
                    }
                }
                let arrow_x = c.rect.left() + 4.0 + row.depth as f32 * TREE_INDENT;
                if row.has_children && p.x >= arrow_x && p.x < arrow_x + 14.0 {
                    self.events.push(UiEvent::TreeToggled(id, row.key, !row.open));
                    return true;
                }
                let now = std::time::Instant::now();
                let double = self.last_click.is_some_and(|(l, t)| l == id && now - t < crate::DOUBLE_CLICK) && self.tree_selected(id) == Some(row.key);
                self.last_click = if double { None } else { Some((id, now)) };
                self.set_tree_selected(id, Some(row.key));
                self.events.push(UiEvent::TreeSelected(id, row.key));
                if double {
                    self.events.push(UiEvent::TreeActivated(id, row.key));
                } else if row.draggable {
                    self.drag = Some(Drag::Tree { id, key: row.key, start: p, active: false });
                }
                true
            }
            Kind::Popup { .. } => {
                // Clicked outside the popup's content.
                if let Some(c) = self.get_mut(id) {
                    c.visible = false;
                }
                self.events.push(UiEvent::PopupClosed(id));
                true
            }
            _ => false,
        }
    }

    /// Right or middle press. Returns true if handled.
    pub(crate) fn other_press(&mut self, button: MouseButton, p: Pos2) -> bool {
        if self.menus_press(p) {
            return true;
        }
        let Some(id) = self.hit(p) else { return false };
        let Some(c) = self.get(id).cloned() else { return false };
        match &c.kind {
            Kind::Canvas { .. } => {
                self.drag = Some(Drag::Canvas { id, button, last: p });
                self.events.push(UiEvent::Canvas(id, CanvasInput::Press { button, pos: p }));
            }
            Kind::Tree { rows, offset, .. } if button == MouseButton::Right => {
                let i = ((p.y - c.rect.top() + offset) / TREE_ROW).floor();
                let key = (i >= 0.0).then(|| rows.get(i as usize).map(|r| r.key)).flatten();
                if let Some(k) = key {
                    self.set_tree_selected(id, Some(k));
                    self.events.push(UiEvent::TreeSelected(id, k));
                }
                self.events.push(UiEvent::TreeContext(id, key, p));
            }
            _ if button == MouseButton::Right => self.events.push(UiEvent::RightClicked(id, p)),
            _ => return false,
        }
        true
    }

    pub(crate) fn other_release(&mut self, button: MouseButton, p: Pos2) {
        if let Some(Drag::Canvas { id, button: b, .. }) = self.drag
            && b == button
        {
            self.drag = None;
            self.events.push(UiEvent::Canvas(id, CanvasInput::Release { button, pos: p }));
        }
    }

    /// Mouse moved: drive drags. Returns true if a drag used it.
    pub(crate) fn drag_move(&mut self, p: Pos2) -> bool {
        let Some(drag) = self.drag.take() else { return false };
        self.drag = Some(match drag {
            Drag::Code { id } => {
                if let Some(Kind::CodeEdit(h)) = self.get(id).map(|c| &c.kind) {
                    h.0.borrow_mut().drag_to(p);
                }
                Drag::Code { id }
            }
            Drag::Split { id, start_offset, start } => {
                let vertical = matches!(self.get(id).map(|c| &c.kind), Some(Kind::Split { vertical: true, .. }));
                let d = if vertical { p.y - start.y } else { p.x - start.x };
                if let Some(Control { kind: Kind::Split { offset, .. }, .. }) = self.get_mut(id) {
                    *offset = start_offset + d;
                }
                Drag::Split { id, start_offset, start }
            }
            Drag::Spin { id, last_x, moved } => {
                let dx = p.x - last_x;
                let fine = self.modifiers.shift;
                if let Some(Control { kind: Kind::SpinBox { value, step, min, max, .. }, .. }) = self.get_mut(id) {
                    let s = if fine { *step * 0.1 } else { *step };
                    *value = (*value + dx as f64 * s).clamp(*min, *max);
                    let v = *value;
                    if dx != 0.0 {
                        self.events.push(UiEvent::ValueChanged(id, v));
                    }
                }
                Drag::Spin { id, last_x: p.x, moved: moved + dx.abs() }
            }
            Drag::Canvas { id, button, last } => {
                self.events.push(UiEvent::Canvas(id, CanvasInput::Drag { button, pos: p, delta: p - last }));
                Drag::Canvas { id, button, last: p }
            }
            Drag::Tree { id, key, start, active } => Drag::Tree { id, key, start, active: active || p.distance(start) > 5.0 },
        });
        true
    }

    /// Left release ending a drag. Returns true if a drag was active.
    pub(crate) fn drag_release(&mut self, p: Pos2) -> bool {
        let Some(drag) = self.drag.take() else { return false };
        match drag {
            Drag::Split { .. } | Drag::Code { .. } => {}
            Drag::Spin { id, moved, .. } => {
                if moved < 2.0 {
                    // A click without dragging types a value, like Godot's EditorSpinSlider.
                    let text = match self.get(id).map(|c| &c.kind) {
                        Some(Kind::SpinBox { value, decimals, .. }) => fmt_value(*value, *decimals),
                        _ => String::new(),
                    };
                    if let Some(Control { kind: Kind::SpinBox { edit, .. }, .. }) = self.get_mut(id) {
                        *edit = Some(TextEdit::new(text));
                    }
                    self.focus = Some(id);
                }
            }
            Drag::Canvas { id, button, .. } => {
                self.events.push(UiEvent::Canvas(id, CanvasInput::Release { button, pos: p }));
                if button != MouseButton::Left {
                    return true;
                }
            }
            Drag::Tree { id, key, active, .. } => {
                if active
                    && let Some(target) = self.tree_row_at(id, p)
                    && target != key
                {
                    self.events.push(UiEvent::TreeDropped { tree: id, dragged: key, target });
                }
            }
        }
        true
    }

    /// The key of the Tree row under a point, if any (drag-and-drop targets).
    pub fn tree_row_at(&self, id: Id, p: Pos2) -> Option<u64> {
        let c = self.get(id)?;
        let Kind::Tree { rows, offset, .. } = &c.kind else { return None };
        if !c.rect.contains(p) {
            return None;
        }
        let i = ((p.y - c.rect.top() + offset) / TREE_ROW).floor();
        (i >= 0.0).then(|| rows.get(i as usize).map(|r| r.key)).flatten()
    }

    /// Wheel over a Tree or Canvas. Returns true if used.
    pub(crate) fn widget_wheel(&mut self, delta: Vec2) -> bool {
        let Some(h) = self.hovered else { return false };
        let pos = self.mouse.unwrap_or_default();
        let Some(c) = self.get_mut(h) else { return false };
        match &mut c.kind {
            Kind::Tree { rows, offset, .. } => {
                let max = (rows.len() as f32 * TREE_ROW - c.rect.height()).max(0.0);
                *offset = (*offset - delta.y * 40.0).clamp(0.0, max);
                true
            }
            Kind::Canvas { .. } => {
                self.events.push(UiEvent::Canvas(h, CanvasInput::Wheel { pos, delta }));
                true
            }
            Kind::CodeEdit(code) => {
                let code = code.clone();
                if code.0.borrow_mut().completion_wheel(pos, delta.y) {
                    return true;
                }
                // Shift+wheel scrolls sideways, as in Godot.
                let (dx, dy) = if self.modifiers.shift { (delta.y, 0.0) } else { (delta.x, delta.y) };
                code.0.borrow_mut().wheel(dx, dy);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn canvas_hover(&mut self, p: Pos2) {
        if let Some(h) = self.hovered
            && matches!(self.get(h).map(|c| &c.kind), Some(Kind::Canvas { .. }))
            && self.drag.is_none()
        {
            self.events.push(UiEvent::Canvas(h, CanvasInput::Move { pos: p }));
        }
    }

    // --------------------------------------------------------- text editing

    /// The text being edited in `id`, if any (LineEdit, SpinBox or Tree row).
    pub(crate) fn edit_of(&mut self, id: Id) -> Option<(&mut String, &mut usize, &mut bool)> {
        match &mut self.get_mut(id)?.kind {
            Kind::LineEdit { text, cursor, select_all, .. } => Some((text, cursor, select_all)),
            Kind::SpinBox { edit: Some(e), .. } | Kind::Tree { edit: Some((_, e)), .. } => Some((&mut e.text, &mut e.cursor, &mut e.select_all)),
            _ => None,
        }
    }

    /// Finish in-place edits (SpinBox, Tree), except in `keep`.
    pub(crate) fn commit_edits(&mut self, keep: Option<Id>) {
        let Some(f) = self.focus else { return };
        if Some(f) == keep {
            return;
        }
        self.finish_edit(f, true);
    }

    /// Enter (commit) or Escape (cancel) on an in-place edit. Returns true if `id` was editing.
    pub(crate) fn finish_edit(&mut self, id: Id, commit: bool) -> bool {
        let Some(c) = self.get_mut(id) else { return false };
        match &mut c.kind {
            Kind::SpinBox { edit, value, min, max, .. } => {
                let Some(e) = edit.take() else { return false };
                if commit && let Ok(v) = e.text.trim().replace(',', ".").parse::<f64>() {
                    *value = v.clamp(*min, *max);
                    let v = *value;
                    self.events.push(UiEvent::ValueChanged(id, v));
                }
                true
            }
            Kind::Tree { edit, .. } => {
                let Some((key, e)) = edit.take() else { return false };
                if commit {
                    self.events.push(UiEvent::TreeEdited(id, key, e.text));
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn widget_key(&mut self, key: Key, _m: Modifiers) -> bool {
        let Some(f) = self.focus else { return false };
        let editing = matches!(self.get(f).map(|c| &c.kind), Some(Kind::SpinBox { edit: Some(_), .. } | Kind::Tree { edit: Some(_), .. }));
        match key {
            Key::Enter if editing => {
                self.finish_edit(f, true);
                true
            }
            Key::Enter => {
                // Godot's Tree activates the selected item on Enter (item_activated).
                if let Some(Kind::Tree { selected, .. }) = self.get(f).map(|c| &c.kind)
                    && let Some(key) = *selected
                {
                    self.events.push(UiEvent::TreeActivated(f, key));
                }
                true
            }
            Key::Escape if editing => {
                self.finish_edit(f, false);
                true
            }
            _ => false,
        }
    }

    // ------------------------------------------------------------- painting

    pub(crate) fn paint_widget(&mut self, id: Id, c: &Control, clip: Rect, out: &mut Vec<ClippedShape>, hovered: bool) {
        let t = self.theme.clone();
        let r = c.rect;
        let fs = c.font_size.unwrap_or(t.font_size);
        let push = |out: &mut Vec<ClippedShape>, s: Shape| out.push(ClippedShape::new(clip, s));
        match &c.kind {
            Kind::MenuBar { menus } => {
                let mut x = r.left();
                let mut rects = Vec::new();
                let open = self.menubar_open.filter(|(b, _)| *b == id).map(|(_, i)| i);
                for (i, m) in menus.iter().enumerate() {
                    let g = self.text.layout(&m.title, fs, t.font_color);
                    let tr = Rect::from_min_size(pos2(x, r.top()), vec2(g.size().x + 16.0, r.height()));
                    let hot = open == Some(i) || (hovered && self.mouse.is_some_and(|p| tr.contains(p)));
                    if hot {
                        push(out, Shape::rect_filled(tr.shrink2(vec2(0.0, 2.0)), CornerRadius::same(3), t.button_flat.hover.bg));
                    }
                    let color = if hot { t.font_focus_color } else { t.font_color };
                    push(out, Shape::galley(pos2(tr.left() + 8.0, tr.center().y - g.size().y / 2.0), g, color));
                    rects.push(tr);
                    x = tr.right();
                }
                self.hit_rects.insert(id, rects);
            }
            Kind::TabBar { tabs, current, closable, add_button } => {
                let mut x = r.left();
                let mut rects = Vec::new();
                for (i, tab) in tabs.iter().enumerate() {
                    let on = i == *current;
                    let color = if on { t.font_focus_color } else { t.font_color };
                    let g = self.text.layout(&tab.title, fs, color);
                    let icon_w = if tab.icon.is_some() { 20.0 } else { 0.0 };
                    let w = 24.0 + icon_w + g.size().x + if *closable { TAB_CLOSE + 4.0 } else { 0.0 };
                    let tr = Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height()));
                    let hot = hovered && self.mouse.is_some_and(|p| tr.contains(p));
                    let corner = CornerRadius { nw: t.corner_radius, ne: t.corner_radius, sw: 0, se: 0 };
                    if on {
                        push(out, Shape::rect_filled(tr, corner, t.panel.bg));
                        push(out, Shape::line_segment([tr.left_top() + vec2(2.0, 1.0), tr.right_top() + vec2(-2.0, 1.0)], Stroke::new(2.0, t.accent)));
                    } else if hot {
                        push(out, Shape::rect_filled(tr, corner, t.item_hover));
                    }
                    let mut tx = tr.left() + 12.0;
                    if let Some(img) = tab.icon {
                        push(out, Shape::image(img.texture, Rect::from_center_size(pos2(tx + 8.0, tr.center().y), Vec2::splat(16.0)), img.uv, img.tint));
                        tx += icon_w;
                    }
                    push(out, Shape::galley(pos2(tx, tr.center().y - g.size().y / 2.0), g, color));
                    if *closable && (on || hot) {
                        let cr = Rect::from_center_size(pos2(tr.right() - 6.0 - TAB_CLOSE / 2.0, tr.center().y), Vec2::splat(TAB_CLOSE));
                        let over = self.mouse.is_some_and(|p| cr.contains(p));
                        if over {
                            push(out, Shape::rect_filled(cr, CornerRadius::same(3), t.button_flat.hover.bg));
                        }
                        let s = Stroke::new(1.4, if over { t.font_focus_color } else { t.font_muted_color });
                        let c = cr.center();
                        push(out, Shape::line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], s));
                        push(out, Shape::line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], s));
                    }
                    rects.push(tr);
                    x = tr.right() + 2.0;
                }
                if *add_button {
                    let ar = Rect::from_min_size(pos2(x + 2.0, r.top() + 3.0), vec2(r.height() - 6.0, r.height() - 6.0));
                    if self.mouse.is_some_and(|p| ar.contains(p)) && hovered {
                        push(out, Shape::rect_filled(ar, CornerRadius::same(3), t.button_flat.hover.bg));
                    }
                    let s = Stroke::new(1.5, t.font_color);
                    let c = ar.center();
                    push(out, Shape::line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], s));
                    push(out, Shape::line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 5.0)], s));
                    rects.push(ar);
                }
                self.hit_rects.insert(id, rects);
            }
            Kind::Split { .. } => {
                let dragging = matches!(self.drag, Some(Drag::Split { id: d, .. }) if d == id);
                if let Some(d) = self.split_divider(c)
                    && (dragging || self.mouse.is_some_and(|p| d.contains(p)))
                {
                    push(out, Shape::rect_filled(d.shrink(2.5), CornerRadius::same(2), t.font_muted_color.gamma_multiply(0.5)));
                }
            }
            Kind::SpinBox { value, prefix, suffix, decimals, edit, .. } => {
                let focused = edit.is_some();
                let style = if focused { t.line_edit_focus } else if hovered { t.line_edit.with_bg(t.button.hover.bg) } else { t.line_edit };
                push(out, Shape::rect_filled(r, style.corner(), style.bg));
                if style.border.width > 0.0 {
                    push(out, Shape::rect_stroke(r, style.corner(), style.border, StrokeKind::Inside));
                }
                let inner = style.padding.shrink(r);
                let iclip = clip.intersect(inner.expand(1.0));
                let mut x = inner.left();
                if !prefix.is_empty() {
                    let pc = match prefix.as_str() {
                        "x" => Color32::from_rgb(0xf5, 0x51, 0x6b),
                        "y" => Color32::from_rgb(0x9a, 0xd8, 0x2a),
                        "z" => Color32::from_rgb(0x52, 0x9c, 0xf5),
                        _ => t.font_muted_color,
                    };
                    let g = self.text.layout(prefix, fs, pc);
                    out.push(ClippedShape::new(iclip, Shape::galley(pos2(x, r.center().y - g.size().y / 2.0), g.clone(), pc)));
                    x += g.size().x + 6.0;
                }
                match edit {
                    Some(e) => {
                        let g = self.text.layout(&e.text, fs, t.font_focus_color);
                        let y = r.center().y - g.size().y / 2.0;
                        if e.select_all && !e.text.is_empty() {
                            out.push(ClippedShape::new(iclip, Shape::rect_filled(Rect::from_min_size(pos2(x, y), g.size()), CornerRadius::same(2), t.accent.gamma_multiply(0.35))));
                        }
                        out.push(ClippedShape::new(iclip, Shape::galley(pos2(x, y), g.clone(), t.font_focus_color)));
                        if !e.select_all {
                            let byte = e.text.char_indices().nth(e.cursor).map_or(e.text.len(), |(b, _)| b);
                            let cx = x + self.text.layout(&e.text[..byte], fs, Color32::WHITE).size().x;
                            out.push(ClippedShape::new(iclip, Shape::line_segment([pos2(cx, y), pos2(cx, y + g.size().y)], Stroke::new(1.5, t.accent))));
                        }
                    }
                    None => {
                        let text = format!("{}{}", fmt_value(*value, *decimals), suffix);
                        let color = if c.disabled { t.font_disabled_color } else { t.font_color };
                        let g = self.text.layout(&text, fs, color);
                        out.push(ClippedShape::new(iclip, Shape::galley(pos2(x, r.center().y - g.size().y / 2.0), g, color)));
                    }
                }
            }
            Kind::ColorButton { color } => {
                let style = if hovered { t.button.hover } else { t.button.normal };
                push(out, Shape::rect_filled(r, style.corner(), style.bg));
                push(out, Shape::rect_stroke(r, style.corner(), style.border, StrokeKind::Inside));
                let inner = r.shrink(4.0);
                push(out, Shape::rect_filled(inner, CornerRadius::same(2), color.to_opaque()));
            }
            Kind::Image { image } => {
                let ir = Rect::from_center_size(r.center(), image.size.min(r.size()));
                push(out, Shape::image(image.texture, ir, image.uv, if c.disabled { image.tint.gamma_multiply(0.4) } else { image.tint }));
            }
            Kind::Canvas { shapes } => {
                let cclip = clip.intersect(r);
                for s in shapes {
                    out.push(ClippedShape::new(cclip, s.clone()));
                }
            }
            Kind::Tree { rows, selected, offset, edit } => {
                let tclip = clip.intersect(r);
                let max = (rows.len() as f32 * TREE_ROW - r.height()).max(0.0);
                let off = offset.clamp(0.0, max);
                let drop_target = match self.drag {
                    Some(Drag::Tree { id: d, key, active: true, .. }) if d == id => self.mouse.and_then(|p| self.tree_row_at(id, p)).filter(|&k| k != key),
                    _ => None,
                };
                let hover_row = if hovered { self.mouse.and_then(|p| self.tree_row_at(id, p)) } else { None };
                let first = (off / TREE_ROW).floor() as usize;
                let count = (r.height() / TREE_ROW).ceil() as usize + 1;
                for (i, row) in rows.iter().enumerate().skip(first).take(count) {
                    let rr = Rect::from_min_size(pos2(r.left(), r.top() + i as f32 * TREE_ROW - off), vec2(r.width(), TREE_ROW));
                    if *selected == Some(row.key) {
                        out.push(ClippedShape::new(tclip, Shape::rect_filled(rr.shrink2(vec2(2.0, 1.0)), t.item_selected.corner(), t.item_selected.bg)));
                        out.push(ClippedShape::new(tclip, Shape::rect_stroke(rr.shrink2(vec2(2.0, 1.0)), t.item_selected.corner(), t.item_selected.border, StrokeKind::Inside)));
                    } else if hover_row == Some(row.key) {
                        out.push(ClippedShape::new(tclip, Shape::rect_filled(rr.shrink2(vec2(2.0, 1.0)), CornerRadius::same(3), t.item_hover)));
                    }
                    if drop_target == Some(row.key) {
                        out.push(ClippedShape::new(tclip, Shape::rect_stroke(rr.shrink2(vec2(2.0, 1.0)), CornerRadius::same(3), Stroke::new(1.5, t.accent), StrokeKind::Inside)));
                    }
                    let mut x = r.left() + 4.0 + row.depth as f32 * TREE_INDENT;
                    if row.has_children {
                        let a = pos2(x + 6.0, rr.center().y);
                        let pts = if row.open {
                            vec![a + vec2(-4.0, -2.0), a + vec2(4.0, -2.0), a + vec2(0.0, 2.5)]
                        } else {
                            vec![a + vec2(-2.0, -4.0), a + vec2(2.5, 0.0), a + vec2(-2.0, 4.0)]
                        };
                        out.push(ClippedShape::new(tclip, Shape::convex_polygon(pts, t.font_muted_color, Stroke::NONE)));
                    }
                    x += 16.0;
                    if let Some(img) = row.icon {
                        out.push(ClippedShape::new(tclip, Shape::image(img.texture, Rect::from_center_size(pos2(x + 8.0, rr.center().y), Vec2::splat(16.0)), img.uv, img.tint)));
                        x += 22.0;
                    }
                    for (bi, (img, _)) in row.buttons.iter().enumerate() {
                        let bx = r.right() - 6.0 - (row.buttons.len() - bi) as f32 * TREE_BUTTON;
                        let br = Rect::from_center_size(pos2(bx + TREE_BUTTON / 2.0, rr.center().y), Vec2::splat(16.0));
                        out.push(ClippedShape::new(tclip, Shape::image(img.texture, br, img.uv, img.tint)));
                    }
                    let color = row.color.unwrap_or(if *selected == Some(row.key) { t.font_focus_color } else { t.font_color });
                    match edit {
                        Some((k, e)) if *k == row.key => {
                            let er = Rect::from_min_max(pos2(x - 4.0, rr.top() + 2.0), pos2(r.right() - 4.0, rr.bottom() - 2.0));
                            out.push(ClippedShape::new(tclip, Shape::rect_filled(er, CornerRadius::same(3), t.line_edit.bg)));
                            out.push(ClippedShape::new(tclip, Shape::rect_stroke(er, CornerRadius::same(3), t.line_edit_focus.border, StrokeKind::Inside)));
                            let g = self.text.layout(&e.text, fs, t.font_focus_color);
                            let y = rr.center().y - g.size().y / 2.0;
                            if e.select_all && !e.text.is_empty() {
                                out.push(ClippedShape::new(tclip, Shape::rect_filled(Rect::from_min_size(pos2(x, y), g.size()), CornerRadius::same(2), t.accent.gamma_multiply(0.35))));
                            }
                            out.push(ClippedShape::new(tclip, Shape::galley(pos2(x, y), g.clone(), t.font_focus_color)));
                            if !e.select_all {
                                let byte = e.text.char_indices().nth(e.cursor).map_or(e.text.len(), |(b, _)| b);
                                let cx = x + self.text.layout(&e.text[..byte], fs, Color32::WHITE).size().x;
                                out.push(ClippedShape::new(tclip, Shape::line_segment([pos2(cx, y), pos2(cx, y + g.size().y)], Stroke::new(1.5, t.accent))));
                            }
                        }
                        _ => {
                            let g = self.text.layout(&row.text, fs, color);
                            let w = g.size().x;
                            out.push(ClippedShape::new(tclip, Shape::galley(pos2(x, rr.center().y - g.size().y / 2.0), g, color)));
                            if !row.suffix.is_empty() {
                                let g = self.text.layout(&row.suffix, fs - 1.5, t.font_muted_color);
                                out.push(ClippedShape::new(tclip, Shape::galley(pos2(x + w + 8.0, rr.center().y - g.size().y / 2.0), g, t.font_muted_color)));
                            }
                        }
                    }
                }
                if max > 0.0 {
                    let h = (r.height() * r.height() / (max + r.height())).max(24.0);
                    let y = r.top() + (r.height() - h) * (off / max);
                    out.push(ClippedShape::new(tclip, Shape::rect_filled(Rect::from_min_size(pos2(r.right() - 5.0, y), vec2(3.0, h)), CornerRadius::same(2), t.font_color.gamma_multiply(0.35))));
                }
            }
            _ => {}
        }
    }
}
