//! The scene tree.
//!
//! Nodes live in an arena and are addressed by [`NodeId`] (index plus
//! generation), so ids of removed nodes never alias new ones.

#[allow(unused_imports)]
use neko_base::prelude::*;

use alloc::collections::BTreeMap;

use crate::classdb::ClassDb;
use crate::godot::Value;
use crate::godot::text::Section;
use crate::variant::Variant;

/// What a node keeps from a Godot `.tscn` that Neko does not model, so
/// saving writes it back unchanged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GodotNode {
    /// `unique_id` (Godot 4.7); 0 until first saved.
    pub unique_id: i32,
    /// The Godot class when Neko has no class of that name (the node is
    /// loaded as its closest Neko base and written back with this type).
    pub class: Option<String>,
    /// The node had no `type` (an instanced sub-scene).
    pub untyped: bool,
    /// Header fields other than name/type/parent/unique_id (groups, instance...).
    pub header: Vec<(String, Value)>,
    /// Properties Neko does not map, verbatim.
    pub props: Vec<(String, Value)>,
    /// The node was loaded from a file (not created in Neko).
    pub from_file: bool,
}

/// Scene-level data kept from a Godot `.tscn`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GodotScene {
    /// `uid://...` of the scene file.
    pub uid: Option<String>,
    /// `[ext_resource]` and `[sub_resource]` sections, in file order.
    pub resources: Vec<Section>,
    /// `[connection]` and `[editable]` sections.
    pub tail: Vec<Section>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    /// Pack into a u64 (for UI keys).
    pub fn to_bits(self) -> u64 {
        (self.generation as u64) << 32 | self.index as u64
    }

    pub fn from_bits(bits: u64) -> Self {
        Self { index: bits as u32, generation: (bits >> 32) as u32 }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub class: &'static str,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    /// Only values that differ from the class default are stored.
    props: BTreeMap<String, Variant>,
    /// Godot data Neko keeps but does not edit.
    pub godot: GodotNode,
}

impl Node {
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Property value, falling back to the class default.
    pub fn get(&self, prop: &str) -> Option<Variant> {
        self.props
            .get(prop)
            .cloned()
            .or_else(|| ClassDb::get().property(self.class, prop).map(|p| p.default.clone()))
    }

    pub fn is_overridden(&self, prop: &str) -> bool {
        self.props.contains_key(prop)
    }

