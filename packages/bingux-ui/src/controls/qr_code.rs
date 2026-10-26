//! A QR code: a pure-Rust encoder (byte mode, versions 1 to 15, all four
//! error-correction levels) and the control that draws it.
//!
//! A QR code is scanned by contrast, so the tile is always black modules on
//! a white square with the four-module quiet zone, whatever the theme. The
//! tokens have no plain black or white colour, so the tile takes
//! [`Paint::Knob`] (white in both schemes) and [`Paint::Backdrop`] (black in
//! both schemes).

use freya::prelude::*;

use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;

/// The largest version the encoder builds: 15, 412 bytes at level M.
const MAX_VERSION: usize = 15;
/// Quiet zone around the symbol, in modules (the QR specification's minimum).
const QUIET: usize = 4;
/// The tile's side when the caller gives no size, in px.
const DEFAULT_SIZE: f32 = 176.0;
/// Smallest module that stays crisp and scannable on screen, in px.
const MIN_MODULE: f32 = 2.0;

/// Error-correction level: the share of the symbol that can be damaged and
/// still scan (about 7, 15, 25 and 30 per cent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EccLevel {
    L,
    M,
    Q,
    H,
}

impl EccLevel {
    fn index(self) -> usize {
        match self {
            Self::L => 0,
            Self::M => 1,
            Self::Q => 2,
            Self::H => 3,
        }
    }

    /// The two format-information bits for the level.
    fn format_bits(self) -> u32 {
        match self {
            Self::L => 1,
            Self::M => 0,
            Self::Q => 3,
            Self::H => 2,
        }
    }
}

/// Why a payload could not be encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QrError {
    /// The data does not fit in version 15 at the chosen level.
    TooLong,
}

impl std::fmt::Display for QrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLong => f.write_str("data is too long for a QR code of version 15"),
        }
    }
}

impl std::error::Error for QrError {}

// Error-correction codewords per block and block counts, versions 1..=15,
// indexed [level][version - 1] with levels in L, M, Q, H order.
const ECC_PER_BLOCK: [[usize; MAX_VERSION]; 4] = [
    [7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22],
    [10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24],
    [13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30],
    [17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24],
];
const BLOCKS: [[usize; MAX_VERSION]; 4] = [
    [1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6],
    [1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10],
    [1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12],
    [1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18],
];

/// An encoded QR symbol: a square grid of dark and light modules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrMatrix {
    size: usize,
    modules: Vec<bool>,
}

impl QrMatrix {
    /// Encode `data` in byte mode at the smallest version that fits.
    pub fn encode(data: &[u8], ecc: EccLevel) -> Result<Self, QrError> {
        let level = ecc.index();
        let version = (1..=MAX_VERSION)
            .find(|&v| {
                let count_bits = if v <= 9 { 8 } else { 16 };
                4 + count_bits + data.len() * 8 <= data_codewords(v, level) * 8
                    && (count_bits == 16 || data.len() < 256)
            })
            .ok_or(QrError::TooLong)?;
        let codewords = interleave(&data_stream(data, version, level), version, level);
        Ok(Self::build(&codewords, version, ecc))
    }

    /// Modules per side (21 for version 1, 4 more per version).
    pub fn size(&self) -> usize {
        self.size
    }

    /// The symbol's version, 1 to 15.
    pub fn version(&self) -> usize {
        (self.size - 17) / 4
    }

    /// Whether the module at column `x`, row `y` is dark. Outside the grid is
    /// light, which is what the quiet zone is.
    pub fn dark(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.modules[y * self.size + x]
    }

