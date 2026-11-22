//! Control nodes: the building blocks of every Neko interface, editor included.

use std::rc::Rc;

use neko_render::epaint::{Color32, Shape, TextureId};

use crate::geom::{Pos2, Rect, Sides, Vec2, vec2};
use crate::paint::Painter;
use crate::theme::{StateStyles, StyleBox};

/// Handle to a control in a [`crate::Ui`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub(crate) u32, pub(crate) u32);

impl Id {
    /// Pack into a u64 (for keys outside the UI, e.g. native window ids).
    pub fn to_bits(self) -> u64 {
        (self.1 as u64) << 32 | self.0 as u64
    }

    pub fn from_bits(bits: u64) -> Self {
        Id(bits as u32, (bits >> 32) as u32)
    }
}

/// How a control uses the space its container gives it (Godot's size flags).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeFlags {
    /// Take extra space in the container's direction (SIZE_EXPAND).
    pub expand: bool,
    /// Use all the space given (SIZE_FILL); otherwise keep the minimum size.
    pub fill: bool,
    /// Where a non-filling control sits: SIZE_SHRINK_BEGIN / CENTER / END.
    pub shrink: Align,
    /// Share of the stretch space relative to other expanding siblings.
    pub ratio: f32,
}

impl SizeFlags {
    pub const FILL: SizeFlags = SizeFlags { expand: false, fill: true, shrink: Align::Start, ratio: 1.0 };
    pub const EXPAND_FILL: SizeFlags = SizeFlags { expand: true, fill: true, shrink: Align::Start, ratio: 1.0 };
    pub const SHRINK_CENTER: SizeFlags = SizeFlags { expand: false, fill: false, shrink: Align::Center, ratio: 1.0 };
    pub const SHRINK_END: SizeFlags = SizeFlags { expand: false, fill: false, shrink: Align::End, ratio: 1.0 };
}

/// Horizontal alignment of text and content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Position relative to the parent rect, for controls outside containers
/// (Godot's anchors + offsets). Anchors go from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchors {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub offsets: Sides,
}

impl Anchors {
    pub const FULL_RECT: Anchors = Anchors { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0, offsets: Sides::ZERO };

    /// Centered on the parent at the control's minimum size.
    pub const CENTER: Anchors = Anchors { left: 0.5, top: 0.5, right: 0.5, bottom: 0.5, offsets: Sides::ZERO };

    pub const fn bottom_center(margin: f32) -> Anchors {
        Anchors { left: 0.5, top: 1.0, right: 0.5, bottom: 1.0, offsets: Sides::new(0.0, -margin, 0.0, -margin) }
    }
}

/// Custom drawing callback: `(painter, rect, hovered, pressed)`.
pub type DrawFn = Rc<dyn Fn(&mut Painter, Rect, bool, bool)>;

/// Does mouse input stop at this control or pass to the one below?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseFilter {
    Stop,
    Pass,
    Ignore,
}

