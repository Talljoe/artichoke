use crate::extn::core::random::backend::rand::{Rand, Rng};
use crate::extn::core::random::backend::InternalState;
use crate::types::{Fp, Int};

#[derive(Debug)]
pub struct Prng {
    random: Rand<Rng>,
}

impl From<u64> for Prng {
    fn from(seed: u64) -> Self {
        Self {
            random: Rand::new(Some(seed)),
        }
    }
}

impl From<Option<u64>> for Prng {
    fn from(seed: Option<u64>) -> Self {
        Self {
            random: Rand::new(seed),
        }
    }
}

impl Default for Prng {
    #[inline]
    fn default() -> Self {
        Self {
            random: Rand::new(None),
        }
    }
}

impl Prng {
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    #[inline]
    pub fn seed(&self) -> u64 {
        self.random.seed()
    }

    #[inline]
    pub fn reseed(&mut self, new_seed: Option<u64>) {
        self.random = Rand::new(new_seed);
    }

    #[must_use]
    #[inline]
    pub fn internal_state(&self) -> InternalState {
        self.random.internal_state()
    }

    #[inline]
    pub fn bytes(&mut self, buf: &mut [u8]) {
        self.random.bytes(buf);
    }

    #[inline]
    pub fn rand_int(&mut self, max: Int) -> Int {
        self.random.rand_int(max)
    }

    #[inline]
    pub fn rand_float(&mut self, max: Option<Fp>) -> Fp {
        self.random.rand_float(max)
    }
}
