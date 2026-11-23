//! The ECMA-363 bit stream reader (clause 10 and the reference code in
//! Annex A of ECMA-363 4th edition): an adaptive arithmetic decoder with
//! static and dynamic (histogram) contexts. Uncompressed values are read
//! through the same decoder with the 8-bit static context, which is why a
//! block cannot be read as plain bytes once a compressed value was read.

/// Context for uncompressed 8-bit values.
pub const CONTEXT8: u32 = 0;
/// Contexts `>= STATIC_FULL` are static: `context - STATIC_FULL` equally
/// likely symbols.
pub const STATIC_FULL: u32 = 0x400;
/// Values written with a context `>= MAX_RANGE` are uncompressed.
pub const MAX_RANGE: u32 = STATIC_FULL + 0x3FFF;

const HALF_MASK: u32 = 0x8000;
const NOT_HALF_MASK: u32 = 0x7FFF;
const QUARTER_MASK: u32 = 0x4000;
const NOT_THREE_QUARTER_MASK: u32 = 0x3FFF;
const ELEPHANT: u16 = 0x1FFF;
const MAX_SYMBOL_IN_HISTOGRAM: u32 = 0xFFFF;
const ARRAY_SIZE_INCR: usize = 32;
const SWAP8: [u32; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
const FAST_NOT_MASK: [u32; 5] = [0xFFFF, 0x7FFF, 0x3FFF, 0x1FFF, 0x0FFF];
const READ_COUNT: [u32; 16] = [4, 3, 2, 2, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0];

fn swap_bits8(v: u32) -> u32 {
    (SWAP8[(v & 0xF) as usize] << 4) | SWAP8[((v >> 4) & 0xF) as usize]
}

#[derive(Default)]
struct Histogram {
    /// Occurrences of each symbol.
    count: Vec<u16>,
    /// Occurrences of each symbol and every larger symbol; `cum[0]` is the
    /// total.
    cum: Vec<u16>,
}

/// The dynamic histograms of one block (contexts are per block).
struct Contexts {
    hist: Vec<Option<Histogram>>,
}

impl Contexts {
    fn new() -> Self {
        Self {
            hist: (0..STATIC_FULL).map(|_| None).collect(),
        }
    }

    fn is_dynamic(context: u32) -> bool {
        context < STATIC_FULL && context != CONTEXT8
    }

    fn add_symbol(&mut self, context: u32, symbol: u32) {
        if !Self::is_dynamic(context) || symbol >= MAX_SYMBOL_IN_HISTOGRAM {
            return;
        }
        let s = symbol as usize;
        let h = self.hist[context as usize].get_or_insert_with(|| {
            let mut h = Histogram {
                count: vec![0; s + ARRAY_SIZE_INCR],
                cum: vec![0; s + ARRAY_SIZE_INCR],
            };
            h.count[0] = 1;
            h.cum[0] = 1;
            h
        });
        if h.cum.len() <= s {
            h.count.resize(s + ARRAY_SIZE_INCR, 0);
            h.cum.resize(s + ARRAY_SIZE_INCR, 0);
        }
        if h.cum[0] >= ELEPHANT {
            // Halve all counts to keep them in 16 bits; the escape symbol
            // keeps a count of at least 1.
            let mut acc: u16 = 0;
            for i in (0..h.cum.len()).rev() {
                h.count[i] >>= 1;
                acc = acc.wrapping_add(h.count[i]);
                h.cum[i] = acc;
            }
            h.count[0] += 1;
            h.cum[0] += 1;
        }
        h.count[s] += 1;
        for c in &mut h.cum[..=s] {
            *c += 1;
        }
    }

    fn symbol_frequency(&self, context: u32, symbol: u32) -> u32 {
        if !Self::is_dynamic(context) {
            return 1;
        }
        match &self.hist[context as usize] {
            Some(h) if (symbol as usize) < h.count.len() => h.count[symbol as usize] as u32,
            _ if symbol == 0 => 1,
            _ => 0,
        }
    }

    fn cumulative_frequency(&self, context: u32, symbol: u32) -> u32 {
        if !Self::is_dynamic(context) {
            return symbol.wrapping_sub(1);
        }
        match &self.hist[context as usize] {
            Some(h) if (symbol as usize) < h.cum.len() => {
                (h.cum[0] - h.cum[symbol as usize]) as u32
            }
            Some(h) => h.cum[0] as u32,
            None => 0,
        }
    }

    fn total_frequency(&self, context: u32) -> u32 {
        if Self::is_dynamic(context) {
            self.hist[context as usize]
                .as_ref()
                .map_or(1, |h| h.cum[0] as u32)
        } else if context == CONTEXT8 {
            256
        } else {
            context - STATIC_FULL
        }
    }

    fn symbol_from_frequency(&self, context: u32, freq: u32) -> u32 {
        if !Self::is_dynamic(context) {
            return freq + 1;
        }
        let Some(h) = &self.hist[context as usize] else {
            return 0;
        };
        if freq == 0 || (h.cum[0] as u32) < freq {
            return 0;
        }
        // Largest symbol whose cumulative frequency is <= freq (the
        // reference does a linear scan; cumulative frequency is monotone).
        let total = h.cum[0] as u32;
        let n = h.cum.len();
        let (mut lo, mut hi) = (0usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if total - h.cum[mid] as u32 <= freq {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.saturating_sub(1) as u32
    }
}

/// Reads one block's data section.
pub struct BitReader<'a> {
    data: &'a [u8],
    words: usize,
    position: usize,
    bit_offset: u32,
    local: u32,
    local_next: u32,
    high: u32,
    low: u32,
    underflow: u32,
    code: u32,
    contexts: Contexts,
    /// Set for files with the "no compression" profile bit: compressed
    /// values are stored as their uncompressed counterparts.
    no_compression: bool,
    overrun: bool,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8], no_compression: bool) -> Self {
        let mut r = Self {
            data,
            words: data.len().div_ceil(4),
            position: 0,
            bit_offset: 0,
            local: 0,
            local_next: 0,
            high: 0xFFFF,
            low: 0,
            underflow: 0,
            code: 0,
            contexts: Contexts::new(),
            no_compression,
            overrun: false,
        };
        r.get_local();
        r
    }

    /// True once a read ran past the end of the data (malformed block).
    pub fn overrun(&self) -> bool {
        self.overrun
    }

    fn word(&self, i: usize) -> u32 {
        let b = i * 4;
        let mut w = [0u8; 4];
        for (k, slot) in w.iter_mut().enumerate() {
            if let Some(v) = self.data.get(b + k) {
                *slot = *v;
            }
        }
        u32::from_le_bytes(w)
    }

    fn get_local(&mut self) {
        self.local = self.word(self.position);
        self.local_next = self.word(self.position + 1);
    }

    fn increment_position(&mut self) {
        self.position += 1;
        if self.position > self.words + 1 {
            self.overrun = true;
        }
        self.local = self.local_next;
        self.local_next = self.word(self.position + 1);
    }

    /// Bits consumed so far.
    pub fn bit_count(&self) -> u64 {
        ((self.position as u64) << 5) + self.bit_offset as u64
    }

    /// Byte offset of the next uncompressed read, valid while no
    /// compressed value has been read (used for the modifier chain header).
    pub fn byte_position(&self) -> usize {
        (self.bit_count() / 8) as usize
    }

    fn seek_to_bit(&mut self, position: u64) {
        self.position = (position >> 5) as usize;
        self.bit_offset = (position & 0x1F) as u32;
        self.get_local();
    }

    fn read_bit(&mut self) -> u32 {
        let v = (self.local >> self.bit_offset) & 1;
        self.bit_offset += 1;
        if self.bit_offset >= 32 {
            self.bit_offset -= 32;
            self.increment_position();
        }
        v
    }

    fn read_15_bits(&mut self) -> u32 {
        let mut v = self.local >> self.bit_offset;
        if self.bit_offset > 17 {
            v |= self.local_next << (32 - self.bit_offset);
        }
        v = v.wrapping_add(v);
        v = SWAP8[((v >> 12) & 0xF) as usize]
            | (SWAP8[((v >> 8) & 0xF) as usize] << 4)
            | (SWAP8[((v >> 4) & 0xF) as usize] << 8)
            | (SWAP8[(v & 0xF) as usize] << 12);
        self.bit_offset += 15;
        if self.bit_offset >= 32 {
            self.bit_offset -= 32;
            self.increment_position();
        }
        v
    }

    fn read_symbol(&mut self, context: u32) -> u32 {
        let position = self.bit_count();
        self.code = self.read_bit();
        self.bit_offset += self.underflow;
        while self.bit_offset >= 32 {
            self.bit_offset -= 32;
            self.increment_position();
        }
        let temp = self.read_15_bits();
        self.code = (self.code << 15) | temp;
        self.seek_to_bit(position);

        let total = self.contexts.total_frequency(context);
        let range = (self.high + 1).wrapping_sub(self.low) & 0x1FFFF;
        // Wider arithmetic than the reference so a malformed stream cannot
        // overflow; for valid data the results are identical.
        let offset = (1 + self.code as i64 - self.low as i64).max(0) as u64;
        let code_cum = ((total as u64 * offset).saturating_sub(1) / range.max(1) as u64) as u32;
        let value = self.contexts.symbol_from_frequency(context, code_cum);
        let value_cum = self.contexts.cumulative_frequency(context, value);
        let value_freq = self.contexts.symbol_frequency(context, value);

        let mut low = self.low;
        let mut high = (low as u64
            + (range as u64 * (value_cum as u64 + value_freq as u64)) / total.max(1) as u64)
            .saturating_sub(1) as u32;
        low = (low as u64 + (range as u64 * value_cum as u64) / total.max(1) as u64) as u32;
        self.contexts.add_symbol(context, value);

        let mut bit_count = READ_COUNT[(((low >> 12) ^ (high >> 12)) & 0xF) as usize];
        low &= FAST_NOT_MASK[bit_count as usize];
        high &= FAST_NOT_MASK[bit_count as usize];
        high <<= bit_count;
        low <<= bit_count;
        high |= (1 << bit_count) - 1;

        let mut masked_low = HALF_MASK & low;
        let mut masked_high = HALF_MASK & high;
        while (masked_low | masked_high) == 0
            || (masked_low == HALF_MASK && masked_high == HALF_MASK)
        {
            low = (NOT_HALF_MASK & low) << 1;
            high = ((NOT_HALF_MASK & high) << 1) | 1;
            masked_low = HALF_MASK & low;
            masked_high = HALF_MASK & high;
            bit_count += 1;
            if bit_count > 64 {
                // Degenerate state from a malformed stream.
                self.overrun = true;
                break;
            }
        }
        let saved_low = masked_low;
        let saved_high = masked_high;
        if bit_count > 0 {
            bit_count += self.underflow;
            self.underflow = 0;
        }

        masked_low = QUARTER_MASK & low;
        masked_high = QUARTER_MASK & high;
        let mut underflow = 0;
        while masked_low == 0x4000 && masked_high == 0 {
            low &= NOT_THREE_QUARTER_MASK;
            high &= NOT_THREE_QUARTER_MASK;
            low += low;
            high += high;
            high |= 1;
            masked_low = QUARTER_MASK & low;
            masked_high = QUARTER_MASK & high;
            underflow += 1;
        }
        self.underflow += underflow;
        self.low = low | saved_low;
        self.high = high | saved_high;

        self.bit_offset += bit_count;
        while self.bit_offset >= 32 {
            self.bit_offset -= 32;
            self.increment_position();
        }
        value
    }

    pub fn u8(&mut self) -> u8 {
        let v = self.read_symbol(CONTEXT8).wrapping_sub(1);
        swap_bits8(v & 0xFF) as u8
    }

    pub fn u16(&mut self) -> u16 {
        let lo = self.u8() as u16;
        let hi = self.u8() as u16;
        lo | (hi << 8)
    }

    pub fn u32(&mut self) -> u32 {
        let lo = self.u16() as u32;
        let hi = self.u16() as u32;
        lo | (hi << 16)
    }

    pub fn u64(&mut self) -> u64 {
        let lo = self.u32() as u64;
        let hi = self.u32() as u64;
        lo | (hi << 32)
    }

    pub fn i32(&mut self) -> i32 {
        self.u32() as i32
    }

    pub fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }

    pub fn f64(&mut self) -> f64 {
        f64::from_bits(self.u64())
    }

    /// U16 byte count followed by UTF-8 bytes.
    pub fn string(&mut self) -> String {
        let n = self.u16() as usize;
        let mut b = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            if self.overrun {
                break;
            }
            b.push(self.u8());
        }
        String::from_utf8_lossy(&b).into_owned()
    }

    pub fn compressed_u32(&mut self, context: u32) -> u32 {
        if self.no_compression || context == CONTEXT8 || context >= MAX_RANGE {
            return self.u32();
        }
        let symbol = self.read_symbol(context);
        if symbol != 0 {
            symbol - 1
        } else {
            let v = self.u32();
            self.contexts.add_symbol(context, v.wrapping_add(1));
            v
        }
    }

    pub fn compressed_u16(&mut self, context: u32) -> u16 {
        if self.no_compression || context == CONTEXT8 || context >= MAX_RANGE {
            return self.u16();
        }
        let symbol = self.read_symbol(context);
        if symbol != 0 {
            (symbol - 1) as u16
        } else {
            let v = self.u16();
            self.contexts.add_symbol(context, v as u32 + 1);
            v
        }
    }

    pub fn compressed_u8(&mut self, context: u32) -> u8 {
        if self.no_compression || context == CONTEXT8 || context >= MAX_RANGE {
            return self.u8();
        }
        let symbol = self.read_symbol(context);
        if symbol != 0 {
            (symbol - 1) as u8
        } else {
            let v = self.u8();
            self.contexts.add_symbol(context, v as u32 + 1);
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncompressed_values_are_plain_little_endian() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        data.extend_from_slice(&3u16.to_le_bytes());
        data.extend_from_slice(b"abc");
        data.extend_from_slice(&1.5f32.to_le_bytes());
        let mut r = BitReader::new(&data, false);
        assert_eq!(r.u32(), 0x1234_5678);
        assert_eq!(r.string(), "abc");
        assert_eq!(r.f32(), 1.5);
        assert!(!r.overrun());
    }
}