#[derive(Clone)]
pub enum Kind {
    /// Covers its parent; children are placed by their anchors (full rect by default).
    Control,
    /// Draws a StyleBox behind children placed by anchors.
    Panel,
    /// A StyleBox whose padding wraps its children (Godot's PanelContainer).
    PanelContainer,
    HBox { separation: Option<f32> },
    VBox { separation: Option<f32> },
    Margin(Sides),
    Center,
    /// Vertical scrolling over a single child.
    Scroll { offset: f32, content: f32 },
    /// Covers the screen, dims what is behind and centers its child; blocks input below.
    Modal,
    Label { text: String, align: Align, ellipsis: bool, wrap: bool },
    Button { text: String, align: Align, toggle: bool, pressed: bool, flat: bool, primary: bool, icon: Option<DrawFn> },
    CheckBox { text: String, checked: bool, radio: bool, switch: bool },
    LineEdit { text: String, placeholder: String, cursor: usize, select_all: bool, monospace: bool },
    OptionButton { items: Vec<String>, selected: usize },
    Separator { vertical: bool },
    ColorRect { color: Color32, radius: u8 },
    Custom { draw: DrawFn },
    /// Godot's MenuBar: titles that open PopupMenus.
    MenuBar { menus: Vec<Menu> },
    /// Godot's TabBar.
    TabBar { tabs: Vec<Tab>, current: usize, closable: bool, add_button: bool },
    /// Godot's HSplitContainer / VSplitContainer: two children and a draggable divider.
    /// `offset` is the size of the first child along the split axis.
    Split { vertical: bool, offset: f32 },
    /// Godot's EditorSpinSlider: drag to change, click to type.
    SpinBox { value: f64, step: f64, min: f64, max: f64, prefix: String, suffix: String, decimals: usize, edit: Option<TextEdit> },
    /// Godot's ColorPickerButton (opening the picker is up to the app).
    ColorButton { color: Color32 },
    /// Godot's TextureRect.
    Image { image: Image },
    /// Shapes provided by the app every frame, plus raw mouse input (viewports).
    Canvas { shapes: Vec<Shape> },
    /// Godot's Tree, flattened into rows the app provides.
    Tree { rows: Vec<TreeRow>, selected: Option<u64>, offset: f32, edit: Option<(u64, TextEdit)> },
    /// A non-dimmed overlay that closes when clicking outside (Godot's PopupPanel).
    Popup { at: Pos2 },
    /// Godot's CodeEdit; the state is shared with the app's [`crate::CodeHandle`].
    CodeEdit(crate::code_edit::CodeHandle),
}

/// Text being edited inside a control.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdit {
    pub text: String,
    pub cursor: usize,
    pub select_all: bool,
}

impl TextEdit {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self { cursor: text.chars().count(), select_all: true, text }
    }
}

/// A region of a texture (icons come from an atlas).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Image {
    pub texture: TextureId,
    pub uv: Rect,
    pub tint: Color32,
    pub size: Vec2,
}

impl Image {
    pub fn new(texture: TextureId, uv: Rect, size: Vec2) -> Self {
        Self { texture, uv, tint: Color32::WHITE, size }
    }
    pub fn tinted(mut self, tint: Color32) -> Self {
        self.tint = tint;
        self
    }
}

/// An entry of a PopupMenu (Godot's add_item / add_check_item / add_submenu / add_separator).
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: String,
    /// Reported back in [`crate::UiEvent::Menu`].
    pub action: u32,
    pub enabled: bool,
    pub checked: Option<bool>,
    pub submenu: Vec<MenuItem>,
    pub separator: bool,
    pub icon: Option<Image>,
}

impl MenuItem {
    pub fn item(label: impl Into<String>, action: u32) -> Self {
        Self { label: label.into(), shortcut: String::new(), action, enabled: true, checked: None, submenu: Vec::new(), separator: false, icon: None }
    }
    pub fn separator() -> Self {
        Self { separator: true, ..Self::item("", 0) }
    }
    pub fn submenu(label: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self { submenu: items, ..Self::item(label, 0) }
    }
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = s.into();
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
    pub fn disabled(self) -> Self {
        self.enabled(false)
    }
    pub fn check(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }
    pub fn icon(mut self, i: Image) -> Self {
        self.icon = Some(i);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub title: String,
    pub icon: Option<Image>,
}

impl Tab {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), icon: None }
    }
}

/// One visible row of a Tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    pub key: u64,
    pub depth: u16,
    pub text: String,
    pub icon: Option<Image>,
    pub color: Option<Color32>,
    /// Muted text after the label (e.g. a node's class).
    pub suffix: String,
    pub has_children: bool,
    pub open: bool,
    pub draggable: bool,
    /// Icon buttons at the right end of the row (Godot's `TreeItem::add_button`),
    /// reported as [`crate::UiEvent::TreeButton`] with their index.
    pub buttons: Vec<(Image, String)>,
}

impl TreeRow {
    pub fn new(key: u64, depth: u16, text: impl Into<String>) -> Self {
        Self { key, depth, text: text.into(), icon: None, color: None, suffix: String::new(), has_children: false, open: true, draggable: false, buttons: Vec::new() }
    }
}