    fn build(codewords: &[u8], version: usize, ecc: EccLevel) -> Self {
        let size = version * 4 + 17;
        let function = function_mask(version);
        let mut base = vec![false; size * size];
        draw_function_patterns(&mut base, version);
        place_codewords(&mut base, &function, size, codewords);

        let mut best: Option<(u32, Vec<bool>)> = None;
        for mask in 0..8u32 {
            let mut modules = base.clone();
            for y in 0..size {
                for x in 0..size {
                    if !function[y * size + x] && mask_hit(mask, x, y) {
                        modules[y * size + x] ^= true;
                    }
                }
            }
            draw_format(&mut modules, size, ecc, mask);
            let score = penalty(&modules, size);
            if best.as_ref().is_none_or(|(lowest, _)| score < *lowest) {
                best = Some((score, modules));
            }
        }
        // Eight masks were tried, so `best` is always set.
        let modules = best.map(|(_, modules)| modules).unwrap_or(base);
        Self { size, modules }
    }
}

/// Codewords in the symbol, data and error correction together.
fn raw_codewords(version: usize) -> usize {
    let mut modules = (16 * version + 128) * version + 64;
    if version >= 2 {
        let align = version / 7 + 2;
        modules -= (25 * align - 10) * align - 55;
        if version >= 7 {
            modules -= 36;
        }
    }
    modules / 8
}

fn data_codewords(version: usize, level: usize) -> usize {
    raw_codewords(version) - ECC_PER_BLOCK[level][version - 1] * BLOCKS[level][version - 1]
}

/// Mode indicator, length, data, terminator and pad bytes: exactly the
/// version's data codewords.
fn data_stream(data: &[u8], version: usize, level: usize) -> Vec<u8> {
    let capacity = data_codewords(version, level) * 8;
    let count_bits = if version <= 9 { 8 } else { 16 };
    let mut bits: Vec<bool> = Vec::with_capacity(capacity);
    let mut push = |value: u32, count: usize| {
        for i in (0..count).rev() {
            bits.push((value >> i) & 1 == 1);
        }
    };
    push(0b0100, 4);
    push(data.len() as u32, count_bits);
    for &byte in data {
        push(u32::from(byte), 8);
    }
    // Up to four zero bits end the data, fewer when the symbol is full.
    let used = 4 + count_bits + data.len() * 8;
    push(0, 4.min(capacity - used));
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut bytes: Vec<u8> = bits
        .chunks(8)
        .map(|chunk| chunk.iter().fold(0u8, |acc, &bit| acc << 1 | u8::from(bit)))
        .collect();
    for pad in [0xEC, 0x11].into_iter().cycle() {
        if bytes.len() >= capacity / 8 {
            break;
        }
        bytes.push(pad);
    }
    bytes
}

/// Multiply in GF(256) with the QR polynomial x^8 + x^4 + x^3 + x^2 + 1.
fn gf_mul(a: u8, b: u8) -> u8 {
    let mut product = 0u8;
    for i in (0..8).rev() {
        product = (product << 1) ^ ((product >> 7) * 0x1D);
        product ^= ((b >> i) & 1) * a;
    }
    product
}

/// The Reed-Solomon remainder of `data` for `degree` error-correction bytes.
fn reed_solomon(data: &[u8], degree: usize) -> Vec<u8> {
    // Generator: product of (x - 2^i) for i in 0..degree, highest term implied.
    let mut generator = vec![0u8; degree];
    generator[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            generator[j] = gf_mul(generator[j], root);
            if j + 1 < degree {
                generator[j] ^= generator[j + 1];
            }
        }
        root = gf_mul(root, 2);
    }
    let mut remainder = vec![0u8; degree];
    for &byte in data {
        let factor = byte ^ remainder.remove(0);
        remainder.push(0);
        for (r, &g) in remainder.iter_mut().zip(&generator) {
            *r ^= gf_mul(g, factor);
        }
    }
    remainder
}

