//! A small, fast gzip encoder (RFC 1952 around RFC 1951 deflate) for response bodies.
//!
//! Greedy LZ77 with one hash probe (4-byte minimum match, 32 KB window), then per block the cheapest of dynamic
//! Huffman, fixed Huffman, or stored. It allocates only what the input needs (a hash table sized to the input and
//! one symbol buffer), so small JSON bodies do not pay for a full zlib-sized state.

/// Appends `data` as one gzip member to `out`.
pub fn gzip(data: &[u8], out: &mut Vec<u8>) {
    // header: magic, CM=8 (deflate), FLG=0, MTIME=0, XFL=0, OS=255 (unknown)
    out.extend_from_slice(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255]);
    deflate(data, out);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
}

/// `data` gzipped into a new buffer.
pub fn gzip_vec(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 4 + 64);
    gzip(data, &mut out);
    out
}

// ---------------- CRC-32 (IEEE), slicing by 16 ----------------

/// Table `s` (0..16) is `t[s * 256..s * 256 + 256]`.
const fn crc_tables() -> [u32; 4096] {
    let mut t = [0u32; 4096];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    let mut i = 0;
    while i < 256 {
        let mut s = 1;
        while s < 16 {
            let p = t[(s - 1) * 256 + i];
            t[s * 256 + i] = (p >> 8) ^ t[(p & 0xff) as usize];
            s += 1;
        }
        i += 1;
    }
    t
}
static CRC: [u32; 4096] = crc_tables();

/// One slicing-by-16 step: the CRC state after the 16 bytes of `b`.
#[inline(always)]
fn crc_step16(c: u32, b: &[u8]) -> u32 {
    let t = &CRC;
    let b = &b[..16];
    let x = c ^ u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    t[15 * 256 + (x & 0xff) as usize]
        ^ t[14 * 256 + ((x >> 8) & 0xff) as usize]
        ^ t[13 * 256 + ((x >> 16) & 0xff) as usize]
        ^ t[12 * 256 + (x >> 24) as usize]
        ^ t[11 * 256 + b[4] as usize]
        ^ t[10 * 256 + b[5] as usize]
        ^ t[9 * 256 + b[6] as usize]
        ^ t[8 * 256 + b[7] as usize]
        ^ t[7 * 256 + b[8] as usize]
        ^ t[6 * 256 + b[9] as usize]
        ^ t[5 * 256 + b[10] as usize]
        ^ t[4 * 256 + b[11] as usize]
        ^ t[3 * 256 + b[12] as usize]
        ^ t[2 * 256 + b[13] as usize]
        ^ t[256 + b[14] as usize]
        ^ t[b[15] as usize]
}

/// CRC state after `d`, starting from state `c` (no pre or post inversion).
fn crc_update(mut c: u32, d: &[u8]) -> u32 {
    let mut chunks = d.chunks_exact(16);
    for b in &mut chunks {
        c = crc_step16(c, b);
    }
    for b in chunks.remainder() {
        c = (c >> 8) ^ CRC[((c ^ *b as u32) & 0xff) as usize];
    }
    c
}

/// CRC-32 (IEEE 802.3), as gzip and zlib's `crc32()`.
pub fn crc32(d: &[u8]) -> u32 {
    !crc_update(0xffff_ffff, d)
}

// ---------------- bit writer ----------------

struct Bits<'a> {
    out: &'a mut Vec<u8>,
    buf: u64,
    n: u32,
}
impl Bits<'_> {
    /// Writes the low `n` bits of `v` (n <= 32), least significant first.
    #[inline]
    fn put(&mut self, v: u32, n: u32) {
        self.buf |= (v as u64) << self.n;
        self.n += n;
        if self.n >= 32 {
            self.out.extend_from_slice(&(self.buf as u32).to_le_bytes());
            self.buf >>= 32;
            self.n -= 32;
        }
    }
    /// Pads to a byte boundary and writes the pending bytes.
    fn align(&mut self) {
        while self.n > 0 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.n = self.n.saturating_sub(8);
        }
        self.buf = 0;
    }
}

// ---------------- symbols ----------------

