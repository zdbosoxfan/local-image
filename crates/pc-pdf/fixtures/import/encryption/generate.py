"""Synthetic GPL-3.0-or-later PDF Standard-handler fixtures; no user data.
Python stdlib implements the published R2–4 fixture algorithms independently.
AES fixture blocks use installed OpenSSL, never the application crypto code.
Fixed IVs/passwords/IDs are ONLY for reproducible test data, not production use.
"""
from pathlib import Path
import hashlib
import subprocess
import zlib

OUT = Path(__file__).parent
PAD = bytes.fromhex('28bf4e5e4e758a4164004e56fffa01082e2e00b6d0683e802f0ca9fe6453697a')
ID = bytes.fromhex('00112233445566778899aabbccddeeff')
ART = b'1 0 0 rg 8 8 48 48 re f 0 0 1 rg 24 24 16 16 re f'
XML = b'<Image>plaintext metadata EI</Image>'


def md5(data):
    return hashlib.md5(data).digest()


def padded(password):
    return (password + PAD)[:32]


def rc4(key, data):
    box = list(range(256))
    j = 0
    for i in range(256):
        j = (j + box[i] + key[i % len(key)]) % 256
        box[i], box[j] = box[j], box[i]
    i = j = 0
    out = bytearray()
    for byte in data:
        i = (i + 1) % 256
        j = (j + box[i]) % 256
        box[i], box[j] = box[j], box[i]
        out.append(byte ^ box[(box[i] + box[j]) % 256])
    return bytes(out)


