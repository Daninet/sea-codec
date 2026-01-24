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
    fn analyze(&self, samples: &[i16], total_items: usize) -> Vec<u8> {
        let (t, mut counts) = Self::interpolate_distribution(total_items, self.vbr_target_bitrate);

        let mut residuals = Vec::with_capacity(total_items);
        let mut current_lms = self.base_encoder.lms[0].clone(); // Assume single channel
        let mut current_scalefactor = self.base_encoder.prev_scalefactor[0]; // Track scalefactor to avoid state divergence

        let mut decision_idx = 0;
        let chunk_len = self.scale_factor_frames as usize;

        // Reuse buffers
        let mut dummy_res_bits = vec![0u8; chunk_len];
        let mut dummy_curr_res = vec![0u8; chunk_len];

        while decision_idx < total_items {
            let remaining = total_items - decision_idx;

            // 1. Calculate errors for all remaining chunks using 't'
            let errors = self.calculate_simulation_errors(
                samples,
                decision_idx,
                remaining,
                t,
                &current_lms,
                current_scalefactor,
                chunk_len,
                &mut dummy_res_bits,
                &mut dummy_curr_res,
            );

            // 2. Sort errors and allocate budget
            let local_assignments =
                self.allocate_residuals_based_on_errors(&errors, remaining, t, &counts);

            // 3. Finalize first 16 (or fewer)
            let finalize_count = remaining.min(16);

            for k in 0..finalize_count {
                let assigned = local_assignments[k];
                residuals.push(assigned);

                // Decrement budget
                self.decrement_budget(&mut counts, assigned, t);

                // Update LMS state
                let abs_decision = decision_idx + k;
                let start_sample = abs_decision * chunk_len;
                let end_sample = (start_sample + chunk_len).min(samples.len());
                let chunk_samples = &samples[start_sample..end_sample];

                // Use the standard encoder logic to advance state
                let dqt = self.base_encoder.dequant_tab.get_dqt(assigned as usize);
                let recip = self
                    .base_encoder
                    .dequant_tab
                    .get_scalefactor_reciprocals(assigned as usize);

                // Track scalefactor to ensure state matches the real encoder pass later
                let (_, best_lms, best_sf) = self.base_encoder.get_residuals_with_best_scalefactor(
                    1, // channels=1
                    dqt,
                    recip,
                    chunk_samples,
                    current_scalefactor,
                    &current_lms,
                    SeaResidualSize::from(assigned),
                    &mut dummy_res_bits,
                    &mut dummy_curr_res,
                );

                current_lms = best_lms;
                current_scalefactor = best_sf;
            }

            decision_idx += finalize_count;
        }

        residuals
    }

    #[allow(clippy::too_many_arguments)]
    fn calculate_simulation_errors(
        &self,
        samples: &[i16],
        decision_offset: usize,
        count: usize,
        t: i32,
        base_lms: &SeaLMS,
        start_scalefactor: i32,
        chunk_len: usize,
        buf_bits: &mut [u8],
        buf_res: &mut [u8],
    ) -> Vec<(usize, u64)> {
        let mut errors = Vec::with_capacity(count);
        let mut sim_lms = base_lms.clone();
        let mut sim_scalefactor = start_scalefactor;

        // Prepare T tables
        let t_size = SeaResidualSize::from(t as u8);
        let dqt = self.base_encoder.dequant_tab.get_dqt(t as usize);
        let recip = self
            .base_encoder
            .dequant_tab
            .get_scalefactor_reciprocals(t as usize);

        for k in 0..count {
            let abs_decision = decision_offset + k;
            let start_sample = abs_decision * chunk_len;
            let end_sample = (start_sample + chunk_len).min(samples.len());
            let chunk_samples = &samples[start_sample..end_sample];

            let (rank, next_lms, best_sf) = self.base_encoder.get_residuals_with_best_scalefactor(
                1,
                dqt,
                recip,
                chunk_samples,
                sim_scalefactor,
                &sim_lms,
                t_size,
                buf_bits,
                buf_res,
            );

            errors.push((k, rank));
            sim_lms = next_lms;
            sim_scalefactor = best_sf;
        }
        errors
    }

    fn allocate_residuals_based_on_errors(
        &self,
        errors: &[(usize, u64)],
        count: usize,
        t: i32,
        counts: &[usize; 4],
    ) -> Vec<u8> {
        // Sort indices by error descending
        let mut sorted_indices: Vec<usize> = (0..count).collect();
        sorted_indices.sort_by(|&a, &b| errors[b].1.cmp(&errors[a].1));

        let mut assignments = vec![t as u8; count];
        let temp_counts = *counts;

        let buckets = [
            (t + 2, 3), // Highest error gets largest size
            (t + 1, 2),
            (t, 1),
            (t - 1, 0),
        ];

        let mut sorted_ptr = 0;
        for (size_val, bucket_idx) in buckets {
            let available = temp_counts[bucket_idx];
            for _ in 0..available {
                if sorted_ptr >= sorted_indices.len() {
                    break;
                }
                let original_idx = sorted_indices[sorted_ptr];
                assignments[original_idx] = size_val as u8;
                sorted_ptr += 1;
            }
        }

        assignments
    }

    fn decrement_budget(&self, counts: &mut [usize; 4], assigned: u8, t: i32) {
        let idx = match assigned as i32 - t {
            -1 => 0,
            0 => 1,
            1 => 2,
            2 => 3,
            _ => 1,
        };
        if counts[idx] > 0 {
            counts[idx] -= 1;
        }
    }
}

impl SeaEncoderTrait for VbrEncoder {
    fn encode(&mut self, samples: &[i16]) -> EncodedSamples {
        let slice_size = self.scale_factor_frames as usize * self.channels;

        let total_chunks =
            (samples.len() / self.channels).div_ceil(self.scale_factor_frames as usize);

        let mut scale_factors = vec![0u8; total_chunks * self.channels];
        let mut residuals: Vec<u8> = vec![0u8; samples.len()];
        let mut residual_bits: Vec<u8> = vec![0u8; scale_factors.len()];
        let mut ranks = vec![0u64; self.channels];

        let total_decisions = total_chunks * self.channels;
        let selected_residuals = self.analyze(samples, total_decisions);

        let mut decision_idx = 0;
        let mut out_res_offset = 0;
        let mut global_chunk_idx = 0;

        for chunk_samples in samples.chunks(slice_size) {
            let mut current_chunk_decisions = vec![SeaResidualSize::from(2); self.channels];

            for ch in 0..self.channels {
                if decision_idx < selected_residuals.len() {
                    let r = selected_residuals[decision_idx];
                    current_chunk_decisions[ch] = SeaResidualSize::from(r);
                    residual_bits[global_chunk_idx * self.channels + ch] = r;
                    decision_idx += 1;
                }
            }

            self.base_encoder.get_residuals_for_chunk(
                chunk_samples,
                &current_chunk_decisions,
                &mut scale_factors[global_chunk_idx * self.channels..],
                &mut residuals[out_res_offset..],
                &mut ranks,
            );

            out_res_offset += chunk_samples.len();
            global_chunk_idx += 1;
        }

        EncodedSamples {
            scale_factors,
            residuals,
            residual_bits,
        }
    }
}
