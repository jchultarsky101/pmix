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
///
/// The specification is self-contradictory here: its prose describes a
/// three-bit field whose extremes are 3 and -4, while the code sample
/// beside it sets four. Real files settle it. With three, the one table
/// in the test file whose face identifiers use the adaptive path decodes
/// to values that repeat and do not start at zero; with four they are
/// distinct and start at zero, as identifiers must.
const BLOCK_WIDTH_BITS: u32 = 4;
/// Bits used for the length of a run.
const BLOCK_LENGTH_BITS: u32 = 4;
/// Bits per digit of the variable-length integers a packet header uses.
const NIBBLE_BITS: u32 = 4;

/// Reads bits most significant bit first.
///
/// Two things in a packet are bit packed and they are packed over
/// different units: the code text over 32-bit words, and the histogram
/// before it over plain bytes, padded to a whole byte at the end.
struct Bits<'a> {
    data: &'a [u8],
    next: usize,
    /// The current unit, already shifted so the next bit is the highest.
    val: u32,
    /// How many bits of `val` are still unread.
    held: u32,
    /// Bytes taken per refill: four for code text, one for a histogram.
    unit: usize,
}

impl<'a> Bits<'a> {
    /// Over the 32-bit words a packet's code text is written as.
    fn over_words(data: &'a [u8]) -> Self {
        Self {
            data,
            next: 0,
            val: 0,
            held: 0,
            unit: 4,
        }
    }

    /// Over plain bytes, as a histogram is written.
    fn over_bytes(data: &'a [u8]) -> Self {
        Self {
            data,
            next: 0,
            val: 0,
            held: 0,
            unit: 1,
        }
    }

    /// Take the next unit into the buffer.
    fn refill(&mut self) {
        let taken = self.data.get(self.next..self.next + self.unit);
        self.val = match (taken, self.unit) {
            (Some(b), 4) => u32::from_le_bytes(b.try_into().unwrap()),
            (Some(b), _) => u32::from(b[0]) << 24,
            (None, _) => 0,
        };
        self.next += self.unit;
        self.held = self.unit as u32 * 8;
    }

    /// The next `n` bits, most significant first.
    ///
    /// A read may span any number of refills, which matters for the
    /// byte-packed histogram where a single field is wider than a unit.
    fn unsigned(&mut self, n: u32) -> u32 {
        let mut out: u32 = 0;
        let mut left = n.min(32);
        while left > 0 {
            if self.held == 0 {
                self.refill();
            }
            let take = left.min(self.held);
            let chunk = self.val >> (32 - take);
            out = out.checked_shl(take).unwrap_or(0) | chunk;
            self.val = self.val.checked_shl(take).unwrap_or(0);
            self.held -= take;
            left -= take;
        }
        out
    }

