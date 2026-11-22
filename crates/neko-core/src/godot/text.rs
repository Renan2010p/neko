//! `.tscn` / `.tres` documents (scene/resources/resource_format_text.cpp),
//! as an ordered list of sections:
//!
//! ```text
//! [gd_scene format=3 uid="uid://b6x..."]
//!
//! [ext_resource type="Texture2D" uid="uid://..." path="res://cat.png" id="1_cat"]
//!
//! [sub_resource type="RectangleShape2D" id="RectangleShape2D_abcde"]
//! size = Vector2(48, 56)
//!
//! [node name="Main" type="Node2D"]
//!
//! [node name="Player" type="CharacterBody2D" parent="."]
//! position = Vector2(300, 420)
//! ```

#[allow(unused_imports)]
use neko_base::prelude::*;

use super::value::{Value, parse_tag_fields};

/// One `[tag field=value ...]` header and the `name = value` lines under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub tag: String,
    pub fields: Vec<(String, Value)>,
    pub props: Vec<(String, Value)>,
}

impl Section {
    pub fn new(tag: &str) -> Self {
        Self { tag: tag.into(), fields: Vec::new(), props: Vec::new() }
    }

    pub fn field(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn field_str(&self, key: &str) -> Option<&str> {
        self.field(key).and_then(Value::as_str)
    }

    pub fn with(mut self, key: &str, v: Value) -> Self {
        self.fields.push((key.into(), v));
        self
    }

    pub fn prop(&self, key: &str) -> Option<&Value> {
        self.props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// A whole text resource or scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub sections: Vec<Section>,
}

impl Document {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut sections: Vec<Section> = Vec::new();
        let mut rest = text;
        let mut line_no = 1usize;
        loop {
            // Skip whitespace and `;` comments.
            let trimmed = rest.trim_start();
            line_no += rest[..rest.len() - trimmed.len()].matches('\n').count();
            rest = trimmed;
            if rest.is_empty() {
                break;
            }
            if rest.starts_with(';') {
                rest = rest.find('\n').map_or("", |i| &rest[i..]);
                continue;
            }
            if let Some(after) = rest.strip_prefix('[') {
                // Tag name, then fields up to the closing bracket at depth 0.
                let name_end = after.find(|c: char| c.is_whitespace() || c == ']').ok_or("unterminated section tag")?;
                let tag = after[..name_end].to_string();
                let body = &after[name_end..];
                let close = tag_end(body).ok_or_else(|| format!("line {line_no}: unterminated [{tag}] tag"))?;
                let fields = parse_tag_fields(&body[..close]).map_err(|e| format!("line {line_no}: {e}"))?;
                sections.push(Section { tag, fields, props: Vec::new() });
                let consumed = &rest[..1 + name_end + close + 1];
                line_no += consumed.matches('\n').count();
                rest = &rest[consumed.len()..];
                continue;
            }
            // `name = value`, possibly spanning lines.
            let eq = rest.find('=').ok_or_else(|| format!("line {line_no}: expected 'name = value'"))?;
            let key = rest[..eq].trim();
            let key = key.strip_prefix('"').and_then(|k| k.strip_suffix('"')).unwrap_or(key).to_string();
            let (value, after) = Value::parse_prefix(&rest[eq + 1..]).map_err(|e| format!("line {line_no}: {key}: {e}"))?;
            let consumed = rest.len() - after.len();
            line_no += rest[..consumed].matches('\n').count();
            rest = after;
            match sections.last_mut() {
                Some(s) => s.props.push((key, value)),
                None => return Err(format!("line {line_no}: property before any section")),
            }
        }
        Ok(Self { sections })
    }

    /// Write with Godot's layout: a blank line between sections, except
    /// between consecutive `ext_resource`, `connection` or `editable` lines.
    pub fn write(&self) -> String {
        let mut out = String::new();
        let mut prev: Option<&str> = None;
        for s in &self.sections {
            if let Some(p) = prev {
                let grouped = p == s.tag && matches!(s.tag.as_str(), "ext_resource" | "connection" | "editable");
                if !grouped {
                    out.push('\n');
                }
            }
            out.push('[');
            out.push_str(&s.tag);
            for (k, v) in &s.fields {
                out.push(' ');
                out.push_str(k);
                out.push('=');
                v.write(&mut out);
            }
            out.push_str("]\n");
            for (k, v) in &s.props {
                out.push_str(&property_name_encode(k));
                out.push_str(" = ");
                v.write(&mut out);
                out.push('\n');
            }
            prev = Some(&s.tag);
        }
        out
    }
}

/// Godot quotes property names that are not plain identifiers or paths.
fn property_name_encode(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '/' | ':' | '.' | '-')) {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Index of the `]` closing a tag body, skipping brackets inside values and strings.
fn tag_end(body: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        if in_str {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '[' | '(' | '{' => depth += 1,
            ')' | '}' => depth -= 1,
            ']' if depth == 0 => return Some(i),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene as Godot 4.7 writes it.
    const GODOT: &str = r#"[gd_scene format=3 uid="uid://cd3k1x0c5oqfb"]

[ext_resource type="Texture2D" uid="uid://b5m4hq1x3d7e2" path="res://icon.svg" id="1_abcde"]
[ext_resource type="Script" path="res://player.gd" id="2_fghij"]

[sub_resource type="RectangleShape2D" id="RectangleShape2D_klmno"]
size = Vector2(48, 56)

[node name="Main" type="Node2D" unique_id=1820301468]

[node name="Player" type="CharacterBody2D" parent="." unique_id=95512398 groups=["player"]]
position = Vector2(300, 420)
rotation = 0.785398
script = ExtResource("2_fghij")

[node name="Sprite2D" type="Sprite2D" parent="Player" unique_id=410395720]
texture = ExtResource("1_abcde")

[node name="CollisionShape2D" type="CollisionShape2D" parent="Player" unique_id=2048337421]
shape = SubResource("RectangleShape2D_klmno")

[node name="Label" type="Label" parent="." unique_id=11]
offset_right = 40.0
offset_bottom = 23.0
text = "Score: \"0\""

[connection signal="ready" from="." to="Player" method="_on_ready"]
[connection signal="tree_exited" from="." to="Player" method="_on_exit"]
"#;

    #[test]
    fn godot_scene_round_trips_byte_for_byte() {
        let doc = Document::parse(GODOT).unwrap();
        assert_eq!(doc.sections.len(), 11);
        assert_eq!(doc.sections[5].field_str("name"), Some("Player"));
        assert_eq!(doc.sections[5].field("groups"), Some(&Value::Array(vec![Value::String("player".into())])));
        assert_eq!(doc.write(), GODOT);
    }

    #[test]
    fn comments_and_quoted_names() {
        let doc = Document::parse("; comment\n[resource]\n\"odd name\" = 1\nmetadata/_edit_lock_ = true\n").unwrap();
        assert_eq!(doc.sections[0].props[0].0, "odd name");
        assert_eq!(doc.write(), "[resource]\n\"odd name\" = 1\nmetadata/_edit_lock_ = true\n");
    }
}
