//! The "Create New Node" dialog, laid out like Godot's `CreateDialog`
//! (`editor/gui/create_dialog.cpp`): Favorites / Recent on the left; Search,
//! Matches and Description on the right; Create / Cancel at the bottom.

use std::collections::{HashMap, HashSet};

use neko::core::{ClassDb, ClassInfo};
use neko::platform::Key;
use neko_ui::{Color32, Control, Id, Sides, StyleBox, TreeRow, Ui, UiEvent};

use crate::node_icons::Icons;
use crate::ui_kit;

pub struct CreateNodeDialog {
    modal: Id,
    close: Id,
    search: Id,
    favorite: Id,
    matches: Id,
    favorites_tree: Id,
    recent_tree: Id,
    description: Id,
    create: Id,
    cancel: Id,
    collapsed: HashSet<&'static str>,
    picked: Option<&'static str>,
}

pub enum Outcome {
    Open,
    Cancel,
    Create(&'static str),
}

fn key_of(name: &str) -> u64 {
    ClassDb::get().all().iter().position(|c| c.name == name).map_or(u64::MAX, |i| i as u64)
}

fn class_of(key: u64) -> Option<&'static ClassInfo> {
    ClassDb::get().all().get(key as usize)
}

impl CreateNodeDialog {
    pub fn open(ui: &mut Ui, icons: &Icons, parent_class: &str, favorites: &[String], recents: &[String]) -> Self {
        let t = ui.theme.clone();
        let (modal, body, close) = ui_kit::dialog(ui, "Create New Node", 900.0);
        let split = ui.add(body, Control::h_split(200.0).min(0.0, 480.0));
        let left = ui.add(split, Control::vbox().min(150.0, 0.0));
        ui.add(left, Control::label("Favorites:"));
        let fav_bg = ui.add(left, Control::panel_container().style(t.tree_panel).expand());
        let favorites_tree = ui.add(fav_bg, Control::tree());
        ui.add(left, Control::label("Recent:"));
        let rec_bg = ui.add(left, Control::panel_container().style(t.tree_panel).expand());
        let recent_tree = ui.add(rec_bg, Control::tree());

        let right = ui.add(split, Control::vbox().min(300.0, 0.0));
        ui.add(right, Control::label("Search:"));
        let search_row = ui.add(right, Control::hbox());
        let search = ui.add(search_row, Control::line_edit("").placeholder("Search..."));
        let favorite = ui.add(search_row, ui_kit::icon_button(icons, "Favorites", "(Un)favorite selected item.", Some(false)));
        let mrow = ui.add(right, Control::hbox());
        ui.add(mrow, Control::label("Matches:").v_center());
        ui.add(mrow, Control::spacer());
        ui.add(mrow, Control::button("Filters").flat().disabled(true).tooltip("Not available yet"));
        let m_bg = ui.add(right, Control::panel_container().style(t.tree_panel).expand().ratio(3.0));
        let matches = ui.add(m_bg, Control::tree());
        ui.add(right, Control::label("Description:"));
        let d_bg = ui.add(right, Control::panel_container().style(StyleBox::flat(t.tree_panel.bg, t.corner_radius, Sides::all(8.0))).min(0.0, 70.0));
        let description = ui.add(d_bg, Control::label("").wrap().color(t.font_color));
        let (create, cancel) = ui_kit::ok_cancel(ui, body, "Create", "Cancel");
        ui.focus(search);

        // Start with the branch matching the parent open, like Godot preselects a sensible type.
        let db = ClassDb::get();
        let mut collapsed: HashSet<&'static str> = ["Node3D", "Control", "Container", "Light3D", "CollisionObject3D", "Path2D", "Node2D"].into();
        let preferred = if db.inherits(parent_class, "Node3D") {
            collapsed.remove("Node3D");
            "Node3D"
        } else if db.inherits(parent_class, "Control") {
            collapsed.remove("Control");
            "Control"
        } else {
            collapsed.remove("Node2D");
            "Node2D"
        };
        let mut d = Self {
            modal,
            close,
            search,
            favorite,
            matches,
            favorites_tree,
            recent_tree,
            description,
            create,
            cancel,
            collapsed,
            picked: db.class(preferred).map(|c| c.name),
        };
        d.refresh(ui, icons, favorites, recents);
        d
    }

    pub fn close(self, ui: &mut Ui) {
        ui.remove(self.modal);
    }

    fn query(&self, ui: &Ui) -> String {
        ui.text(self.search).trim().to_lowercase()
    }

    fn matches_query(c: &ClassInfo, q: &str) -> bool {
        q.is_empty() || c.name.to_lowercase().contains(q)
    }

