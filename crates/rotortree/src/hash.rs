pub type Hash = [u8; 32];

/// Streaming hash state
pub trait HashState {
    /// Feed bytes into the state
    fn update(&mut self, data: &[u8]);

    /// Finalize and return the digest
    fn finalize(self) -> Hash;
}

/// Raw hash primitive supporting streaming
pub trait Hasher: Clone + Send + Sync + 'static {
    type State: HashState;

    /// Create a fresh hashing state
    fn new_state(&self) -> Self::State;

    /// Hash the concatenation of child node hashes into their parent.
    ///
    /// Children are hashed verbatim, with no domain tag and no length
    /// prefix, so an internal node is `hash(child_0 || child_1 || ...)`.
    /// This matches the canonical Lean IMT node hash and keeps the
    /// digest compatible with other implementations.
    #[inline]
    fn hash_children(&self, children: &[Hash]) -> Hash {
        let mut sponge = self.new_state();
        sponge.update(children.as_flattened());
        sponge.finalize()
    }

    /// Hash a batch of independent parent groups into `out`.
    ///
    /// Element `i` of `out` receives `hash_children(groups[i])`. The
    /// default implementation is a scalar loop; hashers backed by a
    /// multi-input SIMD kernel (e.g. BLAKE3) override this to hash several
    /// groups at once while staying byte-identical to `hash_children`.
    ///
    /// `out.len()` must be at least `groups.len()`.
    #[inline]
    fn hash_many_into(&self, groups: &[&[Hash]], out: &mut [Hash]) {
        for (g, o) in groups.iter().zip(out.iter_mut()) {
            *o = self.hash_children(g);
        }
    }
}
