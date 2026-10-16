# Permission-only PDF fixtures

Original synthetic fixtures, GPL-3.0-or-later. No user data, private keys or real
passwords are present. `generate.py` uses Python's standard `hashlib`/`zlib`, a
small independent fixture-only RC4 routine, and installed OpenSSL AES128-CBC.
Generated with OpenSSL 3.5.9 (29 Sep 2026). Fixed IVs, IDs and passwords exist only
for reproducible test vectors, never for production encryption. CI reads the
checked-in PDFs and does not run OpenSSL or this generator.

The fixture algorithms implement PDF Standard R2–4 key derivation independently
of the application/pinned Rust implementation. The application adds no crypto
algorithm or dependency; it uses existing hayro-syntax 0.7.2 public APIs with a
synthetic safe page tree, followed by ordinary guarded plaintext admission.

Positive files cover R2/40-bit RC4, R3/128-bit RC4, R4/RC4 and AES128, Identity
stream/string filters, EncryptMetadata=false, nonzero object generation, nested
strings and exact large scalar values. `aes128-object-members.pdf` adds encrypted
Flate object-stream transport using the canonical guard's existing physical
object/member admission representation; it does not imply xref-stream support.
Negative files use a nonempty user password, deliberate zero AES padding,
truncated ciphertext blocks, absent selected CF mapping, shortened O data, and a
compressed million-byte payload. All encryption is applied to raw stream bytes
before PDF framing; string encryption preserves original object identities.

Tests compare rendered pixels with an independently assembled plaintext drawing,
inspect decrypted metadata/strings/references, and prove that original page
cycles and decompression bombs remain outside the bootstrap parser and fail the
subsequent guard. Wrong passwords, malformed security/cipher data, quotas and
cancellation must fail before publication. R5/R6, nonclassic encrypted indexes,
separate EFF/per-stream Crypt policies and nonempty-password UI remain outside
this bounded slice.

Repair-path regressions pair an encrypted million-byte Flate ObjStm with invalid
scalar, nested scalar, hex string, name and overflowing number forms. A matching
valid-value object bomb must reach the bounded plaintext decoder only after
normalization. Both crypto bootstraps contain canonical arrays of hex strings
and no streams at all, even if the pinned parser attempts repair. Stream payloads
use the selected stream cipher through a second StrF transport at the original
object ID/generation. Direct integral-real Length is rewritten as an authoritative
integer; indirect noninteger Length is refused by the verified resolver.
