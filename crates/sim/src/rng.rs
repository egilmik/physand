/// Small, fast, deterministic PRNG (xorshift32). Same seed → same world.
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        // Scramble the seed and avoid the all-zero state.
        let s = seed.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
        Rng(if s == 0 { 0x1234_5678 } else { s })
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform float in `[0, 1)`.
    #[inline]
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// Uniform integer in `[lo, hi]` (inclusive).
    #[inline]
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.next_u32() % (hi - lo + 1)
    }

    /// `-1` or `1`.
    #[inline]
    pub fn dir(&mut self) -> i32 {
        if self.next_u32() & 1 == 0 {
            -1
        } else {
            1
        }
    }
}