def credentials(revision, bits, password, metadata):
    owner_key = md5(padded(b'fixture-owner'))
    if revision >= 3:
        for _ in range(50):
            owner_key = md5(owner_key)
    owner_key = owner_key[:bits // 8]
    owner = rc4(owner_key, padded(password))
    if revision >= 3:
        for i in range(1, 20):
            owner = rc4(bytes(b ^ i for b in owner_key), owner)
    permission = -3904
    key = md5(padded(password) + owner + permission.to_bytes(4, 'little', signed=True) + ID + (b'\xff' * 4 if revision >= 4 and not metadata else b''))
    if revision >= 3:
        for _ in range(50):
            key = md5(key[:bits // 8])
    key = key[:bits // 8]
    if revision == 2:
        user = rc4(key, PAD)
    else:
        user = rc4(key, md5(PAD + ID))
        for i in range(1, 20):
            user = rc4(bytes(b ^ i for b in key), user)
        user += bytes(16)
    return owner, user, key, permission


def object_cipher(key, obj, generation, data, mode, bad_padding=False):
    if mode == 'Identity':
        return data
    derived = md5(key + obj.to_bytes(3, 'little') + generation.to_bytes(2, 'little') + (b'sAlT' if mode == 'AESV2' else b''))[:min(len(key) + 5, 16)]
    if mode == 'V2':
        return rc4(derived, data)
    iv = bytes(range(16))
    command = ['openssl', 'enc', '-aes-128-cbc', '-K', derived.hex(), '-iv', iv.hex()]
    if bad_padding:
        data += bytes(16 - len(data) % 16)
        command.append('-nopad')
    return iv + subprocess.run(command, input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout


def stream(dictionary, data):
    return b'<< /Length ' + str(len(data)).encode() + b' ' + dictionary + b' >>\nstream\n' + data + b'\nendstream'


def pdf(objects, trailer, declared_size=None):
    out = bytearray(b'%PDF-1.7\n')
    offsets = []
    for (obj, generation), body in sorted(objects.items()):
        offsets.append((obj, generation, len(out)))
        out += f'{obj} {generation} obj\n'.encode() + body + b'\nendobj\n'
    xref = len(out)
    out += b'xref\n0 1\n0000000000 65535 f \n'
    for obj, generation, offset in offsets:
        out += f'{obj} 1\n{offset:010} {generation:05} n \n'.encode()
    size = declared_size or max(obj for obj, _ in objects) + 1
    out += f'trailer\n<< /Size {size} '.encode() + trailer + f' >>\nstartxref\n{xref}\n%%EOF\n'.encode()
    return bytes(out)


def build(name, revision=4, bits=128, streams='AESV2', strings='AESV2', metadata=True, password=b'', bad=None, members=False, scalar=None, member_bomb=False, real_length=None):
    version = {2: 1, 3: 2, 4: 4}[revision]
    owner, user, key, permission = credentials(revision, bits, password, metadata)
    crypt = lambda obj, gen, data, mode, bad_padding=False: object_cipher(key, obj, gen, data, mode, bad_padding)
    literal = lambda obj, text: b'<' + crypt(obj, 0, text, strings, bad == 'string-padding' and obj == 6).hex().encode() + b'>'
    art = crypt(4, 2, zlib.compress((b' ' * (1 << 20) + ART) if bad == 'bomb' else ART), streams, bad == 'padding')
    if bad == 'block':
        art = art[:-1]
    objects = {
        (1, 0): b'<< /Type /Catalog /Pages 2 0 R /Metadata 5 0 R >>',
        (2, 0): b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        (3, 0): b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Contents 4 2 R >>',
        (4, 2): stream(b'/Filter /FlateDecode', art),
        (5, 0): stream(b'/Type /Metadata /Subtype /XML /Description ' + literal(5, b'metadata dictionary string'), crypt(5, 0, XML, streams) if metadata else XML),
        (6, 0): b'<< /Title ' + literal(6, b'Permission-only EI title') + b' /Private ' + literal(6, b'\x00\xff exact string') + b' >>',
        (7, 0): b'[4 2 R ' + literal(7, b'array literal') + b']',
        (8, 0): b'1844674407370955161',
    }
    if scalar is not None:
        objects[(8, 0)] = scalar
    if real_length is not None:
        prefix = b'/Length ' + str(len(art)).encode()
        replacement = prefix + b'.0' if real_length == 'direct' else b'/Length 8 0 R'
        objects[(4, 2)] = objects[(4, 2)].replace(prefix, replacement, 1)
        if real_length == 'indirect':
            objects[(8, 0)] = str(len(art)).encode() + b'.0'
    auth = f'<< /Filter /Standard /V {version} /R {revision} /Length {bits} /P {permission} /O <{owner.hex()}> /U <{user.hex()}> /EncryptMetadata {str(metadata).lower()}'.encode()
    if revision == 4:
        auth += f' /StmF /{ "Missing" if bad == "mapping" else "Source" } /StrF /Text /CF << /Source << /CFM /{ "None" if streams == "Identity" else streams } /Length 16 /AuthEvent /DocOpen >> /Text << /CFM /{ "None" if strings == "Identity" else strings } /Length 16 /AuthEvent /DocOpen >> >>'.encode()
    auth += b' >>'
    if bad == 'owner':
        auth = auth.replace(owner.hex().encode(), owner[:-1].hex().encode())
    objects[(9, 0)] = auth
    if members:
        member = b'11 0 << /PrivateString (member stays plaintext after stream decryption) >>'
        if member_bomb:
            member += b' ' * (1 << 20)
        encoded = crypt(10, 0, zlib.compress(member), streams)
        objects[(10, 0)] = stream(b'/Type /ObjStm /N 1 /First 5 /Filter /FlateDecode', encoded)
    trailer = b'/Root 1 0 R /Info 6 0 R /Encrypt 9 0 R /ID [<' + ID.hex().encode() + b'><' + ID.hex().encode() + b'>]'
    OUT.joinpath(name + '.pdf').write_bytes(pdf(objects, trailer, 12 if members else None))


if __name__ == '__main__':
    build('rc4-r2', revision=2, bits=40, streams='V2', strings='V2')
    build('rc4-r3', revision=3, streams='V2', strings='V2')
    build('rc4-r4', streams='V2', strings='V2')
    build('aes128-r4')
    build('aes128-clear-metadata', metadata=False)
    build('aes128-identity-streams', streams='Identity')
    build('aes128-identity-strings', strings='Identity')
    build('aes128-object-members', members=True)
    build('aes128-user-password', password=b'not-empty')
    for kind in ('padding', 'string-padding', 'block', 'mapping', 'owner', 'bomb'):
        build('invalid-' + kind, bad=kind)

    build('aes128-direct-real-length', real_length='direct')
    build('invalid-indirect-real-length', real_length='indirect')
    build('aes128-object-bomb', members=True, member_bomb=True)
    for name, scalar in [('exponent', b'1e2'), ('unknown', b'mystery'),
                         ('nested-exponent', b'[true [1e2]]'),
                         ('nested-unknown', b'<< /Nested [mystery] >>'),
                         ('hex', b'<not-hex>'), ('name', b'/bad#GG'),
                         ('number-overflow', b'184467440737095516160')]:
        build('invalid-object-bomb-' + name, members=True, member_bomb=True, scalar=scalar)
