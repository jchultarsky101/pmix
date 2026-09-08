//! Properties attached to scene-graph elements (specification section 5.6).
//!
//! A JT file records its model units, part names, and similar metadata as
//! string property atoms in the logical scene graph, tied to elements by a
//! property table that follows the element stream. `pmix` reads the table
//! to learn the unit that PMI measures are expressed in.

use std::collections::BTreeMap;

use super::element::Elements;
use super::file::Guid;

/// Object type identifier of the String Property Atom Element.
pub const STRING_PROPERTY_ATOM: Guid = Guid([
    0x6e, 0x10, 0xdd, 0x10, 0xc8, 0x2a, 0xd1, 0x11, 0x9b, 0x6b, 0x00, 0x80, 0xc7, 0xbb, 0x59, 0x97,
]);

/// Bytes of the end-of-elements marker: its length field and the identifier.
const MARKER: usize = 4 + 16;

/// The string properties a segment carries, by the element they describe.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties {
    /// Key and value pairs, keyed by the object identifier they belong to.
    pub by_element: BTreeMap<i32, Vec<(String, String)>>,
}

impl Properties {
    /// The first value stored under `key` anywhere in the segment.
    pub fn find(&self, key: &str) -> Option<&str> {
        self.by_element
            .values()
            .flatten()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Read an `MbString` at `pos`: a count of UTF-16 code units, then the units.
fn string_at(data: &[u8], pos: usize) -> Option<String> {
    let count = i32::from_le_bytes(data.get(pos..pos + 4)?.try_into().ok()?);
    let count = usize::try_from(count).ok()?;
    let bytes = data.get(pos + 4..pos + 4 + count.checked_mul(2)?)?;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// Whether an element stream begins at `pos`, judged by whether the first
/// element's length is possible. The property table that follows the last
/// stream begins with a version number and a count, which together make an
/// implausible length, so this tells the two apart.
fn starts_a_stream(payload: &[u8], pos: usize) -> bool {
    let Some(bytes) = payload.get(pos..pos + 4) else {
        return false;
    };
    let length = i32::from_le_bytes(bytes.try_into().unwrap());
    // Every element header is 21 bytes; the marker's payload is its
    // 16-byte identifier.
    (16..=i32::MAX).contains(&length) && pos + 4 + length as usize <= payload.len()
}

/// Read the string properties of a decoded segment payload.
///
/// Anything malformed yields fewer properties rather than an error: the
/// properties are supporting metadata, and a file whose table cannot be
/// read should still give up its PMI.
pub fn read(payload: &[u8]) -> Properties {
    // A scene graph holds several element streams one after another, each
    // closed by the end-of-elements marker: the nodes and attributes come
    // first and the property atoms after them. Walk them all, then read
    // the table that follows the last one.
    let mut atoms: BTreeMap<i32, String> = BTreeMap::new();
    let mut start = 0;
    while starts_a_stream(payload, start) {
        let mut elements = Elements::new(&payload[start..]);
        for element in elements.by_ref() {
            if element.object_type != STRING_PROPERTY_ATOM {
                continue;
            }
            // Base property atom data is a version byte and state flags,
            // then this element's own version byte, then the value.
            if let Some(value) = string_at(element.data, 1 + 4 + 1) {
                atoms.insert(element.object_id, value);
            }
        }
        let next = start + elements.position() + MARKER;
        if next <= start {
            break;
        }
        start = next;
    }

    let mut out = Properties::default();
    let Some(rest) = payload.get(start..) else {
        return out;
    };
    let int = |pos: usize| -> Option<i32> {
        rest.get(pos..pos + 4)
            .and_then(|b| b.try_into().ok())
            .map(i32::from_le_bytes)
    };
    let mut pos = 2; // the table's version number
    let Some(count) = int(pos) else { return out };
    pos += 4;
    for _ in 0..count.max(0) {
        let Some(owner) = int(pos) else { return out };
        pos += 4;
        let mut pairs = Vec::new();
        loop {
            let Some(key) = int(pos) else { return out };
            pos += 4;
            if key == 0 {
                break;
            }
            let Some(value) = int(pos) else { return out };
            pos += 4;
            if let (Some(k), Some(v)) = (atoms.get(&key), atoms.get(&value)) {
                pairs.push((k.clone(), v.clone()));
            }
        }
        if !pairs.is_empty() {
            out.by_element.entry(owner).or_default().extend(pairs);
        }
    }
    out
}

/// Canonical unit name for a JT `JT_PROP_MEASUREMENT_UNITS` value.
pub fn unit_name(declared: &str) -> Option<&'static str> {
    Some(match declared.trim().to_ascii_lowercase().as_str() {
        "micrometers" => "um",
        "millimeters" => "mm",
        "centimeters" => "cm",
        "decimeters" => "dm",
        "meters" => "m",
        "kilometers" => "km",
        "inches" => "in",
        "feet" => "ft",
        "yards" => "yd",
        "miles" => "mi",
        "mils" => "mil",
        _ => return None,
    })
}

/// How many of `unit` make a metre.
///
/// JT writes some properties in metres, its base unit, while PMI
/// geometry is in the unit the model declares, so converting between
/// them needs this.
pub fn per_metre(unit: &str) -> Option<f64> {
    Some(match unit {
        "um" => 1e6,
        "mm" => 1e3,
        "cm" => 1e2,
        "dm" => 1e1,
        "m" => 1.0,
        "km" => 1e-3,
        "in" => 1.0 / 0.0254,
        "ft" => 1.0 / 0.3048,
        "yd" => 1.0 / 0.9144,
        "mi" => 1.0 / 1609.344,
        "mil" => 1.0 / 0.0000254,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mb(s: &str) -> Vec<u8> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut out = (units.len() as i32).to_le_bytes().to_vec();
        for u in units {
            out.extend(u.to_le_bytes());
        }
        out
    }

    fn atom(id: i32, value: &str) -> Vec<u8> {
        let mut data = vec![1u8]; // base version
        data.extend(0u32.to_le_bytes()); // state flags
        data.push(1); // element version
        data.extend(mb(value));
        let mut out = ((21 + data.len()) as i32).to_le_bytes().to_vec();
        out.extend(STRING_PROPERTY_ATOM.0);
        out.push(1);
        out.extend(id.to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn reads_a_property_table() {
        let mut payload = atom(7, "JT_PROP_MEASUREMENT_UNITS");
        payload.extend(atom(8, "millimeters"));
        payload.extend(16i32.to_le_bytes());
        payload.extend([0xFF; 16]);
        payload.extend(1i16.to_le_bytes()); // table version
        payload.extend(1i32.to_le_bytes()); // one element table
        payload.extend(42i32.to_le_bytes()); // owning element
        payload.extend(7i32.to_le_bytes());
        payload.extend(8i32.to_le_bytes());
        payload.extend(0i32.to_le_bytes()); // end of this element's list

        let props = read(&payload);
        assert_eq!(props.find("JT_PROP_MEASUREMENT_UNITS"), Some("millimeters"));
        assert_eq!(props.by_element[&42].len(), 1);
        assert_eq!(props.find("absent"), None);
    }

    #[test]
    fn a_truncated_table_yields_what_it_read() {
        let mut payload = atom(7, "key");
        payload.extend(16i32.to_le_bytes());
        payload.extend([0xFF; 16]);
        payload.extend(1i16.to_le_bytes());
        payload.extend(9i32.to_le_bytes()); // claims nine tables
        assert!(read(&payload).by_element.is_empty());
        assert!(read(&[]).by_element.is_empty());
    }

    #[test]
    fn unit_names_are_canonical() {
        assert_eq!(unit_name("Millimeters"), Some("mm"));
        assert_eq!(unit_name(" inches "), Some("in"));
        assert_eq!(unit_name("furlongs"), None);
    }

    #[test]
    fn every_unit_name_converts_from_metres() {
        for name in ["Micrometers", "Millimeters", "Meters", "Inches", "Feet"] {
            let unit = unit_name(name).unwrap();
            assert!(per_metre(unit).is_some_and(|f| f > 0.0), "{unit}");
        }
        assert_eq!(per_metre("mm"), Some(1000.0));
        assert_eq!(per_metre("furlong"), None);
    }
}