// A symbol is a literal byte (dist 0) or a match: `dist << 9 | len`, len 3..=258, dist 1..=32768.
const BLOCK_SYMS: usize = 8 * 1024;
const WINDOW: usize = 32 * 1024;
const MAX_MATCH: usize = 258;
const MIN_MATCH: usize = 4;

/// Length symbol index (0..=28, for codes 257..=285), extra bit count, extra bit value.
#[inline]
fn len_code(len: u32) -> (usize, u32, u32) {
    let v = len - 3;
    if v < 8 {
        return (v as usize, 0, 0);
    }
    if v == 255 {
        return (28, 0, 0);
    }
    let b = 31 - v.leading_zeros();
    let i = 4 * (b - 1) + ((v >> (b - 2)) & 3);
    let extra = (i - 4) / 4;
    let base = (4 + (i & 3)) << extra;
    (i as usize, extra, v - base)
}

/// Distance code (0..=29), extra bit count, extra bit value.
#[inline]
fn dist_code(dist: u32) -> (usize, u32, u32) {
    let v = dist - 1;
    if v < 4 {
        return (v as usize, 0, 0);
    }
    let b = 31 - v.leading_zeros();
    let code = 2 * b + ((v >> (b - 1)) & 1);
    let extra = b - 1;
    let base = (2 | (code & 1)) << extra;
    (code as usize, extra, v - base)
}

#[inline(always)]
fn read4(d: &[u8], i: usize) -> u32 {
    let s = &d[i..i + 4];
    u32::from_le_bytes([s[0], s[1], s[2], s[3]])
}
#[inline(always)]
fn read8(d: &[u8], i: usize) -> u64 {
    let s = &d[i..i + 8];
    u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
}

struct Block {
    syms: Vec<u32>,
    lfreq: [u32; 286],
    dfreq: [u32; 30],
    /// Input bytes covered by the pending symbols.
    start: usize,
}
impl Block {
    #[inline]
    fn lit(&mut self, c: u8) {
        self.syms.push(c as u32);
        self.lfreq[c as usize] += 1;
    }
    #[inline]
    fn mat(&mut self, len: u32, dist: u32) {
        self.syms.push(dist << 9 | len);
        self.lfreq[257 + len_code(len).0] += 1;
        self.dfreq[dist_code(dist).0] += 1;
    }
}

fn deflate(data: &[u8], out: &mut Vec<u8>) {
    let n = data.len();
    let mut w = Bits { out, buf: 0, n: 0 };
    let mut bl = Block { syms: Vec::with_capacity(if n < BLOCK_SYMS { n + 1 } else { BLOCK_SYMS }), lfreq: [0; 286], dfreq: [0; 30], start: 0 };
    if n == 0 {
        flush_block(&mut w, &mut bl, data, 0, true);
        w.align();
        return;
    }
    // Hash table of the last position (its low 16 bits) for each 4-byte hash, sized to the input. A stale or
    // wrapped entry only points at some earlier position less than 64 KB back; the bytes are compared before use.
    let lg = (usize::BITS - n.leading_zeros()) as usize;
    let bits = lg.saturating_sub(3).clamp(9, 14);
    let mut table = vec![0u16; 1 << bits];
    let shift = 32 - bits as u32;
    let mut i = 0;
    let mut miss = 0u32;
    while i + MIN_MATCH <= n {
        let x = read4(data, i);
        let h = (x.wrapping_mul(0x9E37_79B1) >> shift) as usize;
        let back = (i as u16).wrapping_sub(table[h]) as usize;
        table[h] = i as u16;
        let cand = i.wrapping_sub(back);
        if back > 0 && back <= WINDOW && back <= i && read4(data, cand) == x {
            let max = if n - i < MAX_MATCH { n - i } else { MAX_MATCH };
            let mut l = MIN_MATCH;
            while l + 8 <= max {
                let d = read8(data, cand + l) ^ read8(data, i + l);
                if d != 0 {
                    l += (d.trailing_zeros() / 8) as usize;
                    break;
                }
                l += 8;
            }
            if l + 8 > max {
                while l < max && data[cand + l] == data[i + l] {
                    l += 1;
                }
            }
            bl.mat(l as u32, (i - cand) as u32);
            // remember the position just before the match end, so the next repeat of this text finds it
            let e = i + l - 1;
            if e + MIN_MATCH <= n && l > MIN_MATCH {
                let he = (read4(data, e).wrapping_mul(0x9E37_79B1) >> shift) as usize;
                table[he] = e as u16;
            }
            i += l;
            miss = 0;
        } else {
            bl.lit(data[i]);
            i += 1;
            // after a long run without a match, hash fewer positions (data that does not compress goes faster)
            miss += 1;
            if miss >= 64 {
                let stop = i + (miss >> 6) as usize;
                while i < stop && i < n {
                    bl.lit(data[i]);
                    i += 1;
                }
            }
        }
        if bl.syms.len() >= BLOCK_SYMS {
            flush_block(&mut w, &mut bl, data, i, false);
        }
    }
    while i < n {
        bl.lit(data[i]);
        i += 1;
    }
    flush_block(&mut w, &mut bl, data, n, true);
    w.align();
}

