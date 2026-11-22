//! Small builders shared by editor screens, matching Godot's dialogs
//! (AcceptDialog: title bar, content, centered OK/Cancel buttons).

use neko_ui::{Color32, Control, Id, Image, Rect, Sides, StyleBox, Ui, vec2};

use crate::icons::{self, Icon};
use crate::node_icons::Icons;

/// A modal dialog with a title bar. Returns (modal, body, close button).
pub fn dialog(ui: &mut Ui, title: &str, width: f32) -> (Id, Id, Id) {
    let t = ui.theme.clone();
    let root = ui.root();
    let modal = ui.add(root, Control::modal());
    ui.set_visible(modal, true);
    let panel = ui.add(modal, Control::panel_container().style(t.dialog).min(width, 0.0));
    let body = ui.add(panel, Control::vbox().separation(8.0));
    let bar = ui.add(body, Control::hbox());
    ui.add(bar, Control::spacer());
    ui.add(bar, Control::label(title).font(15.0).color(t.font_focus_color));
    ui.add(bar, Control::spacer());
    let close = ui.add(
        bar,
        Control::custom(vec2(22.0, 22.0), |p, r, hovered, _| {
            let c = if hovered { Color32::WHITE } else { Color32::from_gray(160) };
            icons::paint_on(p, r.shrink(4.0), Icon::Close, c);
        })
        .clickable()
        .tooltip("Close (Esc)"),
    );
    // A Godot Window: its own OS window unless Single Window Mode is on.
    ui.register_window(modal, title, bar, close);
    (modal, body, close)
}

/// Godot-style centered dialog buttons. Returns (ok, cancel).
pub fn ok_cancel(ui: &mut Ui, body: Id, ok_text: &str, cancel_text: &str) -> (Id, Id) {
    let row = ui.add(body, Control::hbox().separation(12.0));
    ui.add(row, Control::spacer());
    let cancel = ui.add(row, Control::button(cancel_text).min(96.0, 0.0));
    let ok = ui.add(row, Control::button(ok_text).primary().min(96.0, 0.0));
    ui.add(row, Control::spacer());
    (ok, cancel)
}

/// A flat icon-only button using one of Godot's icons; `toggle` makes it a toggle.
pub fn icon_button(icons: &Icons, godot_icon: &str, tooltip: &str, toggle: Option<bool>) -> Control {
    let img = icons.get(godot_icon);
    let mut c = Control::button("")
        .flat()
        .icon(move |p, r, hovered, on| draw_icon(p, img, r, hovered, on))
        .tooltip(tooltip)
        .button_style(icon_styles())
        .v_center();
    if let Some(on) = toggle {
        c = c.toggle(on);
    }
    c
}

/// A flat button with a Godot icon and a label.
pub fn icon_text_button(icons: &Icons, godot_icon: &str, text: &str) -> Control {
    let img = icons.get(godot_icon);
    Control::button(text).flat().icon(move |p, r, hovered, on| draw_icon(p, img, r, hovered, on)).v_center()
}

fn icon_styles() -> neko_ui::StateStyles {
    let t = neko_ui::Theme::neko_dark();
    let pad = Sides::all(5.0);
    neko_ui::StateStyles {
        normal: StyleBox::flat(Color32::TRANSPARENT, 4, pad),
        hover: t.button_flat.hover.with_padding(pad),
        pressed: StyleBox::flat(Color32::from_rgba_unmultiplied(0xf2, 0xb5, 0x44, 46), 4, pad),
        disabled: StyleBox::flat(Color32::TRANSPARENT, 4, pad),
    }
}

/// Icons are light gray; hovered gets brighter and toggled-on turns amber (Godot tints with the accent).
pub fn draw_icon(p: &mut neko_ui::Painter, img: Option<Image>, r: Rect, hovered: bool, on: bool) {
    let Some(img) = img else { return };
    let tint = if on {
        crate::theme::ACCENT
    } else if hovered {
        Color32::WHITE
    } else {
        Color32::from_gray(214)
    };
    p.image(img.tinted(tint), Rect::from_center_size(r.center(), vec2(16.0, 16.0)));
}

/// Class-colored icons keep their colors (Godot's node icons are already colored).
pub fn class_image(icons: &Icons, class: &str) -> Control {
    match icons.class(class) {
        Some(img) => Control::image(img),
        None => Control::control().min(16.0, 16.0),
    }
}
