use alloc::vec;
use alloc::vec::Vec;

use crate::{
    codec::{common::SeaResidualSize, lms::LMS_LEN},
    encoder::EncoderSettings,
};

use super::{
    common::{EncodedSamples, SeaEncoderTrait},
    encoder_base::EncoderBase,
    file::SeaFileHeader,
    lms::SeaLMS,
};

pub struct VbrEncoder {
    channels: usize,
    scale_factor_frames: u8,
    vbr_target_bitrate: f32,
    base_encoder: EncoderBase,
}

impl VbrEncoder {
    pub fn new(file_header: &SeaFileHeader, encoder_settings: &EncoderSettings) -> Self {
        VbrEncoder {
            channels: file_header.channels as usize,
            scale_factor_frames: encoder_settings.scale_factor_frames,
            base_encoder: EncoderBase::new(
                file_header.channels as usize,
                encoder_settings.scale_factor_bits as usize,
            ),
            vbr_target_bitrate: Self::get_normalized_vbr_bitrate(encoder_settings),
        }
    }

    pub fn get_lms(&self) -> &Vec<SeaLMS> {
        &self.base_encoder.lms
    }

    fn get_normalized_vbr_bitrate(encoder_settings: &EncoderSettings) -> f32 {
        let mut vbr_bitrate = encoder_settings.residual_bits;

        // compensate lms
        vbr_bitrate -= (LMS_LEN as f32 * 16.0 * 2.0) / encoder_settings.frames_per_chunk as f32;

        // compensate scale factor data
        vbr_bitrate -=
            encoder_settings.scale_factor_bits as f32 / encoder_settings.scale_factor_frames as f32;

        // compensate vbr data
        vbr_bitrate -= 2.0 / encoder_settings.scale_factor_frames as f32;
        vbr_bitrate
    }

    // returns (base_residual, items count [base-1, base, base+1, base+2])
    fn interpolate_distribution(items: usize, target_rate: f32) -> (i32, [usize; 4]) {
        if target_rate < 2.0 || target_rate > 6.0 {
            panic!("target must be in [2, 6]");
        }

        // 1. Detect the "Danger Zone" using modulo
        // This occurs when the decimal part is between 0.3 and 0.5
        let frac = target_rate.fract();
        let is_transition_zone = frac >= 0.3 && frac < 0.5;

        let pc: f32;
        let pd: f32;
        let offset: f32;
        let t: i32;

        if is_transition_zone {
            // Boosted offsets to pull the mean up for targets like X.4
            pc = 0.25;
            pd = 0.125;
            t = target_rate.floor() as i32;
        } else {
            // Standard offsets
            pc = 0.15;
            pd = 0.075;
            t = target_rate.round() as i32;
        }
        offset = (1.0 * pc) + (2.0 * pd);

        // 2. Calculate percentages
        // Derived Mean Formula: pa = t + offset - target
        let mut pa = (t as f32) + offset - target_rate;
        let mut pb = (1.0 - pc - pd) - pa;

        // 3. Safety Clamping (Prevents negatives in extreme edge cases)
        let remaining_pct = 1.0 - pc - pd;
        if pa < 0.0 {
            pa = 0.0;
            pb = remaining_pct;
        }
        if pb < 0.0 {
            pb = 0.0;
            pa = remaining_pct;
        }

        // 4. Convert to absolute counts
        let total_f = items as f32;
        let a = (total_f * pa).floor() as usize;
        let b = (total_f * pb).floor() as usize;
        let c = (total_f * pc).floor() as usize;
        let d = (total_f * pd).floor() as usize;

        // 5. Greedy Remainder Distribution
        let mut counts = [a, b, c, d];
        let weights = [t - 1, t, t + 1, t + 2];
        let current_count_sum: usize = counts.iter().sum();
        let mut rem = items - current_count_sum;
        let target_sum = target_rate * total_f;

        while rem > 0 {
            let current_sum: f32 = weights
                .iter()
                .zip(counts.iter())
                .map(|(w, count)| (*w as f32) * (*count as f32))
                .sum();

            let mut best_bucket = 0;
            let mut min_diff = f32::INFINITY;

            for i in 0..4 {
                let potential_sum = current_sum + (weights[i] as f32);
                let diff = (potential_sum - target_sum).abs();
                if diff < min_diff {
                    min_diff = diff;
                    best_bucket = i;
                }
            }
            counts[best_bucket] += 1;
            rem -= 1;
        }

        println!("res: {:.3} {} {:?}", target_rate, t, counts);

        (t, counts)
    }

