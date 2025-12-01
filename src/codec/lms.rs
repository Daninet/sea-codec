use alloc::vec::Vec;

pub const LMS_LEN: usize = 4;

#[derive(Debug, Clone)]
pub struct SeaLMS {
    pub history: [i32; LMS_LEN],
    pub weights: [i32; LMS_LEN],
}

const FLOATING_BITS: usize = 3;

impl SeaLMS {
    pub fn new() -> Self {
        Self {
            history: [0; LMS_LEN],
            weights: [0; LMS_LEN],
        }
    }

    pub fn init_vec(channels: u32) -> Vec<SeaLMS> {
        let mut lms_vec = Vec::with_capacity(channels as usize);
        for _ in 0..channels {
            let mut lms = SeaLMS {
                history: [0; LMS_LEN],
                weights: [0; LMS_LEN],
            };
            lms.weights[LMS_LEN - 2] = -(1 << (16 - FLOATING_BITS));
            lms.weights[LMS_LEN - 1] = 1 << (17 - FLOATING_BITS);

            lms_vec.push(lms);
        }
        lms_vec
    }
    pub fn predict(&self) -> i32 {
        let mut prediction: i32 = 0;

        for i in 0..LMS_LEN {
            prediction += self.weights[i] * self.history[i];
        }

        prediction >> (16 - FLOATING_BITS)
    }

    pub fn update(&mut self, sample: i16, residual: i32) {
        let delta = residual >> (FLOATING_BITS + 1);
        for i in 0..LMS_LEN {
            self.weights[i] += if self.history[i] < 0 { -delta } else { delta };
        }

        self.history.copy_within(1.., 0);
        self.history[LMS_LEN - 1] = sample as i32;
    }

    pub fn get_weights_penalty(&self) -> u64 {
        let mut sum: i64 = 0;

        for i in 0..LMS_LEN {
            sum += self.weights[i] as i64 * self.weights[i] as i64;
        }

        let penalty = (sum >> 18) - 0x8ff;
        (penalty.max(0) as u64).pow(2)
    }

    pub fn serialize(&self) -> [u8; LMS_LEN * 4] {
        let mut output = [0u8; LMS_LEN * 4];

        for i in 0..LMS_LEN {
            let history_bytes = self.history[i].to_le_bytes();
            output[i * 2] = history_bytes[0];
            output[i * 2 + 1] = history_bytes[1];

            let weights_bytes = self.weights[i].to_le_bytes();
            output[LMS_LEN * 2 + i * 2] = weights_bytes[0];
            output[LMS_LEN * 2 + i * 2 + 1] = weights_bytes[1];
        }

        output
    }

    pub fn from_bytes(data: &[u8; LMS_LEN * 4]) -> Self {
        let mut history = [0i32; LMS_LEN];
        let mut weights = [0i32; LMS_LEN];

        for i in 0..LMS_LEN {
            let i16_history = i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
            history[i] = i16_history as i32;

            let i16_weights =
                i16::from_le_bytes([data[LMS_LEN * 2 + i * 2], data[LMS_LEN * 2 + i * 2 + 1]]);
            weights[i] = i16_weights as i32;
        }

        SeaLMS { history, weights }
    }
}