    /// How many bits have been taken, which is what says where the
    /// byte-aligned data after a bit-packed block begins.
    fn consumed(&self) -> usize {
        self.next * 8 - self.held as usize
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

/// One value the arithmetic coder can emit, and how often it occurs.
#[derive(Debug, Clone, Copy)]
struct Symbol {
    /// Whether this stands for a value held outside the coded stream.
    escape: bool,
    /// Relative frequency, which is what gives the value its share of
    /// the coding range.
    count: u32,
    /// Where this symbol's share begins.
    cumulative: u32,
    value: i32,
}

/// The trimmed histogram the arithmetic coder works from.
#[derive(Debug, Clone, Default)]
struct Histogram {
    symbols: Vec<Symbol>,
    total: u32,
}

impl Histogram {
    /// The symbol whose share of the range covers `at`.
    fn at(&self, at: u32) -> Option<&Symbol> {
        self.symbols
            .iter()
            .rev()
            .find(|s| s.cumulative <= at)
            .or_else(|| self.symbols.first())
    }
}

/// Read the histogram, which is bit packed and padded to a whole byte.
fn histogram(bytes: &[u8]) -> Result<(Histogram, usize)> {
    let mut bits = Bits::over_bytes(bytes);
    let count = bits.unsigned(16) as usize;
    let count_bits = bits.unsigned(6);
    let value_bits = bits.unsigned(7);
    let minimum = bits.unsigned(32) as i32;
    // Each entry costs at least one bit, so a count the bytes cannot
    // hold means this is not a histogram.
    let entry_bits = (1 + count_bits + value_bits) as usize;
    if count.saturating_mul(entry_bits.max(1)) > bytes.len() * 8 {
        return Err(CodecError {
            offset: 0,
            message: format!(
                "a histogram of {count} entries cannot fit in {} bytes",
                bytes.len()
            ),
        });
    }
    let mut out = Histogram::default();
    for _ in 0..count {
        let escape = bits.unsigned(1) == 1;
        let occurrences = bits.unsigned(count_bits);
        // The value is stored as its distance above the minimum, so it
        // is never negative however the specification types the field.
        let value = bits.unsigned(value_bits) as i32;
        out.symbols.push(Symbol {
            escape,
            count: occurrences,
            cumulative: out.total,
            value: value.wrapping_add(minimum),
        });
        out.total += occurrences;
    }
    Ok((out, bits.consumed().div_ceil(8)))
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

    /// A count that must be possible for the bytes that remain.
    fn count_of_values(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 {
            return self.err(format!("negative count {n}"));
        }
        let remaining = self.data.len().saturating_sub(self.at);
        if n as usize * 4 > remaining {
            return self.err(format!(
                "{n} values need {} bytes, {remaining} remain",
                n as usize * 4
            ));
        }
        Ok(n as usize)
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
            0 | 1 | 3 => {
                let bits = self.i32()?;
                if bits < 0 {
                    return self.err(format!("negative code text length {bits}"));
                }
                let words = (bits as usize).div_ceil(32);
                let text = self.take(words * 4)?.to_vec();
                match codec {
                    0 => {
                        let mut reader = Bits::over_words(&text);
                        (0..count).map(|_| reader.signed(32)).collect()
                    }
                    1 => bitlength(&mut Bits::over_words(&text), count),
                    _ => {
                        // The histogram follows the code text, and the
                        // values held outside the coded stream follow
                        // that. There are none unless the histogram has
                        // an escape symbol to stand in for them, and
                        // nothing is written for them in that case.
                        let (hist, used) = histogram(&self.data[self.at..])?;
                        self.at += used;
                        let mut oob = Vec::new();
                        if hist.symbols.iter().any(|s| s.escape) {
                            let outside = self.count_of_values()?;
                            oob.reserve(outside.min(1 << 16));
                            for _ in 0..outside {
                                oob.push(self.i32()?);
                            }
                        }
                        // With nothing coded, every value is outside.
                        if bits == 0 {
                            oob
                        } else {
                            arithmetic(&mut Bits::over_words(&text), count, &hist, &oob)
                        }
                    }
                }
            }
            5 => {
                // A move-to-front packet holds no code text of its own:
                // it is two packets, the values as they were first seen
                // and the offsets that replay them.
                let values = self.packet(Predictor::None)?;
                let offsets = self.packet(Predictor::None)?;
                move_to_front(count, &values, &offsets)
            }
            other => {
                return self.err(format!(
                    "codec {other} is not implemented; the null, bitlength, and \
                     arithmetic codecs are"
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

/// Decode the arithmetic codec.
///
/// A value's share of the coding range is its frequency in the
/// histogram, so a common value costs less than a rare one. A value too
/// rare to be worth a share is written outside the coded stream and its
/// place taken by an escape symbol.
fn arithmetic(bits: &mut Bits<'_>, count: usize, hist: &Histogram, oob: &[i32]) -> Vec<i32> {
    let mut out = Vec::with_capacity(count.min(1 << 16));
    if hist.total == 0 {
        return out;
    }
    let scale = hist.total;
    let mut low: u16 = 0;
    let mut high: u16 = 0xffff;
    let mut code = bits.unsigned(16) as u16;
    let mut escapes = 0;

    for _ in 0..count {
        // Where the current code falls within the histogram's range.
        let range = (high as u32 - low as u32) + 1;
        let at = (((code as u32 - low as u32) + 1) * scale - 1) / range;
        let Some(symbol) = hist.at(at.min(scale.saturating_sub(1))) else {
            break;
        };
        if symbol.escape {
            match oob.get(escapes) {
                Some(v) => out.push(*v),
                None => break,
            }
            escapes += 1;
        } else {
            out.push(symbol.value);
        }

        // Narrow the range to the symbol's share, then shift out the
        // bits the two ends now agree on.
        let (lower, upper) = (symbol.cumulative, symbol.cumulative + symbol.count);
        let range = (high as u32 - low as u32) + 1;
        high = (low as u32).wrapping_add(range * upper / scale - 1) as u16;
        low = (low as u32).wrapping_add(range * lower / scale) as u16;
        loop {
            if (!(high ^ low)) >> 15 == 1 {
                // The ends agree on their top bit, so it is settled.
            } else if (low >> 14) == 1 && (high >> 14) == 2 {
                // The ends straddle the middle and are converging too
                // slowly to settle a bit; drop the second bit instead.
                code ^= 0x4000;
                low &= 0x3fff;
                high |= 0x4000;
            } else {
                break;
            }
            low <<= 1;
            high = (high << 1) | 1;
            code = (code << 1) | bits.unsigned(1) as u16;
        }
    }
    out
}

/// How many recently seen values the move-to-front window holds.
const WINDOW: usize = 16;

/// Replay a move-to-front stream.
///
/// Data that keeps returning to the same few values is cheaper to write
/// as positions in a window of what was seen lately than as the values
/// themselves. An offset outside the window means the value was not in
/// it and is taken from the values stream instead, which is what the
/// specification calls an escape.
fn move_to_front(count: usize, values: &[i32], offsets: &[i32]) -> Vec<i32> {
    let mut out = Vec::with_capacity(count.min(1 << 16));
    let mut window: Vec<i32> = Vec::with_capacity(WINDOW);
    let mut next_value = 0;
    for offset in offsets.iter().take(count) {
        let value = match usize::try_from(*offset) {
            Ok(at) if at < window.len() => {
                // Seen lately: name it by where it sits, and move it to
                // the front so the next mention is cheaper still.
                let value = window.remove(at);
                window.insert(0, value);
                value
            }
            _ => {
                let Some(value) = values.get(next_value) else {
                    break;
                };
                next_value += 1;
                window.insert(0, *value);
                window.truncate(WINDOW);
                *value
            }
        };
        out.push(value);
    }
    out
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
        let mut bits = Bits::over_words(&word);
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
        let mut bits = Bits::over_words(&bytes);
        assert_eq!(bits.unsigned(28), 0);
        // Four bits left in the first word, four taken from the second.
        assert_eq!(bits.unsigned(8), 0b1111_1111);
    }

    #[test]
    fn signed_fields_are_sign_extended() {
        let word = 0b1111_0111_0000_0000_0000_0000_0000_0000u32.to_le_bytes();
        let mut bits = Bits::over_words(&word);
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
    fn a_move_to_front_stream_replays_the_window() {
        // Values seen for the first time come from the values stream and
        // enter the window; an offset names one already in it and moves
        // it to the front.
        let values = [7, 9, 4];
        // escape, escape, "the one at 1" (7), escape, "the one at 2" (9)
        let offsets = [99, 99, 1, 99, 2];
        assert_eq!(
            move_to_front(5, &values, &offsets),
            [7, 9, 7, 4, 9],
            "window replay"
        );
        // Running out of values ends the run rather than inventing one.
        assert_eq!(move_to_front(4, &[1], &[99, 99]), [1]);
        // An offset into an empty window is an escape like any other.
        assert_eq!(move_to_front(1, &[5], &[0]), [5]);
        assert!(move_to_front(3, &[], &[99]).is_empty());
    }

    #[test]
    fn the_window_forgets_what_it_has_not_seen_lately() {
        // Seventeen new values, then the oldest is no longer nameable.
        let values: Vec<i32> = (0..17).collect();
        let offsets: Vec<i32> = vec![99; 17];
        let out = move_to_front(17, &values, &offsets);
        assert_eq!(out, values);
        // The window holds the last sixteen, most recent first, so the
        // furthest offset is the seventeenth value's predecessor.
        let mut probe = offsets.clone();
        probe.push(15);
        assert_eq!(move_to_front(18, &values, &probe).last(), Some(&1));
    }

    #[test]
    fn an_unimplemented_codec_is_reported_rather_than_guessed() {
        for codec in [4u8, 9] {
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

    /// Build a histogram's bytes: 16-bit entry count, 6-bit count width,
    /// 7-bit value width, 32-bit minimum, then the entries.
    fn histogram_bytes(
        count_bits: u32,
        value_bits: u32,
        minimum: i32,
        entries: &[(bool, u32, u32)],
    ) -> Vec<u8> {
        let mut bits: Vec<bool> = Vec::new();
        let mut put = |v: u32, n: u32| {
            for k in (0..n).rev() {
                bits.push((v >> k) & 1 == 1);
            }
        };
        put(entries.len() as u32, 16);
        put(count_bits, 6);
        put(value_bits, 7);
        put(minimum as u32, 32);
        for (escape, occurrences, value) in entries {
            put(u32::from(*escape), 1);
            put(*occurrences, count_bits);
            put(*value, value_bits);
        }
        let mut out = vec![0u8; bits.len().div_ceil(8)];
        for (i, bit) in bits.iter().enumerate() {
            if *bit {
                out[i / 8] |= 1 << (7 - i % 8);
            }
        }
        out
    }

    #[test]
    fn a_histogram_gives_each_value_its_share_of_the_range() {
        let bytes = histogram_bytes(8, 8, -5, &[(false, 3, 0), (true, 1, 0), (false, 6, 12)]);
        let (h, used) = histogram(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(h.total, 10);
        assert_eq!(h.symbols.len(), 3);
        // A value is stored as its distance above the minimum.
        assert_eq!(h.symbols[0].value, -5);
        assert_eq!(h.symbols[2].value, 7);
        assert!(h.symbols[1].escape);
        // Shares are laid end to end, and a point falls in exactly one.
        assert_eq!(h.symbols[0].cumulative, 0);
        assert_eq!(h.symbols[1].cumulative, 3);
        assert_eq!(h.symbols[2].cumulative, 4);
        assert_eq!(h.at(0).unwrap().value, -5);
        assert_eq!(h.at(2).unwrap().value, -5);
        assert!(h.at(3).unwrap().escape);
        assert_eq!(h.at(9).unwrap().value, 7);
    }

    #[test]
    fn a_histogram_too_large_for_its_bytes_is_refused() {
        let mut bytes = 0xffffu32.to_le_bytes().to_vec();
        bytes.extend([0u8; 8]);
        assert!(histogram(&bytes).is_err());
    }

    #[test]
    fn values_written_outside_the_coded_stream_are_used_as_they_stand() {
        // Nothing is coded, so every value is out of band and the
        // histogram is nothing but the escape symbol.
        let hist = histogram_bytes(8, 8, 0, &[(true, 1, 0)]);
        let mut bytes = 3i32.to_le_bytes().to_vec();
        bytes.push(3); // the arithmetic codec
        bytes.extend(0i32.to_le_bytes()); // no code text
        bytes.extend(&hist);
        bytes.extend(3i32.to_le_bytes()); // three values outside
        for v in [11i32, -22, 33] {
            bytes.extend(v.to_le_bytes());
        }
        let mut c = Cursor::new(&bytes);
        assert_eq!(c.packet(Predictor::None).unwrap(), [11, -22, 33]);
        assert_eq!(c.at, bytes.len());
    }

    #[test]
    fn a_histogram_without_an_escape_is_followed_by_nothing() {
        // With no escape symbol there are no out-of-band values, so the
        // packet ends at the histogram and the next one starts there.
        let hist = histogram_bytes(8, 8, 0, &[(false, 1, 7)]);
        let mut bytes = 2i32.to_le_bytes().to_vec();
        bytes.push(3);
        bytes.extend(0i32.to_le_bytes());
        bytes.extend(&hist);
        let end = bytes.len();
        bytes.extend(99i32.to_le_bytes()); // whatever follows
        let mut c = Cursor::new(&bytes);
        let _ = c.packet(Predictor::None).unwrap();
        assert_eq!(c.at, end, "the packet stops before what follows it");
    }

    #[test]
    fn the_width_of_a_value_is_what_it_needs() {
        assert_eq!(bit_width(0), 0);
        assert_eq!(bit_width(1), 1);
        assert_eq!(bit_width(2), 2);
        assert_eq!(bit_width(255), 8);
    }
}
