//! The element stream inside a decoded segment.
//!
//! A segment's payload is a sequence of elements, each introduced by its
//! length and an identifier saying what the element holds. The stream ends
//! with a marker element whose identifier is every byte `0xFF`.

use super::file::Guid;

/// One element of a segment.
#[derive(Debug, Clone, PartialEq)]
pub struct Element<'a> {
    /// What the element holds. Named object types are listed in the
    /// specification's annex A.
    pub object_type: Guid,
    /// The base object type, which says how to read the object data.
    pub base_type: u8,
    /// Identifier other elements use to reference this one.
    pub object_id: i32,
    /// The element's own bytes, after the header.
    pub data: &'a [u8],
    /// Offset of the element within the segment payload.
    pub offset: usize,
}

/// Bytes of an element header after its length field: the object type,
/// base type, and object identifier.
const HEADER_AFTER_LENGTH: usize = 16 + 1 + 4;

/// Walks the elements of a decoded segment payload.
///
/// Iteration stops at the end-of-elements marker, at the end of the
/// payload, or at the first malformed length, so a truncated or
/// unexpected stream yields what could be read rather than panicking.
#[derive(Debug, Clone)]
pub struct Elements<'a> {
    payload: &'a [u8],
    pos: usize,
    done: bool,
}

impl<'a> Elements<'a> {
    pub fn new(payload: &'a [u8]) -> Self {
        Self {
            payload,
            pos: 0,
            done: false,
        }
    }

    /// Byte offset reached so far.
    pub fn position(&self) -> usize {
        self.pos
    }
}

impl<'a> Iterator for Elements<'a> {
    type Item = Element<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let start = self.pos;
        let length_bytes = self.payload.get(start..start + 4)?;
        let length = i32::from_le_bytes(length_bytes.try_into().ok()?);
        if length < 0 {
            self.done = true;
            return None;
        }
        let length = length as usize;
        let object_type = Guid(self.payload.get(start + 4..start + 20)?.try_into().ok()?);
        if object_type == Guid::END_OF_ELEMENTS {
            self.done = true;
            return None;
        }
        // The length covers everything after the length field, so an
        // element shorter than its own header is malformed.
        if length < HEADER_AFTER_LENGTH {
            self.done = true;
            return None;
        }
        let base_type = *self.payload.get(start + 20)?;
        let object_id =
            i32::from_le_bytes(self.payload.get(start + 21..start + 25)?.try_into().ok()?);
        let data_start = start + 4 + HEADER_AFTER_LENGTH;
        let data_end = start + 4 + length;
        let data = self
            .payload
            .get(data_start..data_end.min(self.payload.len()))?;
        self.pos = data_end;
        Some(Element {
            object_type,
            base_type,
            object_id,
            data,
            offset: start,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an element: length, type identifier, base type, object id, data.
    fn element(type_byte: u8, id: i32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(((HEADER_AFTER_LENGTH + data.len()) as i32).to_le_bytes());
        out.extend([type_byte; 16]);
        out.push(9);
        out.extend(id.to_le_bytes());
        out.extend(data);
        out
    }

    fn terminator() -> Vec<u8> {
        let mut out = 16i32.to_le_bytes().to_vec();
        out.extend([0xFF; 16]);
        out.extend([1, 0, 0, 0, 0, 0]);
        out
    }

    #[test]
    fn walks_elements_and_stops_at_the_marker() {
        let mut payload = element(0xAA, 7, b"first");
        payload.extend(element(0xBB, 8, b"second data"));
        payload.extend(terminator());
        payload.extend(b"trailing bytes that must be ignored");

        let found: Vec<_> = Elements::new(&payload).collect();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].object_id, 7);
        assert_eq!(found[0].data, b"first");
        assert_eq!(found[0].base_type, 9);
        assert_eq!(found[0].offset, 0);
        assert_eq!(found[1].object_id, 8);
        assert_eq!(found[1].data, b"second data");
        assert_eq!(found[1].object_type, Guid([0xBB; 16]));
    }

    #[test]
    fn a_truncated_stream_yields_what_it_can() {
        let full = element(0xAA, 1, b"payload");
        for cut in [0, 3, 10, 24] {
            let found: Vec<_> = Elements::new(&full[..cut.min(full.len())]).collect();
            assert!(
                found.is_empty(),
                "a stream cut to {cut} bytes has no whole element"
            );
        }
        // A complete element followed by a partial one yields just the first.
        let mut payload = full.clone();
        payload.extend(&full[..8]);
        assert_eq!(Elements::new(&payload).count(), 1);
    }

    #[test]
    fn an_impossible_length_stops_iteration() {
        let mut payload = 2i32.to_le_bytes().to_vec(); // shorter than a header
        payload.extend([0xAB; 16]);
        payload.extend([0u8; 8]);
        assert_eq!(Elements::new(&payload).count(), 0);

        let mut negative = (-5i32).to_le_bytes().to_vec();
        negative.extend([0u8; 32]);
        assert_eq!(Elements::new(&negative).count(), 0);
    }
}
