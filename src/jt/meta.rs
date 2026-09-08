//! Property proxy metadata elements (specification section 8.2).
//!
//! A JT file states the properties of a part in a metadata segment: a
//! list of key and value pairs, ended by an empty key. The value may be
//! text, a whole number, a real number, or a date.
//!
//! These are separate from the scene graph's own property table, which
//! [`super::property`] reads. A file typically uses both.

use std::fmt;

use super::file::Guid;

/// Object type identifier of the Property Proxy Meta Data Element.
pub const PROPERTY_PROXY: Guid = Guid([
    0x47, 0x72, 0x35, 0xce, 0xfb, 0x38, 0xd1, 0x11, 0xa5, 0x06, 0x00, 0x60, 0x97, 0xbd, 0xc6, 0xe1,
]);

/// What a property says.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Integer(i32),
    Number(f64),
    /// A moment, rendered the way the file states it.
    Date(String),
    /// The file declared a value it did not write.
    Unset,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(v) => f.write_str(v),
            Self::Integer(v) => write!(f, "{v}"),
            Self::Number(v) => write!(f, "{v}"),
            Self::Date(v) => f.write_str(v),
            Self::Unset => Ok(()),
        }
    }
}

/// Read the properties of a property proxy element's object data.
///
/// A malformed pair ends the list rather than failing the read: the
/// properties before it are still what the file says.
pub fn parse(data: &[u8]) -> Vec<(String, Value)> {
    let mut at = 1; // the element's version number
    let mut out = Vec::new();
    while let Some(key) = string(data, &mut at) {
        if key.is_empty() {
            break;
        }
        let Some(kind) = byte(data, &mut at) else {
            break;
        };
        let value = match kind {
            0 => Value::Unset,
            1 => match string(data, &mut at) {
                Some(v) => Value::Text(v),
                None => break,
            },
            2 => match int(data, &mut at) {
                Some(v) => Value::Integer(v),
                None => break,
            },
            3 => match float(data, &mut at) {
                Some(v) => Value::Number(v),
                None => break,
            },
            4 => match date(data, &mut at) {
                Some(v) => Value::Date(v),
                None => break,
            },
            // A type this reader does not know leaves the rest of the
            // list unreadable, because its length is unknown.
            _ => break,
        };
        out.push((key, value));
        if out.len() > 4096 {
            break;
        }
    }
    out
}

fn byte(data: &[u8], at: &mut usize) -> Option<u8> {
    let v = *data.get(*at)?;
    *at += 1;
    Some(v)
}

fn int(data: &[u8], at: &mut usize) -> Option<i32> {
    let v = i32::from_le_bytes(data.get(*at..*at + 4)?.try_into().ok()?);
    *at += 4;
    Some(v)
}

fn float(data: &[u8], at: &mut usize) -> Option<f64> {
    let v = f32::from_le_bytes(data.get(*at..*at + 4)?.try_into().ok()?);
    *at += 4;
    Some(v as f64)
}

fn short(data: &[u8], at: &mut usize) -> Option<i16> {
    let v = i16::from_le_bytes(data.get(*at..*at + 2)?.try_into().ok()?);
    *at += 2;
    Some(v)
}

/// Year, month, day, hour, minute, and second, each a 16-bit number.
fn date(data: &[u8], at: &mut usize) -> Option<String> {
    let parts: Vec<i16> = (0..6).map(|_| short(data, at)).collect::<Option<_>>()?;
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
    ))
}

/// An `MbString`: a count of UTF-16 code units, then the units.
fn string(data: &[u8], at: &mut usize) -> Option<String> {
    let count = int(data, at)?;
    if count < 0 {
        return None;
    }
    let bytes = data.get(*at..*at + count as usize * 2)?;
    *at += count as usize * 2;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units))
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

    #[test]
    fn reads_every_value_type_and_stops_at_the_empty_key() {
        let mut d = vec![1u8];
        d.extend(mb("Part Number"));
        d.push(1);
        d.extend(mb("SYN-004-REV-A"));
        d.extend(mb("Quantity"));
        d.push(2);
        d.extend(7i32.to_le_bytes());
        d.extend(mb("Mass"));
        d.push(3);
        d.extend(2.5f32.to_le_bytes());
        d.extend(mb("Checked"));
        d.push(4);
        for v in [2026i16, 9, 8, 14, 30, 0] {
            d.extend(v.to_le_bytes());
        }
        d.extend(mb("Nothing"));
        d.push(0);
        d.extend(mb("")); // the empty key ends the list
        d.extend(mb("After"));

        let got = parse(&d);
        assert_eq!(
            got,
            [
                (
                    "Part Number".to_owned(),
                    Value::Text("SYN-004-REV-A".into())
                ),
                ("Quantity".to_owned(), Value::Integer(7)),
                ("Mass".to_owned(), Value::Number(2.5)),
                (
                    "Checked".to_owned(),
                    Value::Date("2026-09-08T14:30:00".into())
                ),
                ("Nothing".to_owned(), Value::Unset),
            ]
        );
    }

    #[test]
    fn a_truncated_list_keeps_what_it_read() {
        let mut d = vec![1u8];
        d.extend(mb("Kept"));
        d.push(1);
        d.extend(mb("yes"));
        d.extend(mb("Cut off"));
        d.push(2); // an integer that is not there
        let got = parse(&d);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "Kept");
        assert!(parse(&[]).is_empty());
        assert!(parse(&[1]).is_empty());
    }

    #[test]
    fn an_unknown_value_type_ends_the_list() {
        let mut d = vec![1u8];
        d.extend(mb("Known"));
        d.push(1);
        d.extend(mb("v"));
        d.extend(mb("Odd"));
        d.push(9);
        d.extend(mb("Never reached"));
        let got = parse(&d);
        assert_eq!(got.len(), 1);
    }
}
