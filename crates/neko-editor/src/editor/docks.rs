//! The Scene dock (Godot's SceneTreeDock). The FileSystem dock lives in
//! [`super::filesystem`].

use neko::core::NodeId;
use neko_ui::{MenuItem, TreeRow, UiEvent};

use super::menus::*;
use super::Editor;

/// Godot's per-node context menu (`SceneTreeDock::_tree_rmb`), in order.
fn node_menu(is_root: bool, can_undo_dup: bool, has_script: bool) -> Vec<MenuItem> {
    let todo = |l: &str, s: &str| MenuItem::item(l, 0).shortcut(s).disabled();
    vec![
        MenuItem::item("Add Child Node...", NODE_ADD).shortcut("Ctrl+A"),
        todo("Instantiate Child Scene...", "Ctrl+Shift+A"),
        todo("Expand/Collapse Branch", ""),
        MenuItem::separator(),
        todo("Cut", "Ctrl+X"),
        todo("Copy", "Ctrl+C"),
        todo("Paste (as Child)", "Ctrl+V"),
        todo("Paste as Sibling", "Ctrl+Shift+V"),
        MenuItem::separator(),
        MenuItem::item("Duplicate", NODE_DUPLICATE).shortcut("Ctrl+D").enabled(!is_root && can_undo_dup),
    ]
    .into_iter()
    .chain(if has_script {
        vec![todo("Extend Script...", ""), MenuItem::item("Detach Script", NODE_DETACH_SCRIPT)]
    } else {
        vec![MenuItem::item("Attach Script...", NODE_ATTACH_SCRIPT)]
    })
    .chain([
        MenuItem::separator(),
        MenuItem::item("Rename", NODE_RENAME).shortcut("F2"),
        todo("Change Type...", ""),
        MenuItem::separator(),
        MenuItem::item("Move Up", NODE_MOVE_UP).shortcut("Ctrl+Up").enabled(!is_root),
        MenuItem::item("Move Down", NODE_MOVE_DOWN).shortcut("Ctrl+Down").enabled(!is_root),
        todo("Reparent...", ""),
        todo("Reparent to New Node...", ""),
        todo("Make Scene Root", ""),
        MenuItem::separator(),
        todo("Save Branch as Scene...", ""),
        todo("Copy Node Path", "Ctrl+Shift+C"),
        MenuItem::separator(),
        MenuItem::item("Delete", NODE_DELETE).shortcut("Delete"),
    ])
    .collect()
}

impl Editor {
    pub(super) fn sync_scene_tree(&mut self) {
        let filter = self.ui.text(self.w.node_filter).trim().to_lowercase();
        let doc = &self.docs[self.current];
        let mut rows = Vec::new();
        if doc.has_root {
            let mut hidden_below: Option<usize> = None;
            for (id, depth) in doc.scene.walk() {
                if let Some(d) = hidden_below {
                    if depth > d && filter.is_empty() {
                        continue;
                    }
                    hidden_below = None;
                }
                let Some(n) = doc.scene.get(id) else { continue };
                let collapsed = doc.collapsed.contains(&id);
                if collapsed {
                    hidden_below = Some(depth);
                }
                if !filter.is_empty() && !n.name.to_lowercase().contains(&filter) && !n.class.to_lowercase().contains(&filter) {
                    continue;
                }
                let mut row = TreeRow::new(id.to_bits(), if filter.is_empty() { depth as u16 } else { 0 }, n.name.clone());
                row.icon = self.icons.class(n.class);
                row.has_children = filter.is_empty() && !n.children().is_empty();
                row.open = !collapsed;
                row.draggable = id != doc.scene.root();
                // Godot's "Open Script" button on nodes with a script.
                if let Some(path) = neko::core::tscn::script_path(&doc.scene, id)
                    && let Some(icon) = self.icons.get("Script")
                {
                    row.buttons.push((icon, format!("Open Script: {path}")));
                }
                rows.push(row);
            }
        }
        let selected = doc.selected.map(NodeId::to_bits);
        self.ui.set_rows(self.w.scene_tree, rows);
        self.ui.set_tree_selected(self.w.scene_tree, selected);
    }

    pub(super) fn tree_event(&mut self, ev: UiEvent) {
        let scene_tree = self.w.scene_tree;
        match ev {
            UiEvent::TreeSelected(t, key) if t == scene_tree => self.doc_mut().selected = Some(NodeId::from_bits(key)),
            UiEvent::TreeActivated(t, key) if t == scene_tree => self.ui.edit_tree_row(t, key),
            UiEvent::TreeButton(t, key, _) if t == scene_tree => {
                let id = NodeId::from_bits(key);
                self.doc_mut().selected = Some(id);
                if let Some(p) = self.node_script_file(id) {
                    self.open_script(p);
                }
            }
            UiEvent::TreeToggled(t, key, open) if t == scene_tree => {
                let id = NodeId::from_bits(key);
                let doc = self.doc_mut();
                if open {
                    doc.collapsed.remove(&id);
                } else {
                    doc.collapsed.insert(id);
                }
            }
            UiEvent::TreeEdited(t, key, text) if t == scene_tree => {
                let id = NodeId::from_bits(key);
                let doc = self.doc_mut();
                if doc.scene.get(id).is_some_and(|n| n.name != text.trim()) {
                    doc.checkpoint(None);
                    let _ = doc.scene.rename(id, &text);
                }
            }
            UiEvent::TreeDropped { tree, dragged, target } if tree == scene_tree => {
                let (dragged, target) = (NodeId::from_bits(dragged), NodeId::from_bits(target));
                let doc = self.doc_mut();
                let end = doc.scene.get(target).map_or(0, |n| n.children().len());
                let before = doc.scene.clone();
                doc.checkpoint(None);
                match doc.scene.move_node(dragged, target, end) {
                    Ok(()) => {
                        doc.collapsed.remove(&target);
                        doc.selected = Some(dragged);
                    }
                    Err(e) => {
                        doc.scene = before;
                        doc.undo.pop();
                        self.notify(e.to_string());
                    }
                }
            }
            UiEvent::TreeContext(t, key, pos) if t == scene_tree => {
                if key.is_some() {
                    let doc = self.doc();
                    let is_root = doc.selected == Some(doc.scene.root());
                    let has_script = self.selected_script().is_some();
                    self.ui.open_menu(node_menu(is_root, true, has_script), pos);
                } else if self.doc().has_root {
                    self.ui.open_menu(vec![MenuItem::item("Add Child Node...", NODE_ADD).shortcut("Ctrl+A")], pos);
                }
            }
            // The FileSystem dock owns its own tree and file list.
            UiEvent::TreeSelected(t, key) if t != scene_tree => self.fs_selected(t, key),
            UiEvent::TreeActivated(t, key) if t != scene_tree => self.fs_activated(t, key),
            UiEvent::TreeToggled(t, key, open) if t != scene_tree => self.fs_toggled(t, key, open),
            UiEvent::TreeEdited(t, key, text) if t != scene_tree => self.fs_edited(t, key, &text),
            UiEvent::TreeDropped { tree, dragged, target } if tree != scene_tree => self.fs_dropped(tree, dragged, target),
            UiEvent::TreeContext(t, key, pos) if t != scene_tree => self.fs_context(t, key, pos),
            _ => {}
        }
    }
}
