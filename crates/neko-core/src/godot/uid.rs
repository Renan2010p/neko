//! `uid://` identifiers (core/io/resource_uid.cpp).

#[allow(unused_imports)]
use neko_base::prelude::*;

const CHARS: &[u8; 34] = b"abcdefghijklmnopqrstuvwxy012345678";

/// `uid://...` text for an id (non-negative 63-bit).
pub fn id_to_text(mut id: u64) -> String {
    id &= 0x7FFF_FFFF_FFFF_FFFF;
    let mut tmp = Vec::new();
    loop {
        tmp.push(CHARS[(id % 34) as usize]);
        id /= 34;
        if id == 0 {
            break;
        }
    }
    tmp.reverse();
    format!("uid://{}", String::from_utf8(tmp).expect("ascii"))
}

/// The id of `uid://...` text, or `None` if invalid.
pub fn text_to_id(text: &str) -> Option<u64> {
    let digits = text.strip_prefix("uid://")?;
    if digits.is_empty() || digits == "<invalid>" {
        return None;
    }
    let mut id: u64 = 0;
    for b in digits.bytes() {
        let d = CHARS.iter().position(|&c| c == b)? as u64;
        id = id.checked_mul(34)?.checked_add(d)?;
    }
    Some(id & 0x7FFF_FFFF_FFFF_FFFF)
}

/// Where new ids come from: the OS with `std`, the backend otherwise.
pub trait IdSource {
    fn next_u64(&mut self) -> u64;
}

/// Ids from the operating system's entropy (`ResourceUID::create_id` uses
/// mbedTLS's random generator).
#[cfg(feature = "std")]
#[derive(Default)]
pub struct OsIds;

#[cfg(feature = "std")]
impl IdSource for OsIds {
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        if getrandom::fill(&mut b).is_err() {
            // Extremely unlikely; fall back to time-based bits.
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
            b = t.to_le_bytes();
        }
        u64::from_le_bytes(b)
    }
}

/// A seeded generator (SplitMix64) for targets without OS entropy: seed it
/// from something that varies (a hardware timer, the frame counter).
pub struct SeededIds(pub u64);

impl IdSource for SeededIds {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// A fresh UID (non-negative 63-bit).
pub fn create_id_from(src: &mut dyn IdSource) -> u64 {
    src.next_u64() & 0x7FFF_FFFF_FFFF_FFFF
}

/// A node's `unique_id` (positive 31-bit, never 0), as PackedScene assigns them.
pub fn create_node_id_from(src: &mut dyn IdSource) -> i32 {
    let id = (src.next_u64() & 0x7FFF_FFFF) as i32;
    if id == 0 { 1 } else { id }
}

/// Godot's `Resource::generate_scene_unique_id`: five chars of a-y and 0-8.
pub fn scene_unique_id_from(src: &mut dyn IdSource) -> String {
    let mut n = src.next_u64() as u32;
    let mut s = String::with_capacity(5);
    for _ in 0..5 {
        let c = n % 34;
        s.push(if c < 25 { (b'a' + c as u8) as char } else { (b'0' + (c - 25) as u8) as char });
        n /= 34;
    }
    s
}

/// A fresh UID from the OS.
#[cfg(feature = "std")]
pub fn create_id() -> u64 {
    create_id_from(&mut OsIds)
}

/// A node `unique_id` from the OS.
#[cfg(feature = "std")]
pub fn create_node_id() -> i32 {
    create_node_id_from(&mut OsIds)
}

/// A scene-unique resource id from the OS.
#[cfg(feature = "std")]
pub fn scene_unique_id() -> String {
    scene_unique_id_from(&mut OsIds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_godot_encoding() {
        // The maximum id from resource_uid.cpp's comment.
        assert_eq!(id_to_text(0x7FFF_FFFF_FFFF_FFFF), "uid://d4n4ub6itg400");
        assert_eq!(text_to_id("uid://d4n4ub6itg400"), Some(0x7FFF_FFFF_FFFF_FFFF));
        let id = create_id();
        assert_eq!(text_to_id(&id_to_text(id)), Some(id));
        assert_eq!(text_to_id("uid://<invalid>"), None);
        assert_eq!(scene_unique_id().len(), 5);
    }
}
