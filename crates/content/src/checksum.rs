//! Steam's chunk checksum: Adler-32 with a seed of 0 instead of 1.
//!
//! Source: SteamKit2 `Steam/CDN/DepotChunk.cs` (`AdlerHash` =
//! `Adler32.Calculate(0, input)`); steamroom `src/util/checksum.rs`
//! (`SteamAdler32`, "non-standard Adler32 with zero seed").

const MOD: u32 = 65_521;
/// Largest number of bytes that can be summed before `b` could overflow `u32`.
const BLOCK: usize = 5_552;

/// Computes the zero-seeded Adler-32 checksum of `data`.
pub fn steam_adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (0u32, 0u32);
    for block in data.chunks(BLOCK) {
        for &byte in block {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::steam_adler32;

    /// Reference: the textbook byte-by-byte definition with seed 0.
    fn reference(data: &[u8]) -> u32 {
        let (mut a, mut b) = (0u64, 0u64);
        for &byte in data {
            a = (a + u64::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        u32::try_from((b << 16) | a).unwrap()
    }

    #[test]
    fn matches_the_definition() {
        assert_eq!(steam_adler32(b""), 0);
        // Standard Adler-32 of "abc" is 0x024d0127; with seed 0 a is one
        // lower (0x0126) and b is three lower (0x024a).
        assert_eq!(steam_adler32(b"abc"), 0x024a_0126);
        let large: Vec<u8> = (0..100_000u32).map(|i| (i * 31 % 251) as u8).collect();
        assert_eq!(steam_adler32(&large), reference(&large));
        let all_ff = vec![0xff; 20_000];
        assert_eq!(steam_adler32(&all_ff), reference(&all_ff));
    }
}
