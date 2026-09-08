//! The Int32 compressed data packet (specification section 10.2.1).
//!
//! JT stores the integer arrays of its topology and geometry tables as
//! compressed packets rather than plain vectors. A packet says how many
//! values it holds, which codec encoded them, and how many bits of code
//! text follow; the values are then reconstructed from those bits and,
//! where a predictor was used, from the value before them.
//!
//! Only the codecs the reader has met in real files are implemented. An
//! unimplemented one is reported rather than guessed at, because a wrong
//! guess would produce plausible numbers rather than an error.

use std::fmt;

/// A packet that could not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct CodecError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "compressed packet at byte {}: {}",
            self.offset, self.message
        )
    }
}

impl std::error::Error for CodecError {}

type Result<T> = std::result::Result<T, CodecError>;

/// How a value relates to the one before it (specification `PredictorType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Predictor {
    /// Each value is a difference from the one before.
    Lag1,
    /// Each value is the exclusive or of the one before.
    Xor1,
    /// Values stand alone.
    None,
}

/// Bits used for the field-width change at the head of a run.
const BLOCK_WIDTH_BITS: u32 = 3;
/// Bits used for the length of a run.
const BLOCK_LENGTH_BITS: u32 = 4;
/// Bits per digit of the variable-length integers a packet header uses.
const NIBBLE_BITS: u32 = 4;

/// Reads bits from the packed code text, most significant bit first.
struct Bits<'a> {
    words: &'a [u8],
    next: usize,
    /// The current word, already shifted so the next bit is the highest.
    val: u32,
    /// How many bits of `val` are still unread.
    held: u32,
}

impl<'a> Bits<'a> {
    fn new(words: &'a [u8]) -> Self {
        Self {
            words,
            next: 0,
            val: 0,
            held: 0,
        }
    }

    /// Take the next whole word into the buffer.
    fn refill(&mut self) {
        let word = self
            .words
            .get(self.next..self.next + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .unwrap_or(0);
        self.next += 4;
        self.val = word;
        self.held = 32;
    }

    fn unsigned(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        if self.held == 0 {
            self.refill();
        }
        if self.held >= n {
            let out = self.val >> (32 - n);
            self.val = self.val.checked_shl(n).unwrap_or(0);
            self.held -= n;
            return out;
        }
        // The value straddles two words.
        let taken = self.held;
        let mut out = self.val >> (32 - n);
        self.refill();
        let rest = n - taken;
        out |= self.val >> (32 - rest);
        self.val = self.val.checked_shl(rest).unwrap_or(0);
        self.held -= rest;
        out
    }

    fn signed(&mut self, n: u32) -> i32 {
        let raw = self.unsigned(n);
        if n == 0 || n >= 32 {
            return raw as i32;
        }
        // Sign-extend from the top bit of the field.
        ((raw << (32 - n)) as i32) >> (32 - n)
    }

    /// A variable-length integer: digits of [`NIBBLE_BITS`] bits, each
    /// followed by a bit saying whether another digit follows.
    fn nibbler(&mut self) -> i32 {
        let mut value: u32 = 0;
        let mut digits = 0;
        loop {
            let digit = self.unsigned(NIBBLE_BITS);
            value |= digit << (digits * NIBBLE_BITS);
            digits += 1;
            if self.unsigned(1) == 0 || digits * NIBBLE_BITS >= 32 {
                break;
            }
        }
        let width = digits * NIBBLE_BITS;
        if width >= 32 {
            return value as i32;
        }
        ((value << (32 - width)) as i32) >> (32 - width)
    }
}

/// Bits needed to hold `v`, which is none at all for zero.
fn bit_width(v: u32) -> u32 {
    32 - v.leading_zeros()
}

/// A cursor over the bytes a table occupies.
pub struct Cursor<'a> {
    data: &'a [u8],
    pub at: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(CodecError {
            offset: self.at,
            message: message.into(),
        })
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        match self.data.get(self.at..self.at + n) {
            Some(s) => {
                self.at += n;
                Ok(s)
            }
            None => self.err(format!(
                "wanted {n} bytes, {} remain",
                self.data.len().saturating_sub(self.at)
            )),
        }
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    /// Read one compressed packet and undo `predictor`.
    pub fn packet(&mut self, predictor: Predictor) -> Result<Vec<i32>> {
        let count = self.i32()?;
        if count < 0 {
            return self.err(format!("negative value count {count}"));
        }
        let count = count as usize;
        if count == 0 {
            return Ok(Vec::new());
        }
        let remaining = self.data.len().saturating_sub(self.at);
        if count > remaining * 8 {
            return self.err(format!(
                "{count} values cannot fit in the {remaining} bytes that remain"
            ));
        }

        let codec = self.u8()?;
        let mut values = match codec {
            0 | 1 => {
                let bits = self.i32()?;
                if bits < 0 {
                    return self.err(format!("negative code text length {bits}"));
                }
                let words = (bits as usize).div_ceil(32);
                let text = self.take(words * 4)?;
                let mut reader = Bits::new(text);
                if codec == 0 {
                    (0..count).map(|_| reader.signed(32)).collect()
                } else {
                    bitlength(&mut reader, count)
                }
            }
            other => {
                return self.err(format!(
                    "codec {other} is not implemented; only the null and bitlength \
                     codecs have been seen in real files"
                ));
            }
        };

        match predictor {
            Predictor::None => {}
            Predictor::Lag1 => {
                for i in 1..values.len() {
                    values[i] = values[i].wrapping_add(values[i - 1]);
                }
            }
            Predictor::Xor1 => {
                for i in 1..values.len() {
                    values[i] ^= values[i - 1];
                }
            }
        }
        Ok(values)
    }

