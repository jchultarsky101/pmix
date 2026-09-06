//! Decoding of Part 21 string literals.
//!
//! The raw content between the quotes may contain:
//!
//! - `''` for a literal apostrophe;
//! - `\\` for a literal backslash;
//! - `\N\` for a newline;
//! - `\S\c` for the character `c + 0x80` in the current 8-bit code page
//!   (ISO 8859-1 by default, `\P?\` selects another page; we treat all pages
//!   as Latin-1, which is exact for the default and close for the rest);
//! - `\X\hh` for the ISO 8859-1 character with code `hh`;
//! - `\X2\hhhh...\X0\` for UTF-16BE code units;
//! - `\X4\hhhhhhhh...\X0\` for UTF-32BE code points.
//!
//! Anything unrecognised is kept verbatim. Bytes above 0x7F that appear
//! raw (which the standard forbids but exporters emit) are decoded as UTF-8
//! with replacement characters for invalid sequences.

/// Decode the raw bytes of a string literal (without the surrounding quotes).
pub fn decode(raw: &[u8]) -> String {
    // Fast path: plain ASCII without escapes.
    if raw
        .iter()
        .all(|b| b.is_ascii() && *b != b'\\' && *b != b'\'')
    {
        // SAFETY-free: checked ASCII above.
        return String::from_utf8_lossy(raw).into_owned();
    }

    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        match b {
            b'\'' => {
                // Doubled apostrophe. A lone one cannot occur inside a
                // well-formed literal, but tolerate it.
                out.push('\'');
                i += if raw.get(i + 1) == Some(&b'\'') { 2 } else { 1 };
            }
            b'\\' => {
                let (consumed, ok) = decode_escape(&raw[i..], &mut out);
                if ok {
                    i += consumed;
                } else {
                    out.push('\\');
                    i += 1;
                }
            }
            _ if b.is_ascii() => {
                out.push(b as char);
                i += 1;
            }
            _ => {
                // Raw non-ASCII: take the maximal run and decode as UTF-8.
                let start = i;
                while i < raw.len() && !raw[i].is_ascii() {
                    i += 1;
                }
                out.push_str(&String::from_utf8_lossy(&raw[start..i]));
            }
        }
    }
    out
}

/// Try to decode one escape sequence at the start of `s` (which begins with
/// a backslash). Returns `(bytes consumed, success)`.
fn decode_escape(s: &[u8], out: &mut String) -> (usize, bool) {
    // s[0] == b'\\'
    let Some(&tag) = s.get(1) else {
        return (0, false);
    };
    match tag {
        b'\\' => {
            out.push('\\');
            (2, true)
        }
        b'N' if s.get(2) == Some(&b'\\') => {
            out.push('\n');
            (3, true)
        }
        b'S' if s.get(2) == Some(&b'\\') => match s.get(3) {
            Some(&c) if c.is_ascii() => {
                out.push(char::from(c.wrapping_add(0x80)));
                (4, true)
            }
            _ => (0, false),
        },
        // \P?\ : code page selector, consumed without output.
        b'P' if s.get(2) == Some(&b'\\') && s.get(4) == Some(&b'\\') => (5, true),
        b'X' => match s.get(2) {
            // \X\hh
            Some(&b'\\') => match hex_byte(s.get(3..5)) {
                Some(v) => {
                    out.push(char::from(v));
                    (5, true)
                }
                None => (0, false),
            },
            // \X2\....\X0\  and  \X4\....\X0\
            Some(&width @ (b'2' | b'4')) if s.get(3) == Some(&b'\\') => {
                let unit = if width == b'2' { 4 } else { 8 };
                let body = &s[4..];
                let Some(end) = find_x0(body) else {
                    return (0, false);
                };
                let hex = &body[..end];
                if hex.len() % unit != 0 || !hex.iter().all(u8::is_ascii_hexdigit) {
                    return (0, false);
                }
                if unit == 4 {
                    let units: Vec<u16> = hex
                        .chunks(4)
                        .map(|c| u16::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
                        .collect();
                    out.extend(char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')));
                } else {
                    for c in hex.chunks(8) {
                        let v = u32::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap();
                        out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
                    }
                }
                // 4 (\X2\) + hex + 4 (\X0\)
                (4 + end + 4, true)
            }
            _ => (0, false),
        },
        _ => (0, false),
    }
}

fn hex_byte(s: Option<&[u8]>) -> Option<u8> {
    let s = s?;
    if s.len() != 2 || !s.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u8::from_str_radix(std::str::from_utf8(s).ok()?, 16).ok()
}

/// Position of `\X0\` in `s`, if present.
fn find_x0(s: &[u8]) -> Option<usize> {
    s.windows(4).position(|w| w == b"\\X0\\")
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn plain_and_apostrophe() {
        assert_eq!(decode(b"hello"), "hello");
        assert_eq!(decode(b"it''s"), "it's");
        assert_eq!(decode(b""), "");
    }

    #[test]
    fn backslash_and_newline() {
        assert_eq!(decode(br"a\\b"), r"a\b");
        assert_eq!(decode(br"a\N\b"), "a\nb");
    }

    #[test]
    fn latin1_escapes() {
        assert_eq!(decode(br"\S\d"), "ä"); // 'd' = 0x64 + 0x80 = 0xE4
        assert_eq!(decode(br"\X\E9"), "é");
        assert_eq!(decode(br"\P\A\\S\d"), "ä"); // code page selector ignored
    }

    #[test]
    fn utf16_and_utf32_escapes() {
        assert_eq!(decode(br"\X2\00E9\X0\"), "é");
        assert_eq!(decode(br"\X2\2300\X0\ 12"), "⌀ 12");
        assert_eq!(decode(br"\X2\00E900E8\X0\"), "éè");
        assert_eq!(decode(br"\X4\0001F600\X0\"), "😀");
    }

    #[test]
    fn malformed_escapes_are_kept_verbatim() {
        assert_eq!(decode(br"\X2\zz\X0\"), r"\X2\zz\X0\");
        assert_eq!(decode(br"\Q\"), r"\Q\");
        assert_eq!(decode(br"trailing\"), r"trailing\");
    }

    #[test]
    fn raw_utf8_is_tolerated() {
        assert_eq!(decode("⌀12".as_bytes()), "⌀12");
    }
}