/// Split the data into blocks, add each block's error correction and
/// interleave them as the specification orders codewords.
fn interleave(data: &[u8], version: usize, level: usize) -> Vec<u8> {
    let blocks = BLOCKS[level][version - 1];
    let ecc_len = ECC_PER_BLOCK[level][version - 1];
    let raw = raw_codewords(version);
    let short_blocks = blocks - raw % blocks;
    let short_len = raw / blocks;
    let mut parts: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(blocks);
    let mut at = 0;
    for index in 0..blocks {
        let len = short_len - ecc_len + usize::from(index >= short_blocks);
        let chunk = &data[at..at + len];
        at += len;
        parts.push((chunk.to_vec(), reed_solomon(chunk, ecc_len)));
    }
    let longest = short_len - ecc_len + 1;
    let mut out = Vec::with_capacity(raw);
    for i in 0..longest {
        out.extend(parts.iter().filter_map(|(d, _)| d.get(i).copied()));
    }
    for i in 0..ecc_len {
        out.extend(parts.iter().map(|(_, e)| e[i]));
    }
    out
}

/// Alignment pattern centres along one axis.
fn alignment_positions(version: usize) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let count = version / 7 + 2;
    let size = version * 4 + 17;
    let step = (version * 4 + count * 2 + 1) / (count * 2 - 2) * 2;
    let mut positions = vec![6];
    positions.extend((0..count - 1).rev().map(|i| size - 7 - i * step));
    positions
}

/// Centres of every alignment pattern that is drawn (those that would sit on
/// a finder pattern are skipped).
fn alignment_centres(version: usize) -> Vec<(usize, usize)> {
    let positions = alignment_positions(version);
    let last = positions.len().saturating_sub(1);
    let mut centres = Vec::new();
    for (i, &y) in positions.iter().enumerate() {
        for (j, &x) in positions.iter().enumerate() {
            let on_finder = (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0);
            if !on_finder {
                centres.push((x, y));
            }
        }
    }
    centres
}

/// Which modules belong to patterns and information rather than data.
fn function_mask(version: usize) -> Vec<bool> {
    let size = version * 4 + 17;
    let mut mask = vec![false; size * size];
    let mut fill = |x0: usize, y0: usize, w: usize, h: usize| {
        for y in y0..(y0 + h).min(size) {
            for x in x0..(x0 + w).min(size) {
                mask[y * size + x] = true;
            }
        }
    };
    // Finders with separators, and the format areas beside them.
    fill(0, 0, 9, 9);
    fill(size - 8, 0, 8, 9);
    fill(0, size - 8, 9, 8);
    // Timing lines.
    fill(6, 0, 1, size);
    fill(0, 6, size, 1);
    for (cx, cy) in alignment_centres(version) {
        fill(cx - 2, cy - 2, 5, 5);
    }
    if version >= 7 {
        fill(size - 11, 0, 3, 6);
        fill(0, size - 11, 6, 3);
    }
    mask
}

/// Finder, timing and alignment patterns, and the version information.
fn draw_function_patterns(modules: &mut [bool], version: usize) {
    let size = version * 4 + 17;
    let mut set = |x: usize, y: usize, dark: bool| modules[y * size + x] = dark;
    for i in 0..size {
        set(6, i, i % 2 == 0);
        set(i, 6, i % 2 == 0);
    }
    // Dark outer ring and 3x3 core around a light ring.
    for (ox, oy) in [(0, 0), (size - 7, 0), (0, size - 7)] {
        for dy in 0..7usize {
            for dx in 0..7usize {
                let outer = dx == 0 || dx == 6 || dy == 0 || dy == 6;
                let core = (2..=4).contains(&dx) && (2..=4).contains(&dy);
                set(ox + dx, oy + dy, outer || core);
            }
        }
    }
    // Separators are light; the loops above never touch them, and the
    // buffer starts light, so nothing more is needed.
    for (cx, cy) in alignment_centres(version) {
        for dy in 0..5usize {
            for dx in 0..5usize {
                let edge = dx == 0 || dx == 4 || dy == 0 || dy == 4;
                set(cx - 2 + dx, cy - 2 + dy, edge || (dx == 2 && dy == 2));
            }
        }
    }
    if version >= 7 {
        let mut rem = version as u32;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        let bits = (version as u32) << 12 | rem;
        for i in 0..18 {
            let dark = (bits >> i) & 1 == 1;
            let (a, b) = (size - 11 + i % 3, i / 3);
            set(a, b, dark);
            set(b, a, dark);
        }
    }
}