#[derive(Clone)]
pub struct Control {
    pub kind: Kind,
    pub min_size: Vec2,
    pub h: SizeFlags,
    pub v: SizeFlags,
    pub anchors: Option<Anchors>,
    pub visible: bool,
    pub disabled: bool,
    pub tooltip: Option<String>,
    /// Overrides the theme's StyleBox for panels and containers.
    pub style: Option<StyleBox>,
    /// Overrides the theme's button styles.
    pub button_style: Option<StateStyles>,
    pub font_size: Option<f32>,
    pub font_color: Option<Color32>,
    /// Bold text (Godot's `bold` editor font), for OptionButtons.
    pub bold: bool,
    pub mouse: MouseFilter,
    /// Containers and custom controls emit Pressed/DoubleClicked when clicked.
    pub clickable: bool,
    /// Free-form tag for the app (e.g. which project a row shows).
    pub tag: u64,
    pub(crate) parent: Option<Id>,
    pub(crate) children: Vec<Id>,
    pub(crate) rect: Rect,
}

impl Control {
    pub fn new(kind: Kind) -> Self {
        let interactive = matches!(
            kind,
            Kind::Button { .. }
                | Kind::CheckBox { .. }
                | Kind::LineEdit { .. }
                | Kind::OptionButton { .. }
                | Kind::Scroll { .. }
                | Kind::Modal
                | Kind::MenuBar { .. }
                | Kind::TabBar { .. }
                | Kind::SpinBox { .. }
                | Kind::ColorButton { .. }
                | Kind::Canvas { .. }
                | Kind::Tree { .. }
                | Kind::Popup { .. }
                | Kind::CodeEdit(_)
        );
        Self {
            kind,
            min_size: Vec2::ZERO,
            h: SizeFlags::FILL,
            v: SizeFlags::FILL,
            anchors: None,
            visible: true,
            disabled: false,
            tooltip: None,
            style: None,
            button_style: None,
            bold: false,
            font_size: None,
            font_color: None,
            mouse: if interactive { MouseFilter::Stop } else { MouseFilter::Pass },
            clickable: false,
            tag: 0,
            parent: None,
            children: Vec::new(),
            rect: Rect::NOTHING,
        }
    }

    // ------------------------------------------------------------ builders

