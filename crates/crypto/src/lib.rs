//! Crypto that Grafana needs, in safe Rust: SHA-256, HMAC-SHA256, PBKDF2-SHA256, MD5, SHA-1, AES-256-GCM, CRC-32,
//! random bytes, hex and base64. AES and GCM come from baileys-rs crypto/soft.rs (not constant time: use them only
//! for secrets at rest).

#[derive(Debug, Clone, PartialEq)]
pub struct CryptoError(pub &'static str);
impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for CryptoError {}
pub type Result<T> = std::result::Result<T, CryptoError>;

// ---------------- SHA-256 ----------------
const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
const H256: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

fn sha256_block(h: &mut [u32; 8], c: &[u8]) {
    let c = &c[..64];
    let mut w = [0u32; 64];
    let mut i = 0;
    while i < 16 {
        w[i] = u32::from_be_bytes([c[i * 4], c[i * 4 + 1], c[i * 4 + 2], c[i * 4 + 3]]);
        i += 1;
    }
    while i < 64 {
        let x = w[i - 15];
        let y = w[i - 2];
        let s0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
        let s1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        i += 1;
    }
    // the eight working variables in registers (rebuilding an array each round was 10% slower)
    let (mut a, mut b, mut cc, mut d, mut e, mut f, mut g, mut hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
    let mut i = 0;
    while i < 64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = g ^ (e & (f ^ g));
        let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K256[i]).wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let mj = (a & b) | (cc & (a | b));
        let t2 = s0.wrapping_add(mj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = cc;
        cc = b;
        b = a;
        a = t1.wrapping_add(t2);
        i += 1;
    }
    h[0] = h[0].wrapping_add(a);
    h[1] = h[1].wrapping_add(b);
    h[2] = h[2].wrapping_add(cc);
    h[3] = h[3].wrapping_add(d);
    h[4] = h[4].wrapping_add(e);
    h[5] = h[5].wrapping_add(f);
    h[6] = h[6].wrapping_add(g);
    h[7] = h[7].wrapping_add(hh);
}

/// Streaming SHA-256.
#[derive(Clone)]
pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    n: usize,
    len: u64,
}
impl Default for Sha256 {
    fn default() -> Self {
        Sha256::new()
    }
}
impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 { h: H256, buf: [0; 64], n: 0, len: 0 }
    }
    pub fn update(&mut self, d: &[u8]) {
        self.len = self.len.wrapping_add(d.len() as u64);
        let mut d = d;
        if self.n > 0 {
            let take = (64 - self.n).min(d.len());
            self.buf[self.n..self.n + take].copy_from_slice(&d[..take]);
            self.n += take;
            d = &d[take..];
            if self.n == 64 {
                let b = self.buf;
                sha256_block(&mut self.h, &b);
                self.n = 0;
            }
        }
        while d.len() >= 64 {
            sha256_block(&mut self.h, &d[..64]);
            d = &d[64..];
        }
        if !d.is_empty() {
            self.buf[..d.len()].copy_from_slice(d);
            self.n = d.len();
        }
    }
    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.len.wrapping_mul(8);
        let mut pad = [0u8; 72];
        pad[0] = 0x80;
        let padlen = if self.n < 56 { 56 - self.n } else { 120 - self.n };
        pad[padlen..padlen + 8].copy_from_slice(&bits.to_be_bytes());
        self.update(&pad[..padlen + 8]);
        let mut o = [0u8; 32];
        for i in 0..8 {
            o[i * 4..i * 4 + 4].copy_from_slice(&self.h[i].to_be_bytes());
        }
        o
    }
}
pub fn sha256(d: &[u8]) -> [u8; 32] {
    let mut s = Sha256::new();
    s.update(d);
    s.finish()
}

// ---------------- HMAC-SHA256 ----------------
/// HMAC-SHA256 with the key schedule done once: `sign` does no allocation.
#[derive(Clone)]
pub struct HmacSha256 {
    inner: Sha256,
    outer: Sha256,
}
impl HmacSha256 {
    pub fn new(key: &[u8]) -> HmacSha256 {
        let mut k = [0u8; 64];
        if key.len() > 64 {
            k[..32].copy_from_slice(&sha256(key));
        } else {
            k[..key.len()].copy_from_slice(key);
        }
        let mut ip = [0u8; 64];
        let mut op = [0u8; 64];
        for i in 0..64 {
            ip[i] = k[i] ^ 0x36;
            op[i] = k[i] ^ 0x5c;
        }
        let mut inner = Sha256::new();
        inner.update(&ip);
        let mut outer = Sha256::new();
        outer.update(&op);
        HmacSha256 { inner, outer }
    }
    /// Starts a streaming MAC with this key.
    pub fn start(&self) -> HmacStream {
        HmacStream { inner: self.inner.clone(), outer: self.outer.clone() }
    }
    pub fn sign(&self, d: &[u8]) -> [u8; 32] {
        let mut s = self.start();
        s.update(d);
        s.finish()
    }
}
pub struct HmacStream {
    inner: Sha256,
    outer: Sha256,
}
impl HmacStream {
    pub fn update(&mut self, d: &[u8]) {
        self.inner.update(d);
    }
    pub fn finish(self) -> [u8; 32] {
        let ih = self.inner.finish();
        let mut o = self.outer;
        o.update(&ih);
        o.finish()
    }
}
pub fn hmac_sha256(key: &[u8], d: &[u8]) -> [u8; 32] {
    HmacSha256::new(key).sign(d)
}