// ---------------- Huffman codes ----------------

/// Code lengths (at most `max_bits`) for `freq`, written to `lens`. At least two symbols get a code, so every
/// decoder accepts the table.
fn build_lengths(freq: &[u32], max_bits: usize, lens: &mut [u8]) {
    for l in lens.iter_mut() {
        *l = 0;
    }
    // used symbols as `freq << 9 | symbol`, sorted (by frequency, then symbol); a block has fewer than 2^22 symbols
    let mut keys = [0u32; 286];
    let mut n = 0;
    for (s, f) in freq.iter().enumerate() {
        if *f > 0 {
            keys[n] = *f << 9 | s as u32;
            n += 1;
        }
    }
    let mut k = 0;
    while n < 2 {
        if freq[k] == 0 {
            keys[n] = 1 << 9 | k as u32;
            n += 1;
        }
        k += 1;
    }
    let keys = &mut keys[..n];
    keys.sort_unstable();
    // Moffat-Katajainen in-place minimum-redundancy code lengths
    let mut a = [0u32; 286];
    for (i, key) in keys.iter().enumerate() {
        a[i] = key >> 9;
    }
    let a = &mut a[..n];
    a[0] += a[1];
    let mut root = 0usize;
    let mut leaf = 2usize;
    for next in 1..n - 1 {
        if leaf >= n || a[root] < a[leaf] {
            a[next] = a[root];
            a[root] = next as u32;
            root += 1;
        } else {
            a[next] = a[leaf];
            leaf += 1;
        }
        if leaf >= n || (root < next && a[root] < a[leaf]) {
            a[next] += a[root];
            a[root] = next as u32;
            root += 1;
        } else {
            a[next] += a[leaf];
            leaf += 1;
        }
    }
    a[n - 2] = 0;
    let mut next = n as isize - 3;
    while next >= 0 {
        let p = a[next as usize] as usize;
        a[next as usize] = a[p] + 1;
        next -= 1;
    }
    let mut avbl: isize = 1;
    let mut used: isize = 0;
    let mut dpth: u32 = 0;
    let mut root: isize = n as isize - 2;
    let mut next: isize = n as isize - 1;
    while avbl > 0 {
        while root >= 0 && a[root as usize] == dpth {
            used += 1;
            root -= 1;
        }
        while avbl > used {
            a[next as usize] = dpth;
            next -= 1;
            avbl -= 1;
        }
        avbl = 2 * used;
        dpth += 1;
        used = 0;
    }
    // count codes per length, then limit to max_bits (keeping the Kraft sum exact)
    let mut count = [0u32; 33];
    for l in a.iter() {
        let l = *l as usize;
        count[if l > 32 { 32 } else { l }] += 1;
    }
    let mut over = 0;
    for l in max_bits + 1..33 {
        over += count[l];
        count[l] = 0;
    }
    count[max_bits] += over;
    let mut total: u64 = 0;
    for l in 1..=max_bits {
        total += (count[l] as u64) << (max_bits - l);
    }
    while total > 1u64 << max_bits {
        count[max_bits] -= 1;
        for l in (1..max_bits).rev() {
            if count[l] > 0 {
                count[l] -= 1;
                count[l + 1] += 2;
                break;
            }
        }
        total -= 1;
    }
    // longest codes to the rarest symbols
    let mut j = 0;
    for l in (1..=max_bits).rev() {
        for _ in 0..count[l] {
            lens[(keys[j] & 511) as usize] = l as u8;
            j += 1;
        }
    }
}

