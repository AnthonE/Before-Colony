//! LSB-first bit packing over caller buffers.
//!
//! Overflowing writes and reads do not panic. The writer stops and records the overflow; the reader
//! returns zeros and records it. Callers check [`BitWriter::overflowed`] or [`BitReader::overflowed`]
//! once at the end, which keeps the hot encode path free of per-field error plumbing.

/// Writes bit fields into a borrowed byte buffer.
pub struct BitWriter<'a> {
    buf: &'a mut [u8],
    bit_pos: usize,
    limit_bits: usize,
    overflow: bool,
}

impl<'a> BitWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        let limit_bits = buf.len() * 8;
        Self { buf, bit_pos: 0, limit_bits, overflow: false }
    }

    /// Like [`new`](Self::new) but never writes past `max_bytes`.
    pub fn with_limit(buf: &'a mut [u8], max_bytes: usize) -> Self {
        let limit_bits = buf.len().min(max_bytes) * 8;
        Self { buf, bit_pos: 0, limit_bits, overflow: false }
    }

    /// Writes the low `bits` bits of `value` (`bits` ≤ 32).
    #[inline]
    pub fn write_bits(&mut self, value: u32, bits: u32) {
        debug_assert!(bits <= 32);
        if bits == 0 {
            return;
        }
        if self.overflow || self.bit_pos + bits as usize > self.limit_bits {
            self.overflow = true;
            return;
        }
        let mut v = u64::from(value) & ((1u64 << bits) - 1);
        let mut remaining = bits;
        while remaining > 0 {
            let byte = self.bit_pos >> 3;
            let offset = (self.bit_pos & 7) as u32;
            let take = remaining.min(8 - offset);
            let chunk = (v & ((1u64 << take) - 1)) as u8;
            if offset == 0 {
                self.buf[byte] = 0; // first touch of this byte: no need to pre-zero the buffer
            }
            self.buf[byte] |= chunk << offset;
            v >>= take;
            remaining -= take;
            self.bit_pos += take as usize;
        }
    }

    #[inline]
    pub fn write_bool(&mut self, b: bool) {
        self.write_bits(u32::from(b), 1);
    }

    #[inline]
    pub fn write_u8(&mut self, v: u8) {
        self.write_bits(u32::from(v), 8);
    }

    #[inline]
    pub fn write_u16(&mut self, v: u16) {
        self.write_bits(u32::from(v), 16);
    }

    #[inline]
    pub fn write_u32(&mut self, v: u32) {
        self.write_bits(v, 32);
    }

    /// Raw IEEE-754 bits (lossless).
    #[inline]
    pub fn write_f32(&mut self, v: f32) {
        self.write_bits(v.to_bits(), 32);
    }

    /// Signed value in two's complement over `bits` bits.
    #[inline]
    pub fn write_i32(&mut self, v: i32, bits: u32) {
        self.write_bits(v as u32, bits);
    }

    pub fn bits_written(&self) -> usize {
        self.bit_pos
    }

    pub fn bits_remaining(&self) -> usize {
        self.limit_bits.saturating_sub(self.bit_pos)
    }

    /// Bytes touched so far (rounded up).
    pub fn bytes_written(&self) -> usize {
        self.bit_pos.div_ceil(8)
    }

    pub fn overflowed(&self) -> bool {
        self.overflow
    }

    /// Rewinds to an earlier position, e.g. to drop a partially written item.
    pub fn rewind(&mut self, bit_pos: usize) {
        debug_assert!(bit_pos <= self.bit_pos);
        self.bit_pos = bit_pos;
        // The byte at the new position may carry stale high bits; clear them so later writes
        // (which OR into partially filled bytes) stay correct.
        let offset = (bit_pos & 7) as u32;
        if offset != 0 {
            let byte = bit_pos >> 3;
            self.buf[byte] &= ((1u16 << offset) - 1) as u8;
        }
    }
}