    #[allow(clippy::self_named_constructors)] // Godot's base class is also called Control
    pub fn control() -> Self {
        Self::new(Kind::Control)
    }
    pub fn panel() -> Self {
        Self::new(Kind::Panel)
    }
    pub fn panel_container() -> Self {
        Self::new(Kind::PanelContainer)
    }
    pub fn hbox() -> Self {
        Self::new(Kind::HBox { separation: None })
    }
    pub fn vbox() -> Self {
        Self::new(Kind::VBox { separation: None })
    }
    pub fn margin(m: Sides) -> Self {
        Self::new(Kind::Margin(m))
    }
    pub fn center() -> Self {
        Self::new(Kind::Center)
    }
    pub fn scroll() -> Self {
        Self::new(Kind::Scroll { offset: 0.0, content: 0.0 }).expand()
    }
    pub fn modal() -> Self {
        let mut c = Self::new(Kind::Modal);
        c.anchors = Some(Anchors::FULL_RECT);
        c.visible = false;
        c
    }
    pub fn label(text: impl Into<String>) -> Self {
        Self::new(Kind::Label { text: text.into(), align: Align::Start, ellipsis: false, wrap: false })
    }
    pub fn button(text: impl Into<String>) -> Self {
        Self::new(Kind::Button { text: text.into(), align: Align::Center, toggle: false, pressed: false, flat: false, primary: false, icon: None })
    }
    pub fn check_box(text: impl Into<String>, checked: bool) -> Self {
        Self::new(Kind::CheckBox { text: text.into(), checked, radio: false, switch: false })
    }
    pub fn radio(text: impl Into<String>, checked: bool) -> Self {
        Self::new(Kind::CheckBox { text: text.into(), checked, radio: true, switch: false })
    }
    pub fn switch(checked: bool) -> Self {
        Self::new(Kind::CheckBox { text: String::new(), checked, radio: false, switch: true })
    }
    pub fn line_edit(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        Self::new(Kind::LineEdit { text, placeholder: String::new(), cursor, select_all: false, monospace: false }).expand_h()
    }
    pub fn option_button(items: &[&str], selected: usize) -> Self {
        Self::new(Kind::OptionButton { items: items.iter().map(|s| s.to_string()).collect(), selected })
    }
    pub fn h_separator() -> Self {
        Self::new(Kind::Separator { vertical: false })
    }
    pub fn v_separator() -> Self {
        Self::new(Kind::Separator { vertical: true })
    }
    pub fn color_rect(color: Color32) -> Self {
        Self::new(Kind::ColorRect { color, radius: 0 })
    }
    pub fn custom(size: Vec2, draw: impl Fn(&mut Painter, Rect, bool, bool) + 'static) -> Self {
        let mut c = Self::new(Kind::Custom { draw: Rc::new(draw) });
        c.min_size = size;
        c.h = SizeFlags::SHRINK_CENTER;
        c.v = SizeFlags::SHRINK_CENTER;
        c
    }
    pub fn menu_bar(menus: Vec<Menu>) -> Self {
        Self::new(Kind::MenuBar { menus })
    }
    pub fn tab_bar(tabs: Vec<Tab>, current: usize) -> Self {
        Self::new(Kind::TabBar { tabs, current, closable: false, add_button: false }).expand_h()
    }
    pub fn h_split(offset: f32) -> Self {
        Self::new(Kind::Split { vertical: false, offset }).expand()
    }
    pub fn v_split(offset: f32) -> Self {
        Self::new(Kind::Split { vertical: true, offset }).expand()
    }
    pub fn spin_box(value: f64, step: f64) -> Self {
        Self::new(Kind::SpinBox { value, step, min: f64::MIN, max: f64::MAX, prefix: String::new(), suffix: String::new(), decimals: 2, edit: None }).expand_h()
    }
    pub fn color_button(color: Color32) -> Self {
        Self::new(Kind::ColorButton { color }).min(48.0, 0.0)
    }
    pub fn image(image: Image) -> Self {
        let mut c = Self::new(Kind::Image { image });
        c.min_size = image.size;
        c.h = SizeFlags::SHRINK_CENTER;
        c.v = SizeFlags::SHRINK_CENTER;
        c
    }
    pub fn canvas() -> Self {
        Self::new(Kind::Canvas { shapes: Vec::new() }).expand()
    }
    pub fn tree() -> Self {
        Self::new(Kind::Tree { rows: Vec::new(), selected: None, offset: 0.0, edit: None }).expand()
    }
    pub fn popup() -> Self {
        let mut c = Self::new(Kind::Popup { at: Pos2::ZERO });
        c.anchors = Some(Anchors::FULL_RECT);
        c.visible = false;
        c
    }

    /// An empty control that takes the remaining space.
    pub fn spacer() -> Self {
        Self::control().expand()
    }

    // ------------------------------------------------------------ modifiers

