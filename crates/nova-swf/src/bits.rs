//! MSB-first bit writer used by the SWF shape and matrix encoders.

/// Writes bits most-significant-bit first, as required by the SWF format.
#[derive(Debug, Default, Clone)]
pub struct BitWriter {
    bytes: Vec<u8>,
    current: u8,
    bit_count: u8,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write_bit(&mut self, bit: bool) {
        self.current = (self.current << 1) | u8::from(bit);
        self.bit_count += 1;
        if self.bit_count == 8 {
            self.bytes.push(self.current);
            self.current = 0;
            self.bit_count = 0;
        }
    }

    /// Writes the low `count` bits of `value`, MSB first.
    ///
    /// `count` must be in `0..=32`.
    pub fn write_bits(&mut self, value: u32, count: u8) {
        debug_assert!(count <= 32);
        if count == 0 {
            return;
        }
        for i in (0..count).rev() {
            self.write_bit((value >> i) & 1 == 1);
        }
    }

    /// Writes a two's-complement signed value in `count` bits.
    pub fn write_sbits(&mut self, value: i32, count: u8) {
        debug_assert!((1..=32).contains(&count));
        let mask = if count == 32 {
            u32::MAX
        } else {
            (1u32 << count) - 1
        };
        self.write_bits((value as u32) & mask, count);
    }

    pub fn is_aligned(&self) -> bool {
        self.bit_count == 0
    }

    /// Pads with zero bits up to the next byte boundary.
    pub fn align(&mut self) {
        if self.bit_count != 0 {
            let pad = 8 - self.bit_count;
            self.write_bits(0, pad);
        }
    }

    /// Appends raw bytes. The stream must be byte-aligned.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        debug_assert!(self.is_aligned());
        self.align();
        self.bytes.extend_from_slice(bytes);
    }

    /// Appends another bit writer's content, preserving bit alignment.
    pub fn append(&mut self, other: &BitWriter) {
        for byte in &other.bytes {
            self.write_bits(*byte as u32, 8);
        }
        if other.bit_count > 0 {
            self.write_bits(
                other.current as u32 >> (8 - other.bit_count),
                other.bit_count,
            );
        }
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.len() + usize::from(self.bit_count > 0)
    }

    pub fn into_bytes(mut self) -> Vec<u8> {
        self.align();
        self.bytes
    }
}

/// Smallest number of signed bits needed to store `value`.
pub fn signed_bits(value: i32) -> u8 {
    let magnitude = if value >= 0 {
        value as u32
    } else {
        !(value as u32)
    };
    let bits = 32 - magnitude.leading_zeros() + 1;
    bits.clamp(1, 32) as u8
}

/// Smallest number of unsigned bits needed to store `value`.
pub fn unsigned_bits(value: u32) -> u8 {
    if value == 0 {
        1
    } else {
        (32 - value.leading_zeros()) as u8
    }
}

/// Number of bits needed to store a zero-based index `< count`.
pub fn index_bits(count: usize) -> u8 {
    if count <= 1 {
        1
    } else {
        unsigned_bits((count - 1) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_msb_first() {
        let mut w = BitWriter::new();
        w.write_bits(0b101, 3);
        w.write_bits(0b11, 2);
        w.align();
        assert_eq!(w.into_bytes(), vec![0b1011_1000]);
    }

    #[test]
    fn signed_widths() {
        assert_eq!(signed_bits(0), 1);
        assert_eq!(signed_bits(1), 2);
        assert_eq!(signed_bits(-1), 1);
        assert_eq!(signed_bits(2), 3);
        assert_eq!(signed_bits(-2), 2);
        assert_eq!(signed_bits(16383), 15);
        assert_eq!(signed_bits(16384), 16);
    }

    #[test]
    fn negative_roundtrip() {
        let mut w = BitWriter::new();
        w.write_sbits(-3, 4);
        w.write_sbits(5, 4);
        assert_eq!(w.into_bytes(), vec![0b1101_0101]);
    }

    #[test]
    fn append_preserves_partial_bits() {
        let mut a = BitWriter::new();
        a.write_bit(true);
        let mut b = BitWriter::new();
        b.write_bits(0b111, 3);
        b.align();
        a.append(&b);
        a.align();
        assert_eq!(a.into_bytes(), vec![0b1111_0000, 0x00]);
    }
}