/// The 15 format bits for a level and mask, BCH-protected and XOR-masked.
fn format_bits(ecc: EccLevel, mask: u32) -> u32 {
    let data = ecc.format_bits() << 3 | mask;
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
    }
    (data << 10 | rem) ^ 0x5412
}

/// Write both copies of the format information and the fixed dark module.
fn draw_format(modules: &mut [bool], size: usize, ecc: EccLevel, mask: u32) {
    let bits = format_bits(ecc, mask);
    let bit = |i: usize| (bits >> i) & 1 == 1;
    let mut set = |x: usize, y: usize, dark: bool| modules[y * size + x] = dark;
    for i in 0..6 {
        set(8, i, bit(i));
    }
    set(8, 7, bit(6));
    set(8, 8, bit(7));
    set(7, 8, bit(8));
    for i in 9..15 {
        set(14 - i, 8, bit(i));
    }
    for i in 0..8 {
        set(size - 1 - i, 8, bit(i));
    }
    for i in 8..15 {
        set(8, size - 15 + i, bit(i));
    }
    set(8, size - 8, true);
}

/// Lay the codeword bits in the two-column zigzag, skipping function modules.
fn place_codewords(modules: &mut [bool], function: &[bool], size: usize, codewords: &[u8]) {
    let total = codewords.len() * 8;
    let mut i = 0;
    let mut right = size as isize - 1;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        for vert in 0..size {
            for j in 0..2 {
                let x = (right - j) as usize;
                let upward = ((right + 1) & 2) == 0;
                let y = if upward { size - 1 - vert } else { vert };
                if !function[y * size + x] && i < total {
                    modules[y * size + x] = (codewords[i / 8] >> (7 - i % 8)) & 1 == 1;
                    i += 1;
                }
            }
        }
        right -= 2;
    }
}

fn mask_hit(mask: u32, x: usize, y: usize) -> bool {
    let (x, y) = (x as u32, y as u32);
    match mask {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => (x / 3 + y / 2) % 2 == 0,
        5 => x * y % 2 + x * y % 3 == 0,
        6 => (x * y % 2 + x * y % 3) % 2 == 0,
        _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
    }
}

/// The specification's four penalty rules, summed.
fn penalty(modules: &[bool], size: usize) -> u32 {
    let at = |x: usize, y: usize| modules[y * size + x];
    let mut score = 0u32;
    // Rule 1 (runs of five or more) and rule 3 (finder-like 1:1:3:1:1 runs),
    // along rows then columns.
    const FINDER: [bool; 7] = [true, false, true, true, true, false, true];
    for transpose in [false, true] {
        let line = |a: usize, b: usize| if transpose { at(a, b) } else { at(b, a) };
        for a in 0..size {
            let mut run = 1;
            for b in 1..size {
                if line(a, b) == line(a, b - 1) {
                    run += 1;
                    if run == 5 {
                        score += 3;
                    } else if run > 5 {
                        score += 1;
                    }
                } else {
                    run = 1;
                }
            }
            for b in 0..size.saturating_sub(6) {
                if (0..7).all(|k| line(a, b + k) == FINDER[k]) {
                    let light_before = b >= 4 && (b - 4..b).all(|k| !line(a, k));
                    let light_after = b + 11 <= size && (b + 7..b + 11).all(|k| !line(a, k));
                    if light_before || light_after {
                        score += 40;
                    }
                }
            }
        }
    }
    // Rule 2: 2x2 blocks of one colour.
    for y in 0..size - 1 {
        for x in 0..size - 1 {
            let c = at(x, y);
            if c == at(x + 1, y) && c == at(x, y + 1) && c == at(x + 1, y + 1) {
                score += 3;
            }
        }
    }
    // Rule 4: balance of dark and light.
    let total = (size * size) as u32;
    let dark = modules.iter().filter(|&&m| m).count() as u32;
    let steps = ((dark * 20).abs_diff(total * 10) + total - 1) / total;
    score + 10 * steps.saturating_sub(1)
}