    /// [`Cursor::packet`] read as unsigned values.
    pub fn packet_u32(&mut self, predictor: Predictor) -> Result<Vec<u32>> {
        Ok(self
            .packet(predictor)?
            .into_iter()
            .map(|v| v as u32)
            .collect())
    }
}

/// Decode the bitlength codec: either one field width for every value, or
/// runs of values that each announce a change to the current width.
fn bitlength(bits: &mut Bits<'_>, count: usize) -> Vec<i32> {
    let mut out = Vec::with_capacity(count.min(1 << 16));
    if bits.unsigned(1) == 0 {
        let min = bits.nibbler();
        let max = bits.nibbler();
        let width = bit_width((max.wrapping_sub(min)) as u32);
        for _ in 0..count {
            out.push((bits.unsigned(width) as i32).wrapping_add(min));
        }
        return out;
    }

    let mean = bits.nibbler();
    let most_negative = -(1 << (BLOCK_WIDTH_BITS - 1));
    let most_positive = (1 << (BLOCK_WIDTH_BITS - 1)) - 1;
    let mut width: i32 = 0;
    // A run may be empty, so the loop is bounded by the values still
    // wanted rather than by trusting every block to make progress.
    let mut blocks = 0;
    while out.len() < count && blocks <= count + 64 {
        blocks += 1;
        loop {
            let step = bits.signed(BLOCK_WIDTH_BITS);
            width += step;
            if step != most_negative && step != most_positive {
                break;
            }
        }
        if !(0..=32).contains(&width) {
            break;
        }
        let run = bits.unsigned(BLOCK_LENGTH_BITS) as usize;
        for _ in 0..run.min(count - out.len()) {
            out.push(bits.signed(width as u32).wrapping_add(mean));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the bytes of a packet the way a writer would.
    fn packet_bytes(count: i32, codec: u8, bits: i32, words: &[u32]) -> Vec<u8> {
        let mut out = count.to_le_bytes().to_vec();
        out.push(codec);
        out.extend(bits.to_le_bytes());
        for w in words {
            out.extend(w.to_le_bytes());
        }
        out
    }

    #[test]
    fn bits_are_read_from_the_top_of_each_word() {
        let word = 0b1011_0000_0000_0000_0000_0000_0000_0000u32.to_le_bytes();
        let mut bits = Bits::new(&word);
        assert_eq!(bits.unsigned(1), 1);
        assert_eq!(bits.unsigned(1), 0);
        assert_eq!(bits.unsigned(2), 0b11);
        assert_eq!(bits.unsigned(4), 0);
    }

    #[test]
    fn a_value_may_straddle_two_words() {
        let words = [0x0000_000fu32, 0xf000_0000];
        let mut bytes = words[0].to_le_bytes().to_vec();
        bytes.extend(words[1].to_le_bytes());
        let mut bits = Bits::new(&bytes);
        assert_eq!(bits.unsigned(28), 0);
        // Four bits left in the first word, four taken from the second.
        assert_eq!(bits.unsigned(8), 0b1111_1111);
    }

    #[test]
    fn signed_fields_are_sign_extended() {
        let word = 0b1111_0111_0000_0000_0000_0000_0000_0000u32.to_le_bytes();
        let mut bits = Bits::new(&word);
        assert_eq!(bits.signed(4), -1);
        assert_eq!(bits.signed(4), 7);
    }

    #[test]
    fn an_empty_packet_reads_as_no_values() {
        let bytes = 0i32.to_le_bytes();
        let mut c = Cursor::new(&bytes);
        assert_eq!(c.packet(Predictor::None).unwrap(), Vec::<i32>::new());
        assert_eq!(c.at, 4);
    }

    #[test]
    fn a_predictor_is_undone_after_decoding() {
        // The null codec stores each value in a whole word, so the
        // predictor is the only thing under test.
        let bytes2 = packet_bytes(3, 0, 96, &[1, 2, 3]);
        let mut c = Cursor::new(&bytes2);
        assert_eq!(c.packet(Predictor::None).unwrap(), [1, 2, 3]);
        let mut c = Cursor::new(&bytes2);
        assert_eq!(c.packet(Predictor::Lag1).unwrap(), [1, 3, 6]);
        let mut c = Cursor::new(&bytes2);
        assert_eq!(c.packet(Predictor::Xor1).unwrap(), [1, 3, 0]);
    }

    #[test]
    fn an_unimplemented_codec_is_reported_rather_than_guessed() {
        for codec in [3u8, 4, 5, 9] {
            let bytes = packet_bytes(2, codec, 0, &[]);
            let mut c = Cursor::new(&bytes);
            let err = c.packet(Predictor::None).unwrap_err();
            assert!(err.message.contains("not implemented"), "{}", err.message);
        }
    }

    #[test]
    fn a_wild_count_is_refused_before_allocating() {
        let bytes = packet_bytes(i32::MAX, 1, 0, &[]);
        let mut c = Cursor::new(&bytes);
        assert!(c.packet(Predictor::None).is_err());
        let bytes = packet_bytes(-5, 1, 0, &[]);
        let mut c = Cursor::new(&bytes);
        assert!(c.packet(Predictor::None).is_err());
    }

    #[test]
    fn the_width_of_a_value_is_what_it_needs() {
        assert_eq!(bit_width(0), 0);
        assert_eq!(bit_width(1), 1);
        assert_eq!(bit_width(2), 2);
        assert_eq!(bit_width(255), 8);
    }
}