/// Reads bit fields from a borrowed byte slice.
pub struct BitReader<'a> {
    buf: &'a [u8],
    bit_pos: usize,
    overflow: bool,
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, bit_pos: 0, overflow: false }
    }

    /// Reads `bits` bits (≤ 32). Past the end it returns 0 and marks the reader overflowed.
    #[inline]
    pub fn read_bits(&mut self, bits: u32) -> u32 {
        debug_assert!(bits <= 32);
        if bits == 0 {
            return 0;
        }
        if self.overflow || self.bit_pos + bits as usize > self.buf.len() * 8 {
            self.overflow = true;
            return 0;
        }
        let mut out = 0u64;
        let mut got = 0u32;
        while got < bits {
            let byte = self.buf[self.bit_pos >> 3];
            let offset = (self.bit_pos & 7) as u32;
            let take = (bits - got).min(8 - offset);
            let chunk = (u64::from(byte) >> offset) & ((1u64 << take) - 1);
            out |= chunk << got;
            got += take;
            self.bit_pos += take as usize;
        }
        out as u32
    }

    #[inline]
    pub fn read_bool(&mut self) -> bool {
        self.read_bits(1) != 0
    }

    #[inline]
    pub fn read_u8(&mut self) -> u8 {
        self.read_bits(8) as u8
    }

    #[inline]
    pub fn read_u16(&mut self) -> u16 {
        self.read_bits(16) as u16
    }

    #[inline]
    pub fn read_u32(&mut self) -> u32 {
        self.read_bits(32)
    }

    #[inline]
    pub fn read_f32(&mut self) -> f32 {
        f32::from_bits(self.read_bits(32))
    }

    /// Two's-complement signed value over `bits` bits.
    #[inline]
    pub fn read_i32(&mut self, bits: u32) -> i32 {
        let raw = self.read_bits(bits);
        if bits == 32 {
            return raw as i32;
        }
        let shift = 32 - bits;
        ((raw << shift) as i32) >> shift
    }

    pub fn bits_read(&self) -> usize {
        self.bit_pos
    }

    pub fn bits_remaining(&self) -> usize {
        (self.buf.len() * 8).saturating_sub(self.bit_pos)
    }

    pub fn overflowed(&self) -> bool {
        self.overflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_mixed_widths() {
        let mut buf = [0xAAu8; 64];
        let mut w = BitWriter::new(&mut buf);
        w.write_bits(5, 3);
        w.write_bits(0xdead_beef, 32);
        w.write_bool(true);
        w.write_i32(-5, 7);
        w.write_f32(-1.25);
        w.write_bits(0x3ff, 10);
        assert!(!w.overflowed());
        let n = w.bytes_written();
        let mut r = BitReader::new(&buf[..n]);
        assert_eq!(r.read_bits(3), 5);
        assert_eq!(r.read_bits(32), 0xdead_beef);
        assert!(r.read_bool());
        assert_eq!(r.read_i32(7), -5);
        assert_eq!(r.read_f32(), -1.25);
        assert_eq!(r.read_bits(10), 0x3ff);
        assert!(!r.overflowed());
    }

    #[test]
    fn overflow_is_sticky_and_safe() {
        let mut buf = [0u8; 2];
        let mut w = BitWriter::new(&mut buf);
        w.write_bits(0xffff, 16);
        w.write_bits(1, 1);
        assert!(w.overflowed());
        let mut r = BitReader::new(&buf[..1]);
        assert_eq!(r.read_bits(16), 0);
        assert!(r.overflowed());
    }

    #[test]
    fn rewind_clears_partial_byte() {
        let mut buf = [0u8; 4];
        let mut w = BitWriter::new(&mut buf);
        w.write_bits(0b101, 3);
        let mark = w.bits_written();
        w.write_bits(0b11111, 5);
        w.rewind(mark);
        w.write_bits(0b00000, 5);
        w.write_bits(0xff, 8);
        let mut r = BitReader::new(&buf);
        assert_eq!(r.read_bits(3), 0b101);
        assert_eq!(r.read_bits(5), 0);
        assert_eq!(r.read_bits(8), 0xff);
    }
}