    fn choose_residual_len_from_errors(&self, input_len: usize, errors: &[u64]) -> Vec<u8> {
        // we need to ensure that last partial frames are not touched (it would debalance the frame size)
        let sortable_items = input_len / self.scale_factor_frames as usize;

        let mut indices: Vec<u16> = (0..sortable_items as u16).collect();
        indices.sort_unstable_by(|&a, &b| errors[a as usize].cmp(&errors[b as usize]));

        let (base_residual, [minus_one_items, _, plus_one_items, plus_two_items]) =
            Self::interpolate_distribution(sortable_items, self.vbr_target_bitrate);

        let base_residual_bits = base_residual as u8;

        let mut residual_sizes = vec![base_residual_bits; errors.len()];

        for index in indices.iter().take(minus_one_items) {
            residual_sizes[*index as usize] = base_residual_bits - 1;
        }

        for index in indices[(sortable_items - plus_two_items - plus_one_items)..]
            .iter()
            .take(plus_one_items)
        {
            residual_sizes[*index as usize] = base_residual_bits + 1;
        }

        for index in indices[sortable_items - plus_two_items..]
            .iter()
            .take(plus_two_items)
        {
            residual_sizes[*index as usize] = base_residual_bits + 2;
        }

        residual_sizes
    }

    fn analyze(&mut self, input_slice: &[i16]) -> Vec<u8> {
        let analyze_residual_size = SeaResidualSize::from(self.vbr_target_bitrate as u8 + 1);

        let slice_size = self.scale_factor_frames as usize * self.channels;

        let original_lms = self.base_encoder.lms.clone();

        let residual_sizes = vec![analyze_residual_size; self.channels];

        let mut scale_factors = vec![0u8; slice_size];
        let mut residuals: Vec<u8> = vec![0u8; slice_size];

        let mut errors = vec![
            0u64;
            (input_slice.len() / self.channels)
                .div_ceil(self.scale_factor_frames as usize)
                * self.channels
        ];

        for (slice_index, input_slice) in input_slice.chunks(slice_size).enumerate() {
            self.base_encoder.get_residuals_for_chunk(
                input_slice,
                &residual_sizes,
                &mut scale_factors,
                &mut residuals,
                &mut errors[slice_index * self.channels..],
            );
        }

        self.base_encoder.lms = original_lms;

        self.choose_residual_len_from_errors(input_slice.len(), &errors)
    }
}

impl SeaEncoderTrait for VbrEncoder {
    fn encode(&mut self, samples: &[i16]) -> EncodedSamples {
        let mut scale_factors = vec![
            0u8;
            (samples.len() / self.channels)
                .div_ceil(self.scale_factor_frames as usize)
                * self.channels
        ];

        let mut residuals: Vec<u8> = vec![0u8; samples.len()];

        let residual_bits: Vec<u8> = self.analyze(samples);
        println!("residual_bits: {:?}", residual_bits);

        let slice_size = self.scale_factor_frames as usize * self.channels;

        let mut residual_sizes = vec![SeaResidualSize::from(2); self.channels];

        let mut ranks = vec![0u64; self.channels];

        for (slice_index, input_slice) in samples.chunks(slice_size).enumerate() {
            for channel_offset in 0..self.channels {
                residual_sizes[channel_offset] = SeaResidualSize::from(
                    residual_bits[slice_index * self.channels + channel_offset],
                );
            }

            self.base_encoder.get_residuals_for_chunk(
                input_slice,
                &residual_sizes,
                &mut scale_factors[slice_index * self.channels..],
                &mut residuals[slice_index * slice_size..],
                &mut ranks,
            );
        }

        EncodedSamples {
            scale_factors,
            residuals,
            residual_bits,
        }
    }
}