/// PBKDF2-HMAC-SHA256 with any output length (Go's `pbkdf2.Key(sha256.New, pw, salt, iters, len)`).
pub fn pbkdf2_sha256(pw: &[u8], salt: &[u8], iters: u32, out_len: usize) -> Vec<u8> {
    let mac = HmacSha256::new(pw);
    let mut out = Vec::with_capacity(out_len);
    let mut block = 1u32;
    while out.len() < out_len {
        let mut s = mac.start();
        s.update(salt);
        s.update(&block.to_be_bytes());
        let mut u = s.finish();
        let mut t = u;
        let mut i = 1;
        while i < iters {
            u = mac.sign(&u);
            for j in 0..32 {
                t[j] ^= u[j];
            }
            i += 1;
        }
        let take = (out_len - out.len()).min(32);
        out.extend_from_slice(&t[..take]);
        block += 1;
    }
    out
}

/// Grafana's `util.EncodePassword`: hex of PBKDF2-SHA256(password, salt, 10000 rounds, 50 bytes).
pub fn encode_password(password: &str, salt: &str) -> String {
    hex(&pbkdf2_sha256(password.as_bytes(), salt.as_bytes(), 10000, 50))
}

/// Compares two byte strings in time that depends only on their lengths.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for i in 0..a.len() {
        d |= a[i] ^ b[i];
    }
    d == 0
}