    /// Values that differ from the class defaults.
    pub fn overrides(&self) -> impl Iterator<Item = (&str, &Variant)> {
        self.props.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Set a property. Setting it back to the default removes the override.
    pub fn set(&mut self, prop: &str, value: Variant) -> Result<(), SceneError> {
        let def = ClassDb::get()
            .property(self.class, prop)
            .ok_or_else(|| SceneError::UnknownProperty { class: self.class.into(), prop: prop.into() })?;
        if !value.type_matches(def.ty) {
            return Err(SceneError::WrongType { prop: prop.into(), expected: def.ty });
        }
        if value == def.default {
            self.props.remove(prop);
        } else {
            self.props.insert(prop.to_string(), value);
        }
        Ok(())
    }

    pub fn reset(&mut self, prop: &str) {
        self.props.remove(prop);
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SceneError {
    #[error("unknown node type '{0}'")]
    UnknownClass(String),
    #[error("'{0}' is abstract and cannot be instanced")]
    AbstractClass(String),
    #[error("node does not exist")]
    NoSuchNode,
    #[error("the root node cannot be removed or moved")]
    Root,
    #[error("a node cannot become a child of itself or of its descendants")]
    Cycle,
    #[error("{class} has no property '{prop}'")]
    UnknownProperty { class: String, prop: String },
    #[error("property '{prop}' expects a {expected:?}")]
    WrongType { prop: String, expected: crate::variant::PropType },
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    node: Option<Node>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    slots: Vec<Slot>,
    free: Vec<u32>,
    root: NodeId,
    /// Godot data Neko keeps but does not edit.
    pub godot: GodotScene,
}

impl Scene {
    pub fn new(root_class: &str, root_name: &str) -> Result<Self, SceneError> {
        let mut s = Scene { slots: Vec::new(), free: Vec::new(), root: NodeId { index: 0, generation: 0 }, godot: GodotScene::default() };
        let class = Self::instanceable(root_class)?;
        s.root = s.alloc(Node {
            name: sanitize(root_name, class),
            class,
            parent: None,
            children: Vec::new(),
            props: BTreeMap::new(),
            godot: GodotNode::default(),
        });
        Ok(s)
    }

    fn instanceable(class: &str) -> Result<&'static str, SceneError> {
        let info = ClassDb::get().class(class).ok_or_else(|| SceneError::UnknownClass(class.into()))?;
        if info.is_abstract {
            return Err(SceneError::AbstractClass(class.into()));
        }
        Ok(info.name)
    }

    fn alloc(&mut self, node: Node) -> NodeId {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.node = Some(node);
            NodeId { index, generation: slot.generation }
        } else {
            self.slots.push(Slot { generation: 0, node: Some(node) });
            NodeId { index: self.slots.len() as u32 - 1, generation: 0 }
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.slots.get(id.index as usize).filter(|s| s.generation == id.generation).and_then(|s| s.node.as_ref())
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.slots.get_mut(id.index as usize).filter(|s| s.generation == id.generation).and_then(|s| s.node.as_mut())
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.node.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// A name not used by any child of `parent` (other than `except`).
    fn unique_name(&self, parent: NodeId, wanted: &str, except: Option<NodeId>) -> String {
        let taken = |n: &str| {
            self.get(parent).is_some_and(|p| {
                p.children.iter().any(|&c| Some(c) != except && self.get(c).is_some_and(|c| c.name == n))
            })
        };
        if !taken(wanted) {
            return wanted.to_string();
        }
        let base = wanted.trim_end_matches(|c: char| c.is_ascii_digit());
        let base = if base.is_empty() { wanted } else { base };
        (2..).map(|i| format!("{base}{i}")).find(|n| !taken(n)).expect("infinite names")
    }

    /// Add a new node as the last child of `parent`.
    /// `name` defaults to the class name; it is made unique among siblings.
    pub fn add_child(&mut self, parent: NodeId, class: &str, name: Option<&str>) -> Result<NodeId, SceneError> {
        let class = Self::instanceable(class)?;
        if !self.contains(parent) {
            return Err(SceneError::NoSuchNode);
        }
        let name = self.unique_name(parent, &sanitize(name.unwrap_or(class), class), None);
        let id = self.alloc(Node { name, class, parent: Some(parent), children: Vec::new(), props: BTreeMap::new(), godot: GodotNode::default() });
        self.get_mut(parent).expect("checked").children.push(id);
        Ok(id)
    }

    /// Remove a node and its whole subtree.
    pub fn remove(&mut self, id: NodeId) -> Result<(), SceneError> {
        if id == self.root {
            return Err(SceneError::Root);
        }
        let parent = self.get(id).ok_or(SceneError::NoSuchNode)?.parent;
        if let Some(p) = parent.and_then(|p| self.get_mut(p)) {
            p.children.retain(|&c| c != id);
        }
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let slot = &mut self.slots[n.index as usize];
            if let Some(node) = slot.node.take() {
                stack.extend(node.children);
                slot.generation = slot.generation.wrapping_add(1);
                self.free.push(n.index);
            }
        }
        Ok(())
    }

    /// Rename; the result is made unique among siblings. Returns the final name.
    pub fn rename(&mut self, id: NodeId, name: &str) -> Result<String, SceneError> {
        let node = self.get(id).ok_or(SceneError::NoSuchNode)?;
        let clean = sanitize(name, node.class);
        let final_name = match node.parent {
            Some(p) => self.unique_name(p, &clean, Some(id)),
            None => clean,
        };
        self.get_mut(id).expect("checked").name = final_name.clone();
        Ok(final_name)
    }

    pub fn is_ancestor(&self, ancestor: NodeId, mut node: NodeId) -> bool {
        while let Some(p) = self.get(node).and_then(|n| n.parent) {
            if p == ancestor {
                return true;
            }
            node = p;
        }
        false
    }

    /// Move `id` under `new_parent` at `index` (clamped). Works for reordering too.
    pub fn move_node(&mut self, id: NodeId, new_parent: NodeId, index: usize) -> Result<(), SceneError> {
        if id == self.root {
            return Err(SceneError::Root);
        }
        if id == new_parent || self.is_ancestor(id, new_parent) {
            return Err(SceneError::Cycle);
        }
        let old_parent = self.get(id).ok_or(SceneError::NoSuchNode)?.parent.ok_or(SceneError::Root)?;
        if !self.contains(new_parent) {
            return Err(SceneError::NoSuchNode);
        }
        let old = self.get_mut(old_parent).expect("parent exists");
        let old_index = old.children.iter().position(|&c| c == id).expect("child listed in parent");
        old.children.remove(old_index);
        let mut index = index;
        if old_parent == new_parent && old_index < index {
            index -= 1;
        }
        let name = self.get(id).expect("exists").name.clone();
        let unique = self.unique_name(new_parent, &name, Some(id));
        let np = self.get_mut(new_parent).expect("checked");
        let index = index.min(np.children.len());
        np.children.insert(index, id);
        let node = self.get_mut(id).expect("exists");
        node.parent = Some(new_parent);
        node.name = unique;
        Ok(())
    }

    /// Deep-copy a node next to the original. Returns the copy.
    pub fn duplicate(&mut self, id: NodeId) -> Result<NodeId, SceneError> {
        let parent = self.get(id).ok_or(SceneError::NoSuchNode)?.parent.ok_or(SceneError::Root)?;
        let copy = self.copy_subtree(id, parent);
        let siblings = &mut self.get_mut(parent).expect("exists").children;
        siblings.retain(|&c| c != copy);
        let at = siblings.iter().position(|&c| c == id).map_or(siblings.len(), |i| i + 1);
        siblings.insert(at, copy);
        Ok(copy)
    }

    fn copy_subtree(&mut self, src: NodeId, parent: NodeId) -> NodeId {
        let node = self.get(src).expect("exists").clone();
        let name = self.unique_name(parent, &node.name, None);
        let id = self.alloc(Node { name, class: node.class, parent: Some(parent), children: Vec::new(), props: node.props, godot: GodotNode { unique_id: 0, ..node.godot } });
        self.get_mut(parent).expect("exists").children.push(id);
        for child in node.children {
            self.copy_subtree(child, id);
        }
        id
    }

    /// Copy every node of `other` under `parent` (its root becomes a child of
    /// `parent`). Returns `(id in other, id in self)` for each copied node,
    /// parents first.
    pub fn graft(&mut self, parent: NodeId, other: &Scene) -> Result<Vec<(NodeId, NodeId)>, SceneError> {
        if !self.contains(parent) {
            return Err(SceneError::NoSuchNode);
        }
        let mut map: Vec<(NodeId, NodeId)> = Vec::new();
        for (src, _) in other.walk() {
            let node = other.get(src).expect("walk yields live nodes");
            let new_parent = match node.parent {
                None => parent,
                Some(p) => map.iter().find(|(o, _)| *o == p).map(|(_, n)| *n).ok_or(SceneError::NoSuchNode)?,
            };
            let name = self.unique_name(new_parent, &node.name, None);
            let id = self.alloc(Node { name, class: node.class, parent: Some(new_parent), children: Vec::new(), props: node.props.clone(), godot: node.godot.clone() });
            self.get_mut(new_parent).expect("exists").children.push(id);
            map.push((src, id));
        }
        Ok(map)
    }

    /// Path relative to the root: `"."` for the root, `"Player/Sprite2D"` otherwise.
    pub fn path(&self, id: NodeId) -> String {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != self.root {
            let Some(n) = self.get(cur) else { break };
            parts.push(n.name.as_str());
            match n.parent {
                Some(p) => cur = p,
                None => break,
            }
        }
        if parts.is_empty() {
            return ".".into();
        }
        parts.reverse();
        parts.join("/")
    }

    pub fn find(&self, path: &str) -> Option<NodeId> {
        let mut cur = self.root;
        if path == "." || path.is_empty() {
            return Some(cur);
        }
        for part in path.split('/') {
            let node = self.get(cur)?;
            cur = *node.children.iter().find(|&&c| self.get(c).is_some_and(|n| n.name == part))?;
        }
        Some(cur)
    }

    /// Detach `id` and its subtree into a new scene, keeping every node and
    /// property. Returns the new scene and an old -> new id map.
    pub fn extract(&mut self, id: NodeId) -> Result<(Scene, Vec<(NodeId, NodeId)>), SceneError> {
        let n = self.get(id).ok_or(SceneError::NoSuchNode)?;
        let mut out = Scene::new(n.class, &n.name)?;
        let out_root = out.root();
        {
            let dst = out.get_mut(out_root).expect("root");
            dst.props = n.props.clone();
            dst.godot = n.godot.clone();
        }
        let mut map = vec![(id, out_root)];
        let subtree: Vec<(NodeId, usize)> = {
            let mut v = Vec::new();
            let mut stack = vec![(id, 0usize)];
            while let Some((cur, depth)) = stack.pop() {
                v.push((cur, depth));
                if let Some(n) = self.get(cur) {
                    for &c in n.children().iter().rev() {
                        stack.push((c, depth + 1));
                    }
                }
            }
            v
        };
        for (old, _) in subtree.into_iter().skip(1) {
            let src = self.get(old).ok_or(SceneError::NoSuchNode)?;
            let parent_old = src.parent().ok_or(SceneError::Root)?;
            let parent_new = map.iter().find(|(o, _)| *o == parent_old).map(|(_, n)| *n).ok_or(SceneError::NoSuchNode)?;
            let dst = out.add_child(parent_new, src.class, Some(&src.name))?;
            let (props, godot) = (src.props.clone(), src.godot.clone());
            let d = out.get_mut(dst).expect("just added");
            d.props = props;
            d.godot = godot;
            map.push((old, dst));
        }
        self.remove(id)?;
        Ok((out, map))
    }

    /// Move a node up (-1) or down (+1) among its siblings.
    pub fn move_sibling(&mut self, id: NodeId, delta: i32) -> Result<(), SceneError> {
        let parent = self.get(id).ok_or(SceneError::NoSuchNode)?.parent.ok_or(SceneError::Root)?;
        let siblings = &self.get(parent).expect("parent exists").children;
        let i = siblings.iter().position(|&c| c == id).expect("listed") as i32;
        let j = (i + delta).clamp(0, siblings.len() as i32 - 1);
        if i != j {
            let p = self.get_mut(parent).expect("parent exists");
            let n = p.children.remove(i as usize);
            p.children.insert(j as usize, n);
        }
        Ok(())
    }

    /// Depth-first order with depth, root first.
    pub fn walk(&self) -> Vec<(NodeId, usize)> {
        let mut out = Vec::new();
        let mut stack = vec![(self.root, 0)];
        while let Some((id, depth)) = stack.pop() {
            out.push((id, depth));
            if let Some(n) = self.get(id) {
                stack.extend(n.children.iter().rev().map(|&c| (c, depth + 1)));
            }
        }
        out
    }

    /// Sum of 3D positions from the root down to `id` (rotation and scale ignored for now).
    pub fn global_position_3d(&self, id: NodeId) -> [f32; 3] {
        let mut pos = [0.0f32; 3];
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(n) = self.get(c) else { break };
            if ClassDb::get().inherits(n.class, "Node3D")
                && let Some(Variant::Vec3(v)) = n.get("position")
            {
                for i in 0..3 {
                    pos[i] += v[i];
                }
            }
            cur = n.parent;
        }
        pos
    }

    /// Sum of 2D positions from the root down to `id` (rotation and scale ignored for now).
    pub fn global_position_2d(&self, id: NodeId) -> [f32; 2] {
        let mut pos = [0.0f32; 2];
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(n) = self.get(c) else { break };
            if (ClassDb::get().inherits(n.class, "Node2D") || ClassDb::get().inherits(n.class, "Control"))
                && let Some([x, y]) = n.get("position").and_then(|v| v.as_vec2()) {
                    pos[0] += x;
                    pos[1] += y;
                }
            cur = n.parent;
        }
        pos
    }
}

