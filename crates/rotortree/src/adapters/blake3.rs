use blake3::{
    IncrementCounter,
    platform::{
        MAX_SIMD_DEGREE,
        Platform,
    },
};

use crate::{
    Hash,
    HashState,
    Hasher,
};

// BLAKE3 spec constants, hardcoded because blake3's internal `IV` and flag
// consts are private. These pin `hash_many` to the exact configuration that
// makes a single-chunk multi-input hash byte-identical to `blake3::hash`:
//   key = IV, counter = 0, increment_counter = No, flags = 0,
//   flags_start = CHUNK_START, flags_end = CHUNK_END | ROOT.
// Identity holds ONLY when each input is a whole number of 64-byte blocks and
// is <= 1024 bytes (a single chunk). It diverges past 1024 bytes (multi-chunk),
// so the batch path below is restricted to arities whose byte length satisfies
// `len % 64 == 0 && len <= 1024`. See `Cargo.toml` (blake3 = "=1.8.5").
const B3_IV: &[u32; 8] = &[
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB,
    0x5BE0CD19,
];
const CHUNK_START: u8 = 1; // 1 << 0
const CHUNK_END: u8 = 2; // 1 << 1
const ROOT: u8 = 8; // 1 << 3
const FLAGS_END: u8 = CHUNK_END | ROOT; // 10

#[derive(Debug, Clone, Copy, Default)]
pub struct Blake3Hasher;

impl Blake3Hasher {
    pub fn new() -> Self {
        Self
    }
}

/// The detected BLAKE3 platform (SIMD level). `detect()` is cheap (a couple of
/// cfg/cpuid branches) but std lets us cache it for free.
#[cfg(feature = "std")]
#[inline]
fn platform() -> Platform {
    static PLATFORM: std::sync::OnceLock<Platform> = std::sync::OnceLock::new();
    *PLATFORM.get_or_init(Platform::detect)
}

#[cfg(not(feature = "std"))]
#[inline]
fn platform() -> Platform {
    Platform::detect()
}

/// Hash up to `MAX_SIMD_DEGREE` independent single-chunk inputs of exactly
/// `LEN` bytes each through the multi-input SIMD kernel.
///
/// `LEN` MUST satisfy `LEN % 64 == 0 && LEN <= 1024`; otherwise the result is
/// NOT byte-identical to `blake3::hash` (silent corruption on the NEON path).
/// `inputs.len() <= MAX_SIMD_DEGREE` and `out.len() >= inputs.len()`.
#[inline]
fn batch_root<const LEN: usize>(
    plat: &Platform,
    inputs: &[&[u8; LEN]],
    out: &mut [Hash],
) {
    debug_assert!(
        LEN.is_multiple_of(64) && LEN <= 1024,
        "ineligible input length"
    );
    debug_assert!(inputs.len() <= MAX_SIMD_DEGREE);
    debug_assert!(out.len() >= inputs.len());
    let flat: &mut [u8] = out[..inputs.len()].as_flattened_mut();
    plat.hash_many::<LEN>(
        inputs,
        B3_IV,
        0,
        IncrementCounter::No,
        0,
        CHUNK_START,
        FLAGS_END,
        flat,
    );
}

/// Reslice an eligible same-arity batch and dispatch it through `batch_root`.
/// Each group's bytes are contiguous (full groups at these arities are
/// chunk-aligned since `128 % N == 0`), so the checked `try_from` always
/// succeeds. Returns `false` (untouched `out`) for any ineligible arity so the
/// caller can take the scalar path.
fn batch_eligible(
    plat: &Platform,
    arity: usize,
    groups: &[&[Hash]],
    out: &mut [Hash],
) -> bool {
    macro_rules! dispatch {
        ($len:literal) => {{
            // Stack-collect &[u8; LEN] refs (at most MAX_SIMD_DEGREE).
            let mut refs: [&[u8; $len]; MAX_SIMD_DEGREE] =
                [&[0u8; $len]; MAX_SIMD_DEGREE];
            for (slot, g) in refs.iter_mut().zip(groups.iter()) {
                match <&[u8; $len]>::try_from(g.as_flattened()) {
                    Ok(r) => *slot = r,
                    Err(_) => return false, // non-full / discontiguous group
                }
            }
            batch_root::<$len>(plat, &refs[..groups.len()], out);
            true
        }};
    }
    match arity {
        2 => dispatch!(64),
        4 => dispatch!(128),
        8 => dispatch!(256),
        16 => dispatch!(512),
        _ => false,
    }
}

