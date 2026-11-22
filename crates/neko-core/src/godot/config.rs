//! `ConfigFile` syntax (core/io/config_file.cpp), used by `project.godot`
//! and Neko's `project.neko`:
//!
//! ```text
//! ; comment
//! config_version=5
//!
//! [application]
//!
//! config/name="My Game"
//! run/main_scene="res://main.tscn"
//! ```
//!
//! Keys before the first section belong to the section `""`.

#[allow(unused_imports)]
use neko_base::prelude::*;

use super::value::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigFile {
    /// Sections in file order, each with its keys in file order.
    pub sections: Vec<(String, Vec<(String, Value)>)>,
}

impl ConfigFile {
    pub fn get(&self, section: &str, key: &str) -> Option<&Value> {
        self.sections.iter().find(|(s, _)| s == section)?.1.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_str(&self, section: &str, key: &str) -> Option<&str> {
        self.get(section, key).and_then(Value::as_str)
    }

    /// Set a value, keeping the order of existing keys and sections.
    pub fn set(&mut self, section: &str, key: &str, value: Value) {
        let idx = match self.sections.iter().position(|(s, _)| s == section) {
            Some(i) => i,
            None => {
                self.sections.push((section.to_string(), Vec::new()));
                self.sections.len() - 1
            }
        };
        let keys = &mut self.sections[idx].1;
        match keys.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => keys.push((key.to_string(), value)),
        }
    }

    pub fn remove(&mut self, section: &str, key: &str) {
        if let Some((_, keys)) = self.sections.iter_mut().find(|(s, _)| s == section) {
            keys.retain(|(k, _)| k != key);
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cfg = ConfigFile::default();
        let mut section = String::new();
        let mut rest = text;
        let mut line = 1usize;
        loop {
            let t = rest.trim_start();
            line += rest[..rest.len() - t.len()].matches('\n').count();
            rest = t;
            if rest.is_empty() {
                return Ok(cfg);
            }
            if rest.starts_with(';') || rest.starts_with('#') {
                rest = rest.find('\n').map_or("", |i| &rest[i..]);
                continue;
            }
            if let Some(after) = rest.strip_prefix('[') {
                let end = after.find(']').ok_or_else(|| format!("line {line}: unterminated section"))?;
                section = after[..end].to_string();
                if !cfg.sections.iter().any(|(s, _)| *s == section) {
                    cfg.sections.push((section.clone(), Vec::new()));
                }
                rest = &after[end + 1..];
                continue;
            }
            let eq = rest.find('=').ok_or_else(|| format!("line {line}: expected key=value"))?;
            let key = rest[..eq].trim();
            let key = key.strip_prefix('"').and_then(|k| k.strip_suffix('"')).unwrap_or(key).to_string();
            let (v, after) = Value::parse_prefix(&rest[eq + 1..]).map_err(|e| format!("line {line}: {key}: {e}"))?;
            line += rest[..rest.len() - after.len()].matches('\n').count();
            rest = after;
            cfg.set(&section, &key, v);
        }
    }

    /// Write like `ConfigFile::save`: `key=value`, a blank line after each
    /// section header and between sections.
    pub fn write(&self, header_comment: &str) -> String {
        let mut out = String::new();
        for line in header_comment.lines() {
            out.push(';');
            if !line.is_empty() {
                out.push(' ');
                out.push_str(line);
            }
            out.push('\n');
        }
        if !header_comment.is_empty() {
            out.push('\n');
        }
        let mut first = true;
        for (section, keys) in &self.sections {
            if keys.is_empty() && !section.is_empty() {
                continue;
            }
            if !first {
                out.push('\n');
            }
            first = false;
            if !section.is_empty() {
                out.push('[');
                out.push_str(section);
                out.push_str("]\n\n");
            }
            for (k, v) in keys {
                out.push_str(k);
                out.push('=');
                v.write(&mut out);
                out.push('\n');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "; Engine configuration file.\n\nconfig_version=5\n\n[application]\n\nconfig/name=\"Cat Game\"\nrun/main_scene=\"res://main.tscn\"\nconfig/features=PackedStringArray(\"4.7\", \"Forward Plus\")\n\n[display]\n\nwindow/size/viewport_width=640\n";

    #[test]
    fn project_file_round_trips() {
        let c = ConfigFile::parse(PROJECT).unwrap();
        assert_eq!(c.get_str("application", "config/name"), Some("Cat Game"));
        assert_eq!(c.get("display", "window/size/viewport_width"), Some(&Value::Int(640)));
        assert_eq!(c.get("", "config_version"), Some(&Value::Int(5)));
        assert_eq!(c.write("Engine configuration file."), PROJECT);
    }
}