/// A QR code of `data`, drawn black on white. See the module docs.
pub fn qr_code(data: impl Into<String>) -> QrCode {
    QrCode {
        data: data.into(),
        ecc: EccLevel::M,
        label: None,
        size: DEFAULT_SIZE,
    }
}

#[derive(PartialEq)]
pub struct QrCode {
    data: String,
    ecc: EccLevel,
    label: Option<String>,
    size: f32,
}

impl QrCode {
    /// Error-correction level; the default is `M`.
    pub fn ecc(mut self, ecc: EccLevel) -> Self {
        self.ecc = ecc;
        self
    }

    /// What a screen reader says for the code, e.g. "QR code for network Home".
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The tile's side in px, quiet zone included. The tile rounds down to a
    /// whole number of px per module so the modules stay crisp.
    pub fn size(mut self, px: f32) -> Self {
        self.size = px;
        self
    }
}

impl Component for QrCode {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let matrix = QrMatrix::encode(self.data.as_bytes(), self.ecc).ok();
        let modules = matrix.as_ref().map_or(21, QrMatrix::size) + 2 * QUIET;
        let module = (self.size / modules as f32).floor().max(MIN_MODULE);
        let side = module * modules as f32;
        let mut tile = rect()
            .component("QrCode")
            .width(Size::px(side))
            .height(Size::px(side))
            .tw(
                &tokens,
                &[rounded(Radius::Control), bg(Paint::Knob), clip()],
            )
            .a11y_role(AccessibilityRole::Image);
        if let Some(label) = &self.label {
            tile = tile.a11y_alt(label.clone());
        }
        // Data too long for version 15 leaves the white tile empty: a code
        // that does not scan is better than a wrong one.
        let Some(matrix) = matrix else {
            return tile;
        };
        let ink = Paint::Backdrop.color(&tokens);
        let dark = format!("#{:02x}{:02x}{:02x}", ink.r(), ink.g(), ink.b());
        let mut path = String::new();
        for y in 0..matrix.size() {
            let mut x = 0;
            while x < matrix.size() {
                if !matrix.dark(x, y) {
                    x += 1;
                    continue;
                }
                let start = x;
                while matrix.dark(x, y) {
                    x += 1;
                }
                // One rectangle per horizontal run keeps the path short.
                path.push_str(&format!(
                    "M{} {}h{}v{module}h-{}z",
                    (start + QUIET) as f32 * module,
                    (y + QUIET) as f32 * module,
                    (x - start) as f32 * module,
                    (x - start) as f32 * module,
                ));
            }
        }
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{side}" height="{side}" viewBox="0 0 {side} {side}"><path d="{path}" fill="{dark}" shape-rendering="crispEdges"/></svg>"#
        );
        tile.child(
            SvgViewer::new((svg.clone(), Bytes::from(svg)))
                .width(Size::px(side))
                .height(Size::px(side)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: [EccLevel; 4] = [EccLevel::L, EccLevel::M, EccLevel::Q, EccLevel::H];

    /// GF(256) exp/log tables for the decoder, built independently of `gf_mul`.
    fn tables() -> ([u8; 512], [usize; 256]) {
        let (mut exp, mut log) = ([0u8; 512], [0usize; 256]);
        let mut x = 1u16;
        for i in 0..255 {
            exp[i] = x as u8;
            log[x as usize] = i;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= 0x11D;
            }
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        (exp, log)
    }

    fn bch_ok(word: u32) -> bool {
        // The format code word is a multiple of the generator once unmasked.
        let mut rem = word ^ 0x5412;
        for i in (10..15).rev() {
            if (rem >> i) & 1 == 1 {
                rem ^= 0x537 << (i - 10);
            }
        }
        rem == 0
    }

    /// Reads the symbol back the way a scanner does and returns its bytes.
    fn decode(m: &QrMatrix) -> Vec<u8> {
        let size = m.size();
        let version = m.version();
        let bit = |x: usize, y: usize| u32::from(m.dark(x, y));
        // Copy one of the format information.
        let mut first = 0u32;
        let cells: Vec<(usize, usize)> = (0..6)
            .map(|i| (8, i))
            .chain([(8, 7), (8, 8), (7, 8)])
            .chain((9..15).map(|i| (14 - i, 8)))
            .collect();
        for (i, &(x, y)) in cells.iter().enumerate() {
            first |= bit(x, y) << i;
        }
        // Copy two.
        let mut second = 0u32;
        for i in 0..8 {
            second |= bit(size - 1 - i, 8) << i;
        }
        for i in 8..15 {
            second |= bit(8, size - 15 + i) << i;
        }
        assert_eq!(first, second, "format copies differ");
        assert!(bch_ok(first), "format BCH invalid");
        let data = (first ^ 0x5412) >> 10;
        let mask = data & 7;
        let level = match data >> 3 {
            1 => 0,
            0 => 1,
            3 => 2,
            _ => 3,
        };

        let function = function_mask(version);
        let mut bits = Vec::new();
        let mut right = size as isize - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..size {
                for j in 0..2 {
                    let x = (right - j) as usize;
                    let y = if ((right + 1) & 2) == 0 {
                        size - 1 - vert
                    } else {
                        vert
                    };
                    if !function[y * size + x] {
                        bits.push(m.dark(x, y) ^ mask_hit(mask, x, y));
                    }
                }
            }
            right -= 2;
        }
        let raw = raw_codewords(version);
        assert!(bits.len() >= raw * 8 && bits.len() < raw * 8 + 8);
        let words: Vec<u8> = bits[..raw * 8]
            .chunks(8)
            .map(|c| c.iter().fold(0u8, |a, &b| a << 1 | u8::from(b)))
            .collect();

        // De-interleave.
        let blocks = BLOCKS[level][version - 1];
        let ecc_len = ECC_PER_BLOCK[level][version - 1];
        let short_blocks = blocks - raw % blocks;
        let short_len = raw / blocks;
        let data_len = |b: usize| short_len - ecc_len + usize::from(b >= short_blocks);
        let mut block_data: Vec<Vec<u8>> = vec![Vec::new(); blocks];
        let mut block_ecc: Vec<Vec<u8>> = vec![Vec::new(); blocks];
        let mut at = 0;
        for i in 0..short_len - ecc_len + 1 {
            for b in 0..blocks {
                if i < data_len(b) {
                    block_data[b].push(words[at]);
                    at += 1;
                }
            }
        }
        for _ in 0..ecc_len {
            for b in 0..blocks {
                block_ecc[b].push(words[at]);
                at += 1;
            }
        }
        assert_eq!(at, raw);

        // Every block's syndromes 2^0..2^(ecc-1) must vanish.
        let (exp, log) = tables();
        let mut stream = Vec::new();
        for b in 0..blocks {
            let mut word = block_data[b].clone();
            word.extend(&block_ecc[b]);
            for s in 0..ecc_len {
                let mut acc = 0u8;
                for &c in &word {
                    acc = if acc == 0 {
                        0
                    } else {
                        exp[log[acc as usize] + s]
                    } ^ c;
                }
                assert_eq!(acc, 0, "RS syndrome {s} of block {b} is not zero");
            }
            stream.extend(&block_data[b]);
        }

        // Byte-mode header.
        let mut reader = stream
            .iter()
            .flat_map(|b: &u8| (0..8).rev().map(move |i| (*b >> i) & 1u8));
        let mut take =
            |n: usize| (0..n).fold(0usize, |a, _| a << 1 | usize::from(reader.next().unwrap()));
        assert_eq!(take(4), 0b0100);
        let len = take(if version <= 9 { 8 } else { 16 });
        (0..len).map(|_| take(8) as u8).collect()
    }

    #[test]
    fn codeword_table_matches_the_specification() {
        // Data codewords from the QR specification's capacity table.
        let known = [
            (1, [19, 16, 13, 9]),
            (2, [34, 28, 22, 16]),
            (5, [108, 86, 62, 46]),
            (7, [156, 124, 88, 66]),
            (10, [274, 216, 154, 122]),
        ];
        for (version, counts) in known {
            for (level, count) in counts.into_iter().enumerate() {
                assert_eq!(
                    data_codewords(version, level),
                    count,
                    "v{version} level {level}"
                );
            }
        }
        assert_eq!(raw_codewords(1), 26);
        assert_eq!(raw_codewords(7), 196);
        assert_eq!(raw_codewords(15), 655);
        // Version 15 at L: six blocks of 22 error-correction codewords.
        assert_eq!(data_codewords(15, 0), 523);
    }

    #[test]
    fn finder_timing_and_format_structure() {
        for text in ["HELLO WORLD", "https://example.com"] {
            for ecc in LEVELS {
                let m = QrMatrix::encode(text.as_bytes(), ecc).unwrap();
                let n = m.size();
                for (ox, oy) in [(0, 0), (n - 7, 0), (0, n - 7)] {
                    for dy in 0..7usize {
                        for dx in 0..7usize {
                            let outer = dx == 0 || dx == 6 || dy == 0 || dy == 6;
                            let core = (2..=4).contains(&dx) && (2..=4).contains(&dy);
                            assert_eq!(m.dark(ox + dx, oy + dy), outer || core);
                        }
                    }
                }
                for i in 8..n - 8 {
                    assert_eq!(m.dark(i, 6), i % 2 == 0);
                    assert_eq!(m.dark(6, i), i % 2 == 0);
                }
                assert!(m.dark(8, n - 8), "fixed dark module");
            }
        }
    }

    #[test]
    fn round_trips_through_a_reference_decoder() {
        let long = "https://example.com/pair?key=".to_string() + &"k3y".repeat(120);
        let wifi = "WIFI:T:WPA;S:Home network;P:correct horse battery staple;;";
        let inputs = [
            "HELLO WORLD",
            "https://example.com",
            wifi,
            "",
            "é✓ unicode",
            &long,
        ];
        for text in inputs {
            for ecc in LEVELS {
                let Ok(m) = QrMatrix::encode(text.as_bytes(), ecc) else {
                    assert!(text.len() > 200, "{text:?} should fit at {ecc:?}");
                    continue;
                };
                assert_eq!(decode(&m), text.as_bytes(), "{text:?} at {ecc:?}");
            }
        }
    }

    #[test]
    fn every_version_round_trips() {
        for version in 1..=MAX_VERSION {
            for (level, ecc) in LEVELS.into_iter().enumerate() {
                // The longest payload that still needs exactly this version.
                let room = data_codewords(version, level) - if version <= 9 { 2 } else { 3 };
                let data: Vec<u8> = (0..room).map(|i| (i * 7 + version) as u8).collect();
                let m = QrMatrix::encode(&data, ecc).unwrap();
                assert_eq!(m.version(), version, "v{version} {ecc:?}");
                assert_eq!(decode(&m), data);
            }
        }
    }

    #[test]
    fn too_long_input_is_an_error() {
        let cap = data_codewords(MAX_VERSION, 0) - 3;
        assert!(QrMatrix::encode(&vec![b'a'; cap], EccLevel::L).is_ok());
        assert_eq!(
            QrMatrix::encode(&vec![b'a'; cap + 1], EccLevel::L),
            Err(QrError::TooLong)
        );
        assert_eq!(
            QrMatrix::encode(&[0; 500], EccLevel::H),
            Err(QrError::TooLong)
        );
    }
}
