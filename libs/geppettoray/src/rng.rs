//! Generador PCG32 mínimo: rápido, sin dependencias y reproducible por píxel.

#[derive(Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Rng { state: 0 };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed ^ stream.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniforme en [0, 1)
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}
