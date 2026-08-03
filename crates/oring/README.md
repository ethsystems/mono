# oring

A generic sealed-note envelope: seal a note to a recipient public key, open it with the recipient secret key, and batch trial-decrypt a mixed inbox for wallet scanning.

<!-- ANCHOR: intro -->

`oring` (an O-ring is a mechanical seal) packages note-encryption architecture that shielded-pool and stealth-address PoCs keep reimplementing: secp256k1 or X25519 ECDH, HKDF-SHA256, ChaCha20-Poly1305. 

<!-- ANCHOR_END: intro -->

The crate owns one fixed, hardened protocol flow (suite v1: HKDF-SHA256 plus ChaCha20-Poly1305). Consumers supply genericity through two small traits, `Kem` for curve choice and `Domain` for the note codec and tag. This approach makes tradeoffs specific to its callers and is not intended for production use.

<!-- ANCHOR: design -->

## Design decisions

- suite v1 is fixed: consumers cannot swap the KDF or AEAD. pluggability stops at `Kem` and `Domain`.
- the recipient public key (`pk_r`) is mixed into the KDF info. Without it, a low-order or attacker-crafted ephemeral key would make every recipient derive the same key and accept the same envelope; binding `pk_r` keeps the derived key recipient-specific even under adversarial input.
- the AEAD nonce is derived from the KDF. It binds the nonce to the same transcript as the key, so a key-schedule edit that changes one changes both, and an implementer never picks a nonce by hand. It buys nothing against an RNG replay: a replayed ephemeral key reproduces the shared secret, hence the key and the nonce alike. Callers who need to survive a VM snapshot or fork must rotate the recipient key or bind a counter into the AAD.
- every envelope carries a key-commitment tag (`commit`, CTX-style). ChaCha20-Poly1305 is not key-committing on its own, so one crafted ciphertext could otherwise open validly under two different recipients' keys to two different plaintexts, and a trial-decryption scanner would accept it automatically. `open` recomputes `commit` and compares it in constant time before the AEAD ever runs.
- anonymous sender by design: there is no sender authentication anywhere in the envelope, and none is planned.
- adapter correctness is enforced by a conformance test suite (`test-helpers` feature), not by trait bounds: garbage byte strings, low-order points, the identity point, and all-zero Diffie-Hellman outputs must all fail to decapsulate. Third-party `Kem` implementations are expected to run it.
- secret hygiene is not decorative: `SecretKey` and `SharedSecret` carry no `Debug`, `Clone`, or derived `PartialEq`; commit comparison and other secret-adjacent equality checks go through `subtle`; every `SharedSecret` is zeroized once consumed, including scanner scratch buffers, derived key material, and batch out-slots. One residue is upstream: `hkdf` 0.13 keeps the extracted PRK inside an HMAC state that has no `Zeroize` impl, so that copy lives until the allocation is reused.

<!-- ANCHOR_END: design -->

<!-- ANCHOR: usage -->

## Usage

```rust,ignore
use std::convert::Infallible;

use oring::{Domain, open, seal};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use x25519_dalek::{PublicKey, StaticSecret};

struct WalletDomain;

impl Domain for WalletDomain {
    type Note = Vec<u8>;
    // this codec accepts every note; a domain with a real format names its
    // own rejection type here.
    type Error = Infallible;
    const DOMAIN_TAG: &'static str = "oring-example/v1";

    fn encode_note(note: &Self::Note, out: &mut Vec<u8>) -> Result<(), Self::Error> {
        out.extend_from_slice(note);
        Ok(())
    }

    fn decode_note(bytes: &[u8]) -> Result<Self::Note, Self::Error> {
        Ok(bytes.to_vec())
    }
}

let mut rng = ChaCha20Rng::seed_from_u64(42);
let sk = StaticSecret::random_from_rng(&mut rng);
let pk = PublicKey::from(&sk);

let note = b"pay alice 5 units".to_vec();
let aad = b"block-height:1000";

let envelope = seal::<oring::X25519, WalletDomain>(&pk, &note, aad, &mut rng).unwrap();
let opened = open::<oring::X25519, WalletDomain, _>(&sk, &pk, &envelope, aad).unwrap();
assert_eq!(opened, Some(note));
```