/// Canonical codes for `lens`, bit-reversed for the LSB-first writer, as `code | length << 16`.
fn build_codes(lens: &[u8], codes: &mut [u32]) {
    let mut count = [0u16; 16];
    for l in lens.iter() {
        count[*l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for b in 1..16 {
        code = (code + count[b - 1]) << 1;
        next[b] = code;
    }
    for (s, l) in lens.iter().enumerate() {
        let l = *l as usize;
        if l == 0 {
            codes[s] = 0;
            continue;
        }
        let c = next[l];
        next[l] += 1;
        codes[s] = (c.reverse_bits() >> (16 - l)) as u32 | (l as u32) << 16;
    }
}

const LEN_EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_EXTRA: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn fixed_lengths() -> ([u8; 286], [u8; 30]) {
    let mut l = [0u8; 286];
    for (i, v) in l.iter_mut().enumerate() {
        *v = if i < 144 {
            8
        } else if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
    }
    (l, [5u8; 30])
}

/// Bits of the code words for `freq` under `lens`.
fn code_bits(freq: &[u32], lens: &[u8]) -> u64 {
    let mut b = 0u64;
    for (f, l) in freq.iter().zip(lens.iter()) {
        b += *f as u64 * *l as u64;
    }
    b
}

/// Extra bits of the length and distance symbols (the same for every Huffman table).
fn extra_bits(bl: &Block) -> u64 {
    let mut b = 0u64;
    for (f, e) in bl.lfreq[257..].iter().zip(LEN_EXTRA.iter()) {
        b += *f as u64 * *e as u64;
    }
    for (f, e) in bl.dfreq.iter().zip(DIST_EXTRA.iter()) {
        b += *f as u64 * *e as u64;
    }
    b
}

/// Bits of the code words under the fixed Huffman table.
fn fixed_code_bits(bl: &Block) -> u64 {
    let sum = |a: &[u32]| a.iter().map(|f| *f as u64).sum::<u64>();
    sum(&bl.lfreq[..144]) * 8 + sum(&bl.lfreq[144..256]) * 9 + sum(&bl.lfreq[256..280]) * 7 + sum(&bl.lfreq[280..]) * 8 + sum(&bl.dfreq) * 5
}

/// Run-length codes (0..=18 with their extra values) for the code lengths.
fn rle_lengths(lens: &[u8], out: &mut Vec<(u8, u8)>) {
    let mut i = 0;
    while i < lens.len() {
        let l = lens[i];
        let mut run = 1;
        while i + run < lens.len() && lens[i + run] == l {
            run += 1;
        }
        i += run;
        if l == 0 {
            while run >= 11 {
                let r = if run > 138 { 138 } else { run };
                out.push((18, (r - 11) as u8));
                run -= r;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u8));
                run = 0;
            }
        } else {
            out.push((l, 0));
            run -= 1;
            while run >= 3 {
                let r = if run > 6 { 6 } else { run };
                out.push((16, (r - 3) as u8));
                run -= r;
            }
        }
        for _ in 0..run {
            out.push((l, 0));
        }
    }
}