// ---------------- MD5 and SHA-1 ----------------
fn md_pad(d: &[u8], be: bool) -> Vec<u8> {
    let mut m = Vec::with_capacity(d.len() + 72);
    m.extend_from_slice(d);
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    let bits = (d.len() as u64).wrapping_mul(8);
    if be {
        m.extend_from_slice(&bits.to_be_bytes());
    } else {
        m.extend_from_slice(&bits.to_le_bytes());
    }
    m
}
pub fn md5(d: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for c in md_pad(d, false).chunks(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([c[i * 4], c[i * 4 + 1], c[i * 4 + 2], c[i * 4 + 3]]);
        }
        let [mut a, mut b, mut cc, mut dd] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & cc) | (!b & dd), i),
                1 => ((dd & b) | (!dd & cc), (5 * i + 1) % 16),
                2 => (b ^ cc ^ dd, (3 * i + 5) % 16),
                _ => (cc ^ (b | !dd), (7 * i) % 16),
            };
            let t = dd;
            dd = cc;
            cc = b;
            b = b.wrapping_add(a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]).rotate_left(S[i]));
            a = t;
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(cc), h[3].wrapping_add(dd)];
    }
    let mut o = [0u8; 16];
    for i in 0..4 {
        o[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_le_bytes());
    }
    o
}
pub fn sha1(d: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    for c in md_pad(d, true).chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([c[i * 4], c[i * 4 + 1], c[i * 4 + 2], c[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut cc, mut dd, mut e] = h;
        for i in 0..80 {
            let (f, k) = match i / 20 {
                0 => ((b & cc) | (!b & dd), 0x5a827999u32),
                1 => (b ^ cc ^ dd, 0x6ed9eba1),
                2 => ((b & cc) | (b & dd) | (cc & dd), 0x8f1bbcdc),
                _ => (b ^ cc ^ dd, 0xca62c1d6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = dd;
            dd = cc;
            cc = b.rotate_left(30);
            b = a;
            a = t;
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(cc), h[3].wrapping_add(dd), h[4].wrapping_add(e)];
    }
    let mut o = [0u8; 20];
    for i in 0..5 {
        o[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    o
}

// ---------------- CRC-32 (IEEE) ----------------
pub fn crc32(d: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for b in d {
        c ^= *b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
        }
    }
    !c
}

// ---------------- random ----------------
/// Fills `o` from the kernel CSPRNG.
pub fn random_bytes(o: &mut [u8]) {
    use std::io::Read;
    let mut f = std::fs::File::open("/dev/urandom").expect("no /dev/urandom");
    f.read_exact(o).expect("short read from /dev/urandom");
}
pub const ALPHANUMERIC: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
/// Grafana's `util.GetRandomString(n)`: `n` characters from `alphabet` (alphanumeric when empty), without modulo bias.
pub fn random_string(n: usize, alphabet: &[u8]) -> String {
    let alpha = if alphabet.is_empty() { ALPHANUMERIC } else { alphabet };
    let limit = 256 - (256 % alpha.len());
    let mut s = String::with_capacity(n);
    let mut buf = [0u8; 64];
    while s.len() < n {
        random_bytes(&mut buf);
        for b in buf.iter() {
            if s.len() >= n {
                break;
            }
            if (*b as usize) < limit {
                s.push(alpha[*b as usize % alpha.len()] as char);
            }
        }
    }
    s
}
/// Grafana's `util.RandomHex(n)`: hex of `n` random bytes.
pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    random_bytes(&mut b);
    hex(&b)
}

// ---------------- hex and base64 ----------------
const HEXCHARS: &[u8; 16] = b"0123456789abcdef";
pub fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(HEXCHARS[(x >> 4) as usize] as char);
        s.push(HEXCHARS[(x & 15) as usize] as char);
    }
    s
}
pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    let mut o = Vec::with_capacity(b.len() / 2);
    let mut i = 0;
    while i < b.len() {
        let hi = hexval(b[i])?;
        let lo = hexval(b[i + 1])?;
        o.push(hi << 4 | lo);
        i += 2;
    }
    Some(o)
}
fn hexval(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
fn b64_enc(b: &[u8], alpha: &[u8; 64], pad: bool) -> String {
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = ((c[0] as u32) << 16) | ((*c.get(1).unwrap_or(&0) as u32) << 8) | (*c.get(2).unwrap_or(&0) as u32);
        s.push(alpha[(n >> 18) as usize & 63] as char);
        s.push(alpha[(n >> 12) as usize & 63] as char);
        if c.len() > 1 {
            s.push(alpha[(n >> 6) as usize & 63] as char);
        } else if pad {
            s.push('=');
        }
        if c.len() > 2 {
            s.push(alpha[n as usize & 63] as char);
        } else if pad {
            s.push('=');
        }
    }
    s
}
/// Standard base64 with padding (Go `base64.StdEncoding`).
pub fn base64(b: &[u8]) -> String {
    b64_enc(b, B64, true)
}
/// URL-safe base64 without padding (Go `base64.RawURLEncoding`).
pub fn base64url(b: &[u8]) -> String {
    b64_enc(b, B64URL, false)
}
/// Decodes standard or URL-safe base64, padded or not. Fails on any other character.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3 + 3);
    let mut n = 0u32;
    let mut bits = 0u32;
    let mut ended = false;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => {
                ended = true;
                continue;
            }
            _ => return None,
        };
        if ended {
            return None;
        }
        n = (n << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// AES-256-GCM with a random 12-byte nonce: returns nonce || ciphertext || tag.
pub fn seal(plain: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let mut nonce = [0u8; 12];
    random_bytes(&mut nonce);
    let mut out = nonce.to_vec();
    let ct = aes_gcm_encrypt(plain, key, &nonce, &[]).expect("valid key and nonce sizes");
    out.extend_from_slice(&ct);
    out
}
/// Opens what `seal` made.
pub fn open(sealed: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    if sealed.len() < 12 + 16 {
        return Err(CryptoError("sealed data too short"));
    }
    aes_gcm_decrypt(&sealed[12..], key, &sealed[..12], &[])
}


// ---------------- AES-256 and GCM (from baileys-rs soft.rs) ----------------
// AES-256
fn sbox() -> [u8; 256] {
    let mut s = [0u8; 256];
    let (mut p, mut q) = (1u8, 1u8);
    loop {
        p = p ^ (p << 1) ^ (if p & 0x80 != 0 { 0x1b } else { 0 });
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let x = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4);
        s[p as usize] = x ^ 0x63;
        if p == 1 {
            break;
        }
    }
    s[0] = 0x63;
    s
}
struct Aes {
    rk: [[u8; 16]; 15],
    s: [u8; 256],
}
fn xt(a: u8) -> u8 {
    (a << 1) ^ (if a & 0x80 != 0 { 0x1b } else { 0 })
}
impl Aes {
    fn new(key: &[u8]) -> Result<Aes> {
        if key.len() != 32 {
            return Err(CryptoError("Invalid key length"));
        }
        let s = sbox();
        let mut w = [0u8; 240];
        w[..32].copy_from_slice(key);
        let mut rc = 1u8;
        let mut i = 32;
        while i < 240 {
            let mut t = [w[i - 4], w[i - 3], w[i - 2], w[i - 1]];
            if i % 32 == 0 {
                t = [s[t[1] as usize] ^ rc, s[t[2] as usize], s[t[3] as usize], s[t[0] as usize]];
                rc = xt(rc);
            } else if i % 32 == 16 {
                t = [s[t[0] as usize], s[t[1] as usize], s[t[2] as usize], s[t[3] as usize]];
            }
            for j in 0..4 {
                w[i + j] = w[i + j - 32] ^ t[j];
            }
            i += 4;
        }
        let mut rk = [[0u8; 16]; 15];
        for r in 0..15 {
            rk[r].copy_from_slice(&w[r * 16..r * 16 + 16]);
        }
        Ok(Aes { rk, s })
    }
    fn enc(&self, b: &mut [u8; 16]) {
        for j in 0..16 {
            b[j] ^= self.rk[0][j];
        }
        for r in 1..15 {
            let mut t = [0u8; 16];
            for c in 0..4 {
                for j in 0..4 {
                    t[c * 4 + j] = self.s[b[((c + j) % 4) * 4 + j] as usize];
                }
            }
            if r != 14 {
                for c in 0..4 {
                    let x = [t[c * 4], t[c * 4 + 1], t[c * 4 + 2], t[c * 4 + 3]];
                    for j in 0..4 {
                        t[c * 4 + j] = xt(x[j]) ^ (xt(x[(j + 1) % 4]) ^ x[(j + 1) % 4]) ^ x[(j + 2) % 4] ^ x[(j + 3) % 4];
                    }
                }
            }
            for j in 0..16 {
                b[j] = t[j] ^ self.rk[r][j];
            }
        }
    }
}
fn gmul(x: u128, y: u128) -> u128 {
    let (mut z, mut v) = (0u128, y);
    for i in 0..128 {
        if (x >> (127 - i)) & 1 == 1 {
            z ^= v;
        }
        v = if v & 1 == 1 { (v >> 1) ^ (0xe1u128 << 120) } else { v >> 1 };
    }
    z
}
fn ghash(h: u128, aad: &[u8], c: &[u8]) -> u128 {
    let mut y = 0u128;
    for part in [aad, c] {
        for b in part.chunks(16) {
            let mut t = [0u8; 16];
            t[..b.len()].copy_from_slice(b);
            y = gmul(y ^ u128::from_be_bytes(t), h);
        }
    }
    let l = (((aad.len() as u128) * 8) << 64) | ((c.len() as u128) * 8);
    gmul(y ^ l, h)
}
fn gcm_core(d: &[u8], key: &[u8], iv: &[u8]) -> Result<(Vec<u8>, [u8; 16], u128)> {
    if iv.len() != 12 {
        return Err(CryptoError("Invalid IV length"));
    }
    let a = Aes::new(key)?;
    let mut hb = [0u8; 16];
    a.enc(&mut hb);
    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(iv);
    j0[15] = 1;
    let mut e0 = j0;
    a.enc(&mut e0);
    let mut o = Vec::with_capacity(d.len());
    let mut n = 1u32;
    for c in d.chunks(16) {
        n = n.wrapping_add(1);
        let mut k = j0;
        k[12..].copy_from_slice(&n.to_be_bytes());
        a.enc(&mut k);
        for (i, v) in c.iter().enumerate() {
            o.push(v ^ k[i]);
        }
    }
    Ok((o, e0, u128::from_be_bytes(hb)))
}
pub fn aes_gcm_encrypt(p: &[u8], key: &[u8], iv: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let (mut o, e0, h) = gcm_core(p, key, iv)?;
    let t = ghash(h, aad, &o).to_be_bytes();
    for i in 0..16 {
        o.push(t[i] ^ e0[i]);
    }
    Ok(o)
}
pub fn aes_gcm_decrypt(c: &[u8], key: &[u8], iv: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if c.len() < 16 {
        return Err(CryptoError("Unsupported state or unable to authenticate data"));
    }
    let (ct, tag) = c.split_at(c.len() - 16);
    let (o, e0, h) = gcm_core(ct, key, iv)?;
    let t = ghash(h, aad, ct).to_be_bytes();
    let mut diff = 0u8;
    for i in 0..16 {
        diff |= t[i] ^ e0[i] ^ tag[i];
    }
    if diff != 0 {
        return Err(CryptoError("Unsupported state or unable to authenticate data"));
    }
    Ok(o)
}
