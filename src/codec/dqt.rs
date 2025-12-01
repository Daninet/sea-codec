use core::array;

use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, PartialEq)]
pub struct SeaDequantTab {
    scale_factor_bits: usize,

    cached_reciprocals: [Vec<i32>; 9],
    cached_dqt: [Vec<Vec<i32>>; 9],
}

// scale_factors along with residuals should cover all potential values
// we try to calcualte an exponent for max scalefactor that is efficient given the range ot residuals
// theoretically [12, 11, 10, 9, 8, 7] should be fine, but these numbers perform better over a diverse dataset
pub static IDEAL_POW_FACTOR: [f32; 8] = [12.0, 11.65, 11.20, 10.58, 9.64, 8.75, 7.66, 6.63]; // were found experimentally

impl SeaDequantTab {
    pub fn init(scale_factor_bits: usize) -> Self {
        let mut res = SeaDequantTab {
            scale_factor_bits: 0,
            cached_reciprocals: array::from_fn(|_| Vec::new()),
            cached_dqt: array::from_fn(|_| Vec::new()),
        };

        res.set_scalefactor_bits(scale_factor_bits);

        res
    }

    pub fn set_scalefactor_bits(&mut self, scale_factor_bits: usize) {
        if self.scale_factor_bits == scale_factor_bits {
            return;
        }

        self.scale_factor_bits = scale_factor_bits;
        self.cached_reciprocals =
            array::from_fn(|i| Self::generate_reciprocal(scale_factor_bits, i));
        self.cached_dqt = array::from_fn(|i: usize| Self::generate_dqt(scale_factor_bits, i));
    }

    fn get_ideal_pow_factor(scale_factor_bits: usize, residual_bits: usize) -> f32 {
        IDEAL_POW_FACTOR[residual_bits - 1] / (scale_factor_bits as f32)
    }

    fn calculate_scale_factors(residual_bits: usize, scale_factor_bits: usize) -> Vec<i32> {
        let mut output: Vec<i32> = Vec::new();
        let power_factor = Self::get_ideal_pow_factor(scale_factor_bits, residual_bits);

        let scale_factor_items = 1 << scale_factor_bits;
        for index in 1..=scale_factor_items {
            let value: f32 = libm::powf(index as f32, power_factor);
            output.push(value as i32);
        }

        output
    }

    fn generate_reciprocal(scale_factor_bits: usize, residual_bits: usize) -> Vec<i32> {
        if residual_bits == 0 {
            return vec![];
        }

        let scale_factors = Self::calculate_scale_factors(residual_bits, scale_factor_bits);
        let mut new_reciprocal: Vec<i32> = Vec::with_capacity(scale_factors.len());
        for sf in scale_factors {
            let value = ((1 << 16) as f32 / sf as f32) as i32;
            new_reciprocal.push(value);
        }
        new_reciprocal
    }

    pub fn get_scalefactor_reciprocals(&self, residual_bits: usize) -> &Vec<i32> {
        &self.cached_reciprocals[residual_bits]
    }

    fn gen_dqt_table(residual_bits: usize) -> Vec<f32> {
        match residual_bits {
            1 => return vec![2.0],
            2 => return vec![1.115, 4.0],
            _ => (),
        }

        let start: f32 = 0.75f32;
        let steps = 1 << (residual_bits - 1);
        let end = ((1 << residual_bits) - 1) as f32;
        let step = (end - start) / (steps - 1) as f32;
        let step_floor = libm::floorf(step);

        let mut curve = vec![0.0; steps];
        for (i, item) in curve.iter_mut().enumerate().take(steps).skip(1) {
            let y = 0.5 + i as f32 * step_floor;
            *item = y;
        }

        curve[0] = start;
        curve[steps - 1] = end;
        curve
    }

    fn generate_dqt(scale_factor_bits: usize, residual_bits: usize) -> Vec<Vec<i32>> {
        if residual_bits == 0 {
            return vec![];
        }

        let dqt = Self::gen_dqt_table(residual_bits);

        let scalefactor_items = 1 << scale_factor_bits;

        let mut output: Vec<Vec<i32>> = Vec::new();

        let dqt_items = 2usize.pow(residual_bits as u32 - 1);

        let scale_factors = Self::calculate_scale_factors(residual_bits, scale_factor_bits);

        for s in 0..scalefactor_items {
            output.push(Vec::with_capacity(dqt.len()));

            // zig zag pattern decreases quantization error
            for item in dqt.iter().take(dqt_items) {
                let val = libm::roundf(scale_factors[s] as f32 * item) as i32;
                output[s].push(val);
                output[s].push(-val);
            }
        }

        output
    }

    pub fn get_dqt(&self, residual_bits: usize) -> &Vec<Vec<i32>> {
        &self.cached_dqt[residual_bits]
    }
}