    /// Rebuild the trees and description from the current search and pick.
    pub fn refresh(&mut self, ui: &mut Ui, icons: &Icons, favorites: &[String], recents: &[String]) {
        let db = ClassDb::get();
        let q = self.query(ui);
        let muted = ui.theme.font_disabled_color;
        let mut visible: HashSet<&'static str> = HashSet::new();
        for c in db.all() {
            if Self::matches_query(c, &q) {
                visible.extend(db.ancestry(c.name));
            }
        }
        let searching = !q.is_empty();
        let mut rows = Vec::new();
        let mut stack = vec![(db.root().name, 0u16)];
        while let Some((name, depth)) = stack.pop() {
            if !visible.contains(name) {
                continue;
            }
            let c = db.class(name).expect("known class");
            let kids: Vec<_> = db.children(name).iter().copied().filter(|k| visible.contains(k)).collect();
            let open = searching || !self.collapsed.contains(name);
            let mut row = TreeRow::new(key_of(name), depth, name);
            row.icon = icons.class(name);
            row.has_children = !kids.is_empty();
            row.open = open;
            if c.is_abstract || !Self::matches_query(c, &q) {
                row.color = Some(muted);
            }
            rows.push(row);
            if open {
                for k in kids.into_iter().rev() {
                    stack.push((k, depth + 1));
                }
            }
        }
        // When searching, pick the best match (exact, then prefix), like Godot.
        if searching && self.picked.is_none_or(|p| !Self::matches_query(db.class(p).expect("known"), &q)) {
            let best = db
                .all()
                .iter()
                .filter(|c| !c.is_abstract && Self::matches_query(c, &q))
                .min_by_key(|c| {
                    let n = c.name.to_lowercase();
                    (if n == q { 0 } else if n.starts_with(&q) { 1 } else { 2 }, c.name.len())
                })
                .map(|c| c.name);
            self.picked = best;
        }
        ui.set_rows(self.matches, rows);
        ui.set_tree_selected(self.matches, self.picked.map(key_of));

        let list_rows = |names: &[String]| -> Vec<TreeRow> {
            names
                .iter()
                .filter_map(|n| db.class(n))
                .map(|c| {
                    let mut r = TreeRow::new(key_of(c.name), 0, c.name);
                    r.icon = icons.class(c.name);
                    r
                })
                .collect()
        };
        ui.set_rows(self.favorites_tree, list_rows(favorites));
        ui.set_rows(self.recent_tree, list_rows(recents));

        let picked = self.picked.and_then(|p| db.class(p));
        let desc = match picked {
            Some(c) => {
                let mut s = c.description.to_string();
                if c.is_abstract {
                    s += "\n(Abstract class, cannot be instantiated.)";
                }
                s
            }
            None => String::new(),
        };
        ui.set_text(self.description, desc);
        ui.set_disabled(self.create, picked.is_none_or(|c| c.is_abstract));
        ui.set_checked(self.favorite, picked.is_some_and(|c| favorites.iter().any(|f| f == c.name)));
        ui.set_disabled(self.favorite, picked.is_none());
    }

    /// Handle an event; returns what the editor should do.
    pub fn handle(&mut self, ui: &mut Ui, icons: &Icons, ev: &UiEvent, favorites: &mut Vec<String>, recents: &[String]) -> Outcome {
        let db = ClassDb::get();
        let can_create = || self.picked.and_then(|p| db.class(p)).is_some_and(|c| !c.is_abstract);
        match ev {
            UiEvent::Pressed(id) if *id == self.close || *id == self.cancel => return Outcome::Cancel,
            UiEvent::Key { key: Key::Escape, .. } => return Outcome::Cancel,
            UiEvent::Pressed(id) if *id == self.create => {
                if can_create() {
                    return Outcome::Create(self.picked.expect("checked"));
                }
            }
            UiEvent::TextSubmitted(id) if *id == self.search => {
                if can_create() {
                    return Outcome::Create(self.picked.expect("checked"));
                }
            }
            UiEvent::TextChanged(id) if *id == self.search => self.refresh(ui, icons, favorites, recents),
            UiEvent::TreeSelected(_, key) => {
                self.picked = class_of(*key).map(|c| c.name);
                self.refresh(ui, icons, favorites, recents);
            }
            UiEvent::TreeActivated(_, key) => {
                if let Some(c) = class_of(*key)
                    && !c.is_abstract
                {
                    return Outcome::Create(c.name);
                }
            }
            UiEvent::TreeToggled(_, key, open) => {
                if let Some(c) = class_of(*key) {
                    if *open {
                        self.collapsed.remove(c.name);
                    } else {
                        self.collapsed.insert(c.name);
                    }
                    self.refresh(ui, icons, favorites, recents);
                }
            }
            UiEvent::Toggled(id, on) if *id == self.favorite => {
                if let Some(p) = self.picked {
                    favorites.retain(|f| f != p);
                    if *on {
                        favorites.push(p.to_string());
                    }
                    self.refresh(ui, icons, favorites, recents);
                }
            }
            UiEvent::Key { key: Key::ArrowDown | Key::ArrowUp, .. } => {
                // Move through the visible matches while typing, like Godot's search box.
                let order: Vec<&'static str> = visible_order(&self.collapsed, &self.query(ui));
                if let Some(i) = self.picked.and_then(|p| order.iter().position(|n| *n == p)) {
                    let j = if matches!(ev, UiEvent::Key { key: Key::ArrowDown, .. }) { (i + 1).min(order.len() - 1) } else { i.saturating_sub(1) };
                    self.picked = Some(order[j]);
                    self.refresh(ui, icons, favorites, recents);
                }
            }
            _ => {}
        }
        Outcome::Open
    }
}

fn visible_order(collapsed: &HashSet<&'static str>, q: &str) -> Vec<&'static str> {
    let db = ClassDb::get();
    let mut visible: HashSet<&'static str> = HashSet::new();
    for c in db.all() {
        if CreateNodeDialog::matches_query(c, q) {
            visible.extend(db.ancestry(c.name));
        }
    }
    let mut out = Vec::new();
    let mut stack = vec![db.root().name];
    while let Some(name) = stack.pop() {
        if !visible.contains(name) {
            continue;
        }
        out.push(name);
        if !q.is_empty() || !collapsed.contains(name) {
            for &k in db.children(name).iter().rev() {
                stack.push(k);
            }
        }
    }
    out
}

#[allow(dead_code)]
fn _unused(_: HashMap<u8, u8>, _: Color32) {}