The snippet above is illustrative; `examples/seal_open.rs` is the compiled version. It runs this end to end and adds `Scanner`, the batch path: instead of calling `open` once per envelope, a wallet builds one `Scanner` for its key and scans a whole inbox in fixed-size chunks. A commit mismatch means the envelope was not addressed to this key and is skipped; a commit match with a failure afterward (AEAD, note decoding, `Domain::verify`) means an authenticated envelope that is wrong, and is surfaced as `Err(Malformed)` rather than dropped, because silently dropping an authenticated note can lock funds invisibly.

```
cargo run -p oring --example seal_open --features x25519
```

### Features

| feature | pulls in | notes |
|---|---|---|
| (default) | nothing | traits and suite v1 only; bring your own `Kem` |
| `k256` | k256 with `ecdh` | secp256k1, matches most iptf-pocs consumers |
| `x25519` | x25519-dalek | |
| `grumpkin` | ark-grumpkin | heaviest adapter; duplicates the rand/digest stack because arkworks still sits on rand_core 0.6 and digest 0.10. The adapter never routes the caller's rng into arkworks: it draws raw bytes from `CryptoRng` and builds the scalar with `from_le_bytes_mod_order` |
| `parallel` | rayon | chunked scan across the rayon thread pool, `Scanner::scan_parallel` |
| `serde`, `wincode` | gated derives on `SealedNote` | rotortree pattern, serializes the byte string only |
| `std` | forwards std to dependencies | |
| `test-helpers` | `MockKem`, `TestDomain`, the adapter conformance suite | |

`default = []` on purpose: any default adapter would drag its curve into every workspace member through feature unification, and consumers are split between k256 and x25519 already.

### Tuning

- `SCAN_CHUNK` (default `64`): envelopes decapsulated per `Kem::decap_batch` call inside `Scanner::scan`. The k256 adapter overrides `decap_batch` to spend one field inversion per chunk instead of one per envelope (`batch_normalize`), so a larger chunk amortizes that cost further at the price of a larger working set.
- `MAX_CT_LEN` (default `64 KiB`): the ciphertext length bound `SealedNote::parse` enforces, and the size the scanner's scratch buffer is pre-reserved to so a scan never grows it. Raise it only if notes can exceed 64 KiB minus the 16-byte AEAD tag.
- `parallel`: routes each chunk's derive-and-decrypt step across rayon instead of the scanner's own reused buffers, since those cannot be shared across threads. It pays off once a chunk's decrypt work dominates the `decap_batch` call that precedes it, which in practice means scanning enough envelopes per call that thread handoff is not the bottleneck. Benchmark your workload with `benches/scan.rs` before enabling it in a hot path.

<!-- ANCHOR_END: usage -->

## Development

### Prerequisites

- [cargo-hack](https://github.com/taiki-e/cargo-hack?tab=readme-ov-file#installation): to test all combinations of feature flags
- [cargo-nextest](https://nexte.st/): rust test runner

### Check

```
cargo hack check -p oring --feature-powerset
```

### Clippy

```
cargo hack clippy -p oring --feature-powerset -- -D warnings
```

### Format

```
cargo +nightly fmt -p oring
```

### Testing

```
cargo hack nextest run -p oring --feature-powerset
```

Golden vectors under `tests/golden/` freeze the wire format per adapter; a mismatch there is a format break, not a flaky test.

### Benchmarks

```
cargo bench -p oring -- --list
```

`benches/scan.rs` is feature-gated; see the [Cargo.toml entry](Cargo.toml) for the exact flags.