/// Writes the pending symbols (input `bl.start..end`) as one block, the cheapest of dynamic, fixed, or stored.
fn flush_block(w: &mut Bits<'_>, bl: &mut Block, data: &[u8], end: usize, last: bool) {
    bl.lfreq[256] += 1; // end of block
    let mut ll = [0u8; 286];
    let mut dl = [0u8; 30];
    build_lengths(&bl.lfreq, 15, &mut ll);
    build_lengths(&bl.dfreq, 15, &mut dl);
    let mut hlit = 286;
    while hlit > 257 && ll[hlit - 1] == 0 {
        hlit -= 1;
    }
    let mut hdist = 30;
    while hdist > 1 && dl[hdist - 1] == 0 {
        hdist -= 1;
    }
    let mut all = [0u8; 316];
    all[..hlit].copy_from_slice(&ll[..hlit]);
    all[hlit..hlit + hdist].copy_from_slice(&dl[..hdist]);
    let mut rle: Vec<(u8, u8)> = Vec::with_capacity(64);
    rle_lengths(&all[..hlit + hdist], &mut rle);
    let mut clfreq = [0u32; 19];
    for (c, _) in rle.iter() {
        clfreq[*c as usize] += 1;
    }
    let mut cll = [0u8; 19];
    build_lengths(&clfreq, 7, &mut cll);
    let mut hclen = 19;
    while hclen > 4 && cll[CL_ORDER[hclen - 1]] == 0 {
        hclen -= 1;
    }
    let extra = extra_bits(bl);
    let mut dyn_bits = 14 + 3 * hclen as u64 + code_bits(&bl.lfreq, &ll) + code_bits(&bl.dfreq, &dl) + extra;
    for (c, _) in rle.iter() {
        let extra = match *c {
            16 => 2,
            17 => 3,
            18 => 7,
            _ => 0,
        };
        dyn_bits += cll[*c as usize] as u64 + extra;
    }
    let fixed_bits = fixed_code_bits(bl) + extra;
    let raw = end - bl.start;
    let stored_bits = (raw as u64 + 5 * (raw as u64 / 65535 + 1)) * 8 + 8;
    let bfinal = last as u32;
    if stored_bits < dyn_bits.min(fixed_bits) {
        let mut s = bl.start;
        loop {
            let len = if end - s > 65535 { 65535 } else { end - s };
            let fin = bfinal != 0 && s + len == end;
            w.put(fin as u32, 1);
            w.put(0, 2);
            w.align();
            w.out.extend_from_slice(&(len as u16).to_le_bytes());
            w.out.extend_from_slice(&(!(len as u16)).to_le_bytes());
            w.out.extend_from_slice(&data[s..s + len]);
            s += len;
            if s >= end {
                break;
            }
        }
    } else if fixed_bits <= dyn_bits {
        w.put(bfinal, 1);
        w.put(1, 2);
        let (fl, fd) = fixed_lengths();
        write_syms(w, bl, &fl, &fd);
    } else {
        w.put(bfinal, 1);
        w.put(2, 2);
        w.put((hlit - 257) as u32, 5);
        w.put((hdist - 1) as u32, 5);
        w.put((hclen - 4) as u32, 4);
        for k in 0..hclen {
            w.put(cll[CL_ORDER[k]] as u32, 3);
        }
        let mut clc = [0u32; 19];
        build_codes(&cll, &mut clc);
        for (c, x) in rle.iter() {
            let code = clc[*c as usize];
            w.put(code & 0xffff, code >> 16);
            match *c {
                16 => w.put(*x as u32, 2),
                17 => w.put(*x as u32, 3),
                18 => w.put(*x as u32, 7),
                _ => {}
            }
        }
        write_syms(w, bl, &ll, &dl);
    }
    bl.syms.clear();
    bl.lfreq = [0; 286];
    bl.dfreq = [0; 30];
    bl.start = end;
}

fn write_syms(w: &mut Bits<'_>, bl: &Block, ll: &[u8], dl: &[u8]) {
    let mut lc = [0u32; 286];
    let mut dc = [0u32; 30];
    build_codes(ll, &mut lc);
    build_codes(dl, &mut dc);
    for s in bl.syms.iter() {
        let s = *s;
        let dist = s >> 9;
        if dist == 0 {
            let c = lc[(s & 255) as usize];
            w.put(c & 0xffff, c >> 16);
        } else {
            let (li, le, lx) = len_code(s & 511);
            let c = lc[257 + li];
            w.put(c & 0xffff, c >> 16);
            if le > 0 {
                w.put(lx, le);
            }
            let (di, de, dx) = dist_code(dist);
            let c = dc[di];
            w.put(c & 0xffff, c >> 16);
            if de > 0 {
                w.put(dx, de);
            }
        }
    }
    let c = lc[256];
    w.put(c & 0xffff, c >> 16);
}