    pub fn expand(mut self) -> Self {
        self.h = SizeFlags::EXPAND_FILL;
        self.v = SizeFlags { expand: true, ..self.v };
        self
    }
    pub fn expand_h(mut self) -> Self {
        self.h = SizeFlags::EXPAND_FILL;
        self
    }
    pub fn expand_v(mut self) -> Self {
        self.v = SizeFlags::EXPAND_FILL;
        self
    }
    pub fn shrink_center(mut self) -> Self {
        self.h = SizeFlags::SHRINK_CENTER;
        self.v = SizeFlags::SHRINK_CENTER;
        self
    }
    pub fn v_center(mut self) -> Self {
        self.v = SizeFlags::SHRINK_CENTER;
        self
    }
    pub fn h_center(mut self) -> Self {
        self.h = SizeFlags::SHRINK_CENTER;
        self
    }
    pub fn ratio(mut self, r: f32) -> Self {
        self.h.ratio = r;
        self.v.ratio = r;
        self
    }
    pub fn min(mut self, w: f32, h: f32) -> Self {
        self.min_size = vec2(w, h);
        self
    }
    pub fn anchors(mut self, a: Anchors) -> Self {
        self.anchors = Some(a);
        self
    }
    pub fn style(mut self, s: StyleBox) -> Self {
        self.style = Some(s);
        self
    }
    pub fn button_style(mut self, s: StateStyles) -> Self {
        self.button_style = Some(s);
        self
    }
    pub fn font(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }
    pub fn color(mut self, c: Color32) -> Self {
        self.font_color = Some(c);
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn tooltip(mut self, t: impl Into<String>) -> Self {
        self.tooltip = Some(t.into());
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }
    pub fn clickable(mut self) -> Self {
        self.clickable = true;
        self.mouse = MouseFilter::Stop;
        self
    }
    pub fn tag(mut self, t: u64) -> Self {
        self.tag = t;
        self
    }
    pub fn separation(mut self, s: f32) -> Self {
        match &mut self.kind {
            Kind::HBox { separation } | Kind::VBox { separation } => *separation = Some(s),
            _ => {}
        }
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        match &mut self.kind {
            Kind::Label { align, .. } | Kind::Button { align, .. } => *align = a,
            _ => {}
        }
        self
    }
    pub fn ellipsis(mut self) -> Self {
        if let Kind::Label { ellipsis, .. } = &mut self.kind {
            *ellipsis = true;
        }
        self.h = SizeFlags::EXPAND_FILL;
        self
    }
    pub fn wrap(mut self) -> Self {
        if let Kind::Label { wrap, .. } = &mut self.kind {
            *wrap = true;
        }
        self
    }
    pub fn flat(mut self) -> Self {
        if let Kind::Button { flat, .. } = &mut self.kind {
            *flat = true;
        }
        self
    }
    pub fn primary(mut self) -> Self {
        if let Kind::Button { primary, .. } = &mut self.kind {
            *primary = true;
        }
        self
    }
    pub fn toggle(mut self, on: bool) -> Self {
        if let Kind::Button { toggle, pressed, .. } = &mut self.kind {
            *toggle = true;
            *pressed = on;
        }
        self
    }
    pub fn icon(mut self, draw: impl Fn(&mut Painter, Rect, bool, bool) + 'static) -> Self {
        if let Kind::Button { icon, .. } = &mut self.kind {
            *icon = Some(Rc::new(draw));
        }
        self
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        if let Kind::LineEdit { placeholder, .. } = &mut self.kind {
            *placeholder = p.into();
        }
        self
    }
    pub fn closable(mut self) -> Self {
        if let Kind::TabBar { closable, .. } = &mut self.kind {
            *closable = true;
        }
        self
    }
    pub fn add_button(mut self) -> Self {
        if let Kind::TabBar { add_button, .. } = &mut self.kind {
            *add_button = true;
        }
        self
    }
    /// SpinBox range, decimals and prefix ("x", "y"...), like Godot's vector fields.
    pub fn range(mut self, lo: f64, hi: f64) -> Self {
        if let Kind::SpinBox { min, max, .. } = &mut self.kind {
            *min = lo;
            *max = hi;
        }
        self
    }
    pub fn decimals(mut self, d: usize) -> Self {
        if let Kind::SpinBox { decimals, .. } = &mut self.kind {
            *decimals = d;
        }
        self
    }
    pub fn prefix(mut self, p: impl Into<String>) -> Self {
        if let Kind::SpinBox { prefix, .. } = &mut self.kind {
            *prefix = p.into();
        }
        self
    }
    pub fn suffix(mut self, p: impl Into<String>) -> Self {
        if let Kind::SpinBox { suffix, .. } = &mut self.kind {
            *suffix = p.into();
        }
        self
    }
    pub fn monospace(mut self) -> Self {
        if let Kind::LineEdit { monospace, .. } = &mut self.kind {
            *monospace = true;
        }
        self
    }

    /// The rect from the last layout, in points.
    pub fn rect(&self) -> Rect {
        self.rect
    }
}
