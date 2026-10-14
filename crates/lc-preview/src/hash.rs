//! Hashing: one content identity for files, and 128-bit keys for caches.
//!
//! - [`ContentId`] / [`ContentHasher`]: what identifies a file's bytes (duplicate detection,
//!   relinking, copy verification). BLAKE3-256 — cryptographic, so two different files never
//!   share an identity, and the same 64-hex string the editor gives a linked source
//!   (`blake3::hash(bytes).to_hex()`).
//! - [`Hash128`] / [`Hasher128`]: SipHash-1-3 keys for caches and derived files (thumbnails,
//!   smart previews, AI patches). Not an identity: they hash strings that already contain one.
//!   Several are persisted as file names, so they stay as they are.

use std::fmt;
use std::hash::Hasher;

use siphasher::sip128::{Hasher128 as _, SipHasher13};

/// A 128-bit hash, shown as 32 hex digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash128(pub u128);

impl fmt::Display for Hash128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl Hash128 {
    pub fn parse(s: &str) -> Option<Hash128> {
        (s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| u128::from_str_radix(s, 16).ok().map(Hash128)).flatten()
    }
}

/// Streaming hasher.
#[derive(Clone, Debug)]
pub struct Hasher128(SipHasher13);

impl Default for Hasher128 {
    fn default() -> Self {
        Hasher128(SipHasher13::new_with_keys(0x4c69_6768_7443_7261, 0x6674_2043_6f6e_7465))
    }
}

impl Hasher128 {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.write(bytes);
        self
    }
    /// Length-prefixed string (so `("ab","c")` and `("a","bc")` differ).
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.0.write(&(s.len() as u64).to_le_bytes());
        self.0.write(s.as_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.write(&v.to_le_bytes());
        self
    }
    pub fn finish(&self) -> Hash128 {
        Hash128(self.0.finish128().as_u128())
    }
}

/// SipHash-128 of a whole byte string: a cache key, never a file's identity (that is
/// [`ContentId::of`]).
pub fn hash_bytes(bytes: &[u8]) -> Hash128 {
    Hasher128::new().update(bytes).finish()
}

/// What identifies a file's bytes: BLAKE3-256, shown (and stored) as 64 hex digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentId(pub [u8; 32]);

impl ContentId {
    /// The identity of `bytes`.
    pub fn of(bytes: &[u8]) -> ContentId {
        ContentId(*blake3::hash(bytes).as_bytes())
    }

    /// 64 hex digits (either case); anything else is not an identity.
    pub fn parse(s: &str) -> Option<ContentId> {
        if s.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (o, [hi, lo]) in out.iter_mut().zip(s.as_bytes().as_chunks::<2>().0) {
            *o = (hex_digit(*hi)? << 4) | hex_digit(*lo)?;
        }
        Some(ContentId(out))
    }
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|x| write!(f, "{x:02x}"))
    }
}

/// Streaming [`ContentId`] (files read in chunks).
#[derive(Clone, Debug, Default)]
pub struct ContentHasher(Box<blake3::Hasher>);

impl ContentHasher {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.update(bytes);
        self
    }
    pub fn finish(&self) -> ContentId {
        ContentId(*self.0.finalize().as_bytes())
    }
}