impl HashState for blake3::Hasher {
    #[inline]
    fn update(&mut self, data: &[u8]) {
        blake3::Hasher::update(self, data);
    }

    #[inline]
    fn finalize(self) -> Hash {
        *blake3::Hasher::finalize(&self).as_bytes()
    }
}

impl Hasher for Blake3Hasher {
    type State = blake3::Hasher;

    #[inline]
    fn new_state(&self) -> Self::State {
        blake3::Hasher::new()
    }

    /// One-shot scalar parent hash: `blake3::hash(child_0 || child_1 || ...)`.
    ///
    /// Byte-identical to the streaming default but skips the incremental
    /// `Hasher` state. This is the fallback for every group the batched
    /// `hash_many_into` path declines.
    #[inline]
    fn hash_children(&self, children: &[Hash]) -> Hash {
        *blake3::hash(children.as_flattened()).as_bytes()
    }

    /// Batched parent hashing through BLAKE3's multi-input SIMD kernel.
    ///
    /// Eligible only when every group has the same arity N in {2,4,8,16}
    /// (byte length in {64,128,256,512}, all `% 64 == 0` and `<= 1024`), where
    /// the multi-input hash is byte-identical to `hash_children`. Such groups
    /// are processed `MAX_SIMD_DEGREE` at a time. Any other shape (mixed
    /// arity, odd/lift/partial group, arity > 16, or a discontiguous slice)
    /// falls back to the scalar `hash_children` per group.
    fn hash_many_into(&self, groups: &[&[Hash]], out: &mut [Hash]) {
        debug_assert!(out.len() >= groups.len());

        // Eligible iff all groups share one arity in {2,4,8,16}.
        let arity = groups.first().map_or(0, |g| g.len());
        let uniform_eligible =
            matches!(arity, 2 | 4 | 8 | 16) && groups.iter().all(|g| g.len() == arity);

        if !uniform_eligible {
            for (g, o) in groups.iter().zip(out.iter_mut()) {
                *o = self.hash_children(g);
            }
            return;
        }

        let plat = platform();
        let degree = plat.simd_degree().clamp(1, MAX_SIMD_DEGREE);
        let mut g = groups;
        let mut o = &mut out[..groups.len()];
        while !g.is_empty() {
            let take = g.len().min(degree);
            let (g_now, g_rest) = g.split_at(take);
            let (o_now, o_rest) = o.split_at_mut(take);
            // try_from inside batch_eligible re-checks contiguity; a false
            // return means a group was not a full contiguous slice, so hash it
            // scalar. Full groups at these arities are always contiguous.
            if !batch_eligible(&plat, arity, g_now, o_now) {
                for (gg, oo) in g_now.iter().zip(o_now.iter_mut()) {
                    *oo = self.hash_children(gg);
                }
            }
            g = g_rest;
            o = o_rest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each batched group hash must be byte-identical to the scalar
    /// `hash_children` for the eligible arities {2,4,8,16}.
    #[test]
    fn batch_root_byte_identical_to_hash_children() {
        let h = Blake3Hasher;

        fn make_groups(arity: usize) -> [[Hash; 16]; 4] {
            let mut groups = [[[0u8; 32]; 16]; 4];
            for (k, group) in groups.iter_mut().enumerate() {
                for (j, child) in group.iter_mut().enumerate().take(arity) {
                    for (b, byte) in child.iter_mut().enumerate() {
                        *byte = (k as u8)
                            .wrapping_mul(31)
                            .wrapping_add(j as u8)
                            .wrapping_add(b as u8)
                            ^ 0xA5;
                    }
                }
            }
            groups
        }

        for arity in [2usize, 4, 8, 16] {
            let groups = make_groups(arity);
            let refs: [&[Hash]; 4] = [
                &groups[0][..arity],
                &groups[1][..arity],
                &groups[2][..arity],
                &groups[3][..arity],
            ];
            let mut out = [[0u8; 32]; 4];
            h.hash_many_into(&refs, &mut out);
            for (i, g) in refs.iter().enumerate() {
                assert_eq!(
                    out[i],
                    h.hash_children(g),
                    "arity {arity} input {i} batched != scalar",
                );
            }
        }
    }
}