/// Node names cannot be empty or contain path separators.
fn sanitize(name: &str, fallback: &str) -> String {
    let s: String = name.trim().chars().filter(|c| !matches!(c, '/' | ':' | '.' | '"' | '%' | '@')).collect();
    if s.is_empty() { fallback.to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Scene, NodeId, NodeId) {
        let mut s = Scene::new("Node2D", "Main").unwrap();
        let player = s.add_child(s.root(), "CharacterBody2D", Some("Player")).unwrap();
        let sprite = s.add_child(player, "Sprite2D", None).unwrap();
        (s, player, sprite)
    }

    #[test]
    fn add_names_and_paths() {
        let (mut s, player, sprite) = sample();
        assert_eq!(s.path(sprite), "Player/Sprite2D");
        assert_eq!(s.find("Player/Sprite2D"), Some(sprite));
        let second = s.add_child(player, "Sprite2D", None).unwrap();
        assert_eq!(s.get(second).unwrap().name, "Sprite2D2");
        assert_eq!(s.add_child(player, "PhysicsBody2D", None), Err(SceneError::AbstractClass("PhysicsBody2D".into())));
        assert_eq!(s.rename(second, "Sprite2D").unwrap(), "Sprite2D2");
        assert_eq!(s.rename(second, "Bad/Name").unwrap(), "BadName");
    }

    #[test]
    fn remove_subtree_invalidates_ids() {
        let (mut s, player, sprite) = sample();
        assert_eq!(s.remove(s.root()), Err(SceneError::Root));
        s.remove(player).unwrap();
        assert!(!s.contains(player) && !s.contains(sprite));
        assert_eq!(s.len(), 1);
        let reused = s.add_child(s.root(), "Node", None).unwrap();
        assert!(!s.contains(player) && s.contains(reused));
    }

    #[test]
    fn move_and_duplicate() {
        let (mut s, player, sprite) = sample();
        assert_eq!(s.move_node(player, sprite, 0), Err(SceneError::Cycle));
        s.move_node(sprite, s.root(), 0).unwrap();
        assert_eq!(s.get(s.root()).unwrap().children()[0], sprite);
        let copy = s.duplicate(player).unwrap();
        assert_eq!(s.get(copy).unwrap().name, "Player2");
        assert_eq!(s.get(s.root()).unwrap().children(), &[sprite, player, copy]);
    }

    #[test]
    fn properties_store_only_overrides() {
        let (mut s, player, sprite) = sample();
        let n = s.get_mut(sprite).unwrap();
        assert_eq!(n.get("scale"), Some(Variant::Vec2([1.0, 1.0])));
        n.set("position", Variant::Vec2([4.0, 2.0])).unwrap();
        assert!(n.is_overridden("position"));
        n.set("position", Variant::Vec2([0.0, 0.0])).unwrap();
        assert!(!n.is_overridden("position"));
        assert!(n.set("position", Variant::Bool(true)).is_err());
        assert!(n.set("nope", Variant::Bool(true)).is_err());
        s.get_mut(player).unwrap().set("position", Variant::Vec2([10.0, 0.0])).unwrap();
        s.get_mut(sprite).unwrap().set("position", Variant::Vec2([1.0, 2.0])).unwrap();
        assert_eq!(s.global_position_2d(sprite), [11.0, 2.0]);
    }
}
