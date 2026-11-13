//! MD5, which object stores use for ETags. Written out here (RFC 1321)
//! rather than taken as a dependency: it's small, and it's used only to
//! name content, never to protect anything.

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// The constants: the integer part of `abs(sin(i + 1)) * 2^32`.
fn k(i: usize) -> u32 {
    (((i + 1) as f64).sin().abs() * 4_294_967_296.0) as u32
}

/// A hash being built, fed in pieces.
pub struct Md5 {
    state: [u32; 4],
    /// Bytes not yet making a whole block.
    pending: Vec<u8>,
    length: u64,
    k: [u32; 64],
}

impl Default for Md5 {
    fn default() -> Self {
        Self {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            pending: Vec::with_capacity(64),
            length: 0,
            k: std::array::from_fn(k),
        }
    }
}

impl Md5 {
    pub fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        if !self.pending.is_empty() {
            let take = (64 - self.pending.len()).min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.pending.len() < 64 {
                return;
            }
            let block: [u8; 64] = self.pending[..].try_into().expect("64 bytes");
            self.block(&block);
            self.pending.clear();
        }
        let mut blocks = data.chunks_exact(64);
        for block in &mut blocks {
            self.block(block.try_into().expect("64 bytes"));
        }
        self.pending.extend_from_slice(blocks.remainder());
    }

    fn block(&mut self, block: &[u8; 64]) {
        let m: [u32; 16] = std::array::from_fn(|i| {
            u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().expect("4 bytes"))
        });
        let [mut a, mut b, mut c, mut d] = self.state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a
                .wrapping_add(f)
                .wrapping_add(self.k[i])
                .wrapping_add(m[g])
                .rotate_left(S[i]);
            (a, d, c, b) = (d, c, b, b.wrapping_add(rotated));
        }
        for (state, add) in self.state.iter_mut().zip([a, b, c, d]) {
            *state = state.wrapping_add(add);
        }
    }

    /// The 16 bytes of the hash.
    pub fn finish(mut self) -> [u8; 16] {
        let bits = self.length.wrapping_mul(8);
        let mut tail = std::mem::take(&mut self.pending);
        tail.push(0x80);
        while tail.len() % 64 != 56 {
            tail.push(0);
        }
        tail.extend_from_slice(&bits.to_le_bytes());
        for block in tail.chunks_exact(64) {
            self.block(block.try_into().expect("64 bytes"));
        }
        let mut out = [0u8; 16];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }
}

/// Bytes as lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The MD5 of `data`, in hex.
pub fn md5_hex(data: &[u8]) -> String {
    let mut hash = Md5::default();
    hash.update(data);
    hex(&hash.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_rfc() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(b"message digest"),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
        assert_eq!(
            md5_hex(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            ),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn is_the_same_fed_in_pieces() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7 % 251) as u8).collect();
        let whole = md5_hex(&data);
        for size in [1, 3, 63, 64, 65, 200] {
            let mut hash = Md5::default();
            for piece in data.chunks(size) {
                hash.update(piece);
            }
            assert_eq!(hex(&hash.finish()), whole, "pieces of {size}");
        }
    }
}
