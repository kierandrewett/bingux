//! A fixed-size bit set over internal document ids.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    words: Vec<u64>,
    len: usize,
}

impl Bitmap {
    pub fn new(len: usize) -> Self {
        Bitmap {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    pub fn full(len: usize) -> Self {
        let mut b = Bitmap {
            words: vec![u64::MAX; len.div_ceil(64)],
            len,
        };
        b.trim();
        b
    }

    fn trim(&mut self) {
        let rem = self.len % 64;
        if rem != 0
            && let Some(last) = self.words.last_mut()
        {
            *last &= (1u64 << rem) - 1;
        }
    }

    /// Appends one bit.
    pub fn push(&mut self, value: bool) {
        if self.len % 64 == 0 {
            self.words.push(0);
        }
        if value {
            self.words[self.len >> 6] |= 1 << (self.len & 63);
        }
        self.len += 1;
    }

    #[inline]
    pub fn get(&self, i: u32) -> bool {
        let i = i as usize;
        i < self.len && self.words[i >> 6] >> (i & 63) & 1 == 1
    }

    #[inline]
    pub fn set(&mut self, i: u32) {
        let i = i as usize;
        debug_assert!(i < self.len);
        self.words[i >> 6] |= 1 << (i & 63);
    }

    pub fn clear(&mut self, i: u32) {
        let i = i as usize;
        if i < self.len {
            self.words[i >> 6] &= !(1 << (i & 63));
        }
    }

    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn and_with(&mut self, other: &Bitmap) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= *b;
        }
    }

    pub fn or_with(&mut self, other: &Bitmap) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= *b;
        }
    }

    pub fn andnot_with(&mut self, other: &Bitmap) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= !*b;
        }
    }

    pub fn not(&mut self) {
        for w in &mut self.words {
            *w = !*w;
        }
        self.trim();
    }

    /// The set bits in increasing order.
    pub fn ones(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(wi, &w)| {
            let mut w = w;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let bit = w.trailing_zeros();
                w &= w - 1;
                Some((wi as u32) * 64 + bit)
            })
        })
    }

    pub fn heap_bytes(&self) -> usize {
        self.words.capacity() * 8
    }
}
