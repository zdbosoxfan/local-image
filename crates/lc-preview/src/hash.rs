//! Hashing: one content identity for files, and 128-bit keys for caches.
//!
//! - [`ContentId`] / [`ContentHasher`]: what identifies a file's bytes (duplicate detection,
//!   relinking, copy verification). BLAKE3-256 — cryptographic, so two different files never
//!   share an identity, and the same 64-hex string the editor gives a linked source
//!   (`blake3::hash(bytes).to_hex()`). Catalogs written before it stored the 128-bit SipHash of
//!   [`hash_bytes`] instead; [`ContentId::Legacy`] still reads and checks those.
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

/// SipHash-128 of a whole byte string: a cache key. As a file's identity it is the legacy
/// scheme ([`ContentId::Legacy`]); new identities are [`ContentId::of`].
pub fn hash_bytes(bytes: &[u8]) -> Hash128 {
    Hasher128::new().update(bytes).finish()
}

/// What identifies a file's bytes. Shown (and stored) as hex: 64 digits for BLAKE3, 32 for the
/// legacy SipHash; [`ContentId::parse`] tells them apart by length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContentId {
    /// BLAKE3-256 of the bytes: what every new identity is.
    Blake3([u8; 32]),
    /// [`hash_bytes`] of the bytes, as catalogs stored it before BLAKE3. Read and checked, never
    /// written for a new photo.
    Legacy(Hash128),
}

impl ContentId {
    /// The identity of `bytes` (BLAKE3).
    pub fn of(bytes: &[u8]) -> ContentId {
        ContentId::Blake3(*blake3::hash(bytes).as_bytes())
    }

    /// The identity of `bytes` in the same scheme as `self` (to check bytes against a stored one).
    pub fn of_like(&self, bytes: &[u8]) -> ContentId {
        match self {
            ContentId::Blake3(_) => ContentId::of(bytes),
            ContentId::Legacy(_) => ContentId::Legacy(hash_bytes(bytes)),
        }
    }

    /// A streaming hasher in the same scheme as `self`.
    pub fn hasher(&self) -> ContentHasher {
        match self {
            ContentId::Blake3(_) => ContentHasher::new(),
            ContentId::Legacy(_) => ContentHasher::legacy_only(),
        }
    }

    /// 64 hex digits → BLAKE3, 32 → legacy; anything else is not an identity.
    pub fn parse(s: &str) -> Option<ContentId> {
        match s.len() {
            64 => {
                let mut out = [0u8; 32];
                for (o, [hi, lo]) in out.iter_mut().zip(s.as_bytes().as_chunks::<2>().0) {
                    *o = (hex_digit(*hi)? << 4) | hex_digit(*lo)?;
                }
                Some(ContentId::Blake3(out))
            }
            32 => Hash128::parse(s).map(ContentId::Legacy),
            _ => None,
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, ContentId::Legacy(_))
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
        match self {
            ContentId::Blake3(b) => b.iter().try_for_each(|x| write!(f, "{x:02x}")),
            ContentId::Legacy(h) => h.fmt(f),
        }
    }
}

impl From<Hash128> for ContentId {
    fn from(h: Hash128) -> Self {
        ContentId::Legacy(h)
    }
}

/// Streaming [`ContentId`]: BLAKE3, the legacy SipHash, or both in one pass over the bytes
/// (while a catalog still holds legacy identities a new file must be compared against).
#[derive(Clone, Debug)]
pub struct ContentHasher {
    blake3: Option<Box<blake3::Hasher>>,
    legacy: Option<Hasher128>,
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentHasher {
    /// BLAKE3 only.
    pub fn new() -> Self {
        ContentHasher { blake3: Some(Box::default()), legacy: None }
    }
    /// BLAKE3 and, when `legacy`, the legacy SipHash too.
    pub fn with_legacy(legacy: bool) -> Self {
        ContentHasher { legacy: legacy.then(Hasher128::new), ..Self::new() }
    }
    /// The legacy SipHash only (checking bytes against a stored legacy identity).
    pub fn legacy_only() -> Self {
        ContentHasher { blake3: None, legacy: Some(Hasher128::new()) }
    }
    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        if let Some(h) = &mut self.blake3 {
            h.update(bytes);
        }
        if let Some(h) = &mut self.legacy {
            h.update(bytes);
        }
        self
    }
    /// The identity: BLAKE3 when it is being computed, else the legacy one.
    pub fn finish(&self) -> ContentId {
        match (&self.blake3, &self.legacy) {
            (Some(h), _) => ContentId::Blake3(*h.finalize().as_bytes()),
            (None, Some(h)) => ContentId::Legacy(h.finish()),
            (None, None) => ContentId::Legacy(Hasher128::new().finish()),
        }
    }
    /// The legacy identity, when it is being computed.
    pub fn finish_legacy(&self) -> Option<ContentId> {
        self.legacy.as_ref().map(|h| ContentId::Legacy(h.finish()))
    }
}
