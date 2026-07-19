use alloc::{vec, vec::Vec};

use crate::{
    codec::{common::SeaResidualSize, lms::LMS_LEN},
    encoder::EncoderSettings,
};

use super::{
    common::{EncodedSamples, SeaEncoderTrait},
    encoder_base::EncoderBase,
    encoder_vbr_beam::ResidualBeamSearch,
    file::SeaFileHeader,
    lms::SeaLMS,
};

/// Legacy VBR encoder.
///
/// It uses one fixed-width analysis pass to rank periods and one committed
/// encode pass with the resulting width map.  Keeping the search bounded to
/// those two passes is intentional: it preserves the v0.7.0 performance
/// profile while retaining its content-ranked variable-width allocation.
pub struct VbrEncoder {
    channels: usize,
    scale_factor_frames: u8,
    vbr_target_bitrate: f32,
    residual_distribution: [f32; 6],
    fast_mode: bool,
    beam: Option<ResidualBeamSearch>,
    base_encoder: EncoderBase,
}

const STANDARD_RESIDUAL_DISTRIBUTION: [f32; 6] = [0.00, 0.00, 0.90, 0.10, 0.00, 0.00];
const HIGH_RATE_RESIDUAL_DISTRIBUTION: [f32; 6] = [0.00, 0.00, 0.85, 0.15, 0.00, 0.00];
const FAST_RESIDUAL_DISTRIBUTION: [f32; 6] = [0.00, 0.00, 0.95, 0.05, 0.00, 0.00];
const STANDARD_DISTRIBUTION_RATE_CORRECTION: f32 = 0.0016;
const HIGH_RATE_DISTRIBUTION_RATE_CORRECTION: f32 = 0.0008;
impl VbrEncoder {
    pub fn new(file_header: &SeaFileHeader, encoder_settings: &EncoderSettings) -> Self {
        let fast_mode = encoder_settings.vbr_residual_beam_width == 0;
        let high_rate_distribution = (7.0..8.0).contains(&encoder_settings.residual_bits);
        let residual_distribution = if fast_mode {
            FAST_RESIDUAL_DISTRIBUTION
        } else if high_rate_distribution {
            HIGH_RATE_RESIDUAL_DISTRIBUTION
        } else {
            STANDARD_RESIDUAL_DISTRIBUTION
        };
        VbrEncoder {
            channels: file_header.channels as usize,
            scale_factor_frames: encoder_settings.scale_factor_frames,
            base_encoder: EncoderBase::new(
                file_header.channels as usize,
                encoder_settings.scale_factor_bits as usize,
            ),
            vbr_target_bitrate: Self::get_normalized_vbr_bitrate(
                encoder_settings,
                residual_distribution,
                if fast_mode {
                    0.0
                } else if high_rate_distribution {
                    HIGH_RATE_DISTRIBUTION_RATE_CORRECTION
                } else {
                    STANDARD_DISTRIBUTION_RATE_CORRECTION
                },
            ),
            residual_distribution,
            fast_mode,
            beam: (!fast_mode && encoder_settings.residual_bits >= 1.0).then(|| {
                ResidualBeamSearch::new(
                    file_header.channels as usize,
                    encoder_settings.scale_factor_bits as usize,
                    encoder_settings.vbr_residual_beam_width as usize,
                )
            }),
        }
    }

    pub fn get_lms(&self) -> &Vec<SeaLMS> {
        &self.base_encoder.lms
    }

    fn get_normalized_vbr_bitrate(
        encoder_settings: &EncoderSettings,
        distribution: [f32; 6],
        correction: f32,
    ) -> f32 {
        let mut vbr_bitrate = encoder_settings.residual_bits - correction;
        vbr_bitrate -= (LMS_LEN as f32 * 16.0 * 2.0) / encoder_settings.frames_per_chunk as f32;
        vbr_bitrate -=
            encoder_settings.scale_factor_bits as f32 / encoder_settings.scale_factor_frames as f32;
        vbr_bitrate -= 2.0 / encoder_settings.scale_factor_frames as f32;

        let base_residuals = libm::floorf(encoder_settings.residual_bits);
        let distribution_rate = distribution[1] * (base_residuals - 1.0)
            + distribution[2] * base_residuals
            + distribution[3] * (base_residuals + 1.0)
            + distribution[4] * (base_residuals + 2.0);
        vbr_bitrate -= distribution_rate - base_residuals;
        vbr_bitrate
    }

    /// Returns item counts for target-1, target, target+1, and target+2.
    fn interpolate_distribution(&self, items: usize, target_rate: f32) -> [usize; 4] {
        let (frac, _) = libm::modff(target_rate);
        let mut percentages = [0f32; 4];
        for i in 0..4 {
            percentages[i] = self.residual_distribution[i] * frac
                + self.residual_distribution[i + 1] * (1.0 - frac);
        }

        let mut result = [0usize; 4];
        let mut assigned = 0usize;
        while assigned < items {
            let remaining = items - assigned;
            for i in 0..4 {
                let count = (remaining as f32 * percentages[i]) as usize;
                assigned += count;
                result[i] += count;
            }
            if items - assigned == remaining {
                assigned += remaining;
                result[1] += remaining;
            }
        }
        result
    }

    fn choose_residual_len_from_errors(&self, input_len: usize, errors: &[u64]) -> Vec<u8> {
        // Do not redistribute the partial terminal period: it has a different
        // number of samples and would disturb the fixed chunk byte count.
        let sortable_items = input_len / self.scale_factor_frames as usize;
        if self.vbr_target_bitrate < 1.0 {
            return vec![1; errors.len()];
        }
        let mut indices: Vec<u16> = (0..sortable_items as u16).collect();
        indices.sort_unstable_by(|&a, &b| errors[a as usize].cmp(&errors[b as usize]));

        if self.vbr_target_bitrate >= 7.0 {
            let high_items = ((self.vbr_target_bitrate + 0.05 - 7.0).clamp(0.0, 1.0)
                * sortable_items as f32)
                .round() as usize;
            let mut residual_sizes = vec![7; errors.len()];
            for &index in indices.iter().rev().take(high_items) {
                residual_sizes[index as usize] = 8;
            }
            return residual_sizes;
        }

        let [minus_one, _, plus_one, plus_two] =
            self.interpolate_distribution(sortable_items, self.vbr_target_bitrate);
        let base = self.vbr_target_bitrate as u8;
        let mut residual_sizes = vec![base; errors.len()];
        for &index in indices.iter().take(minus_one) {
            residual_sizes[index as usize] = base - 1;
        }
        for &index in indices[sortable_items - plus_two - plus_one..]
            .iter()
            .take(plus_one)
        {
            residual_sizes[index as usize] = base + 1;
        }
        for &index in indices[sortable_items - plus_two..].iter().take(plus_two) {
            residual_sizes[index as usize] = base + 2;
        }
        residual_sizes
    }

    fn analyze(&mut self, input: &[i16]) -> Vec<u8> {
        let analysis_width = SeaResidualSize::from(self.vbr_target_bitrate as u8 + 1);
        let period_samples = self.scale_factor_frames as usize * self.channels;
        let original_lms = self.base_encoder.lms.clone();
        let residual_sizes = vec![analysis_width; self.channels];
        let mut scale_factors = vec![0u8; period_samples];
        let mut residuals = vec![0u8; period_samples];
        let mut errors = vec![
            0u64;
            (input.len() / self.channels)
                .div_ceil(self.scale_factor_frames as usize)
                * self.channels
        ];

        for (period, samples) in input.chunks(period_samples).enumerate() {
            if self.fast_mode {
                self.base_encoder.get_residuals_for_chunk(
                    samples,
                    &residual_sizes,
                    &mut scale_factors,
                    &mut residuals,
                    &mut errors[period * self.channels..],
                );
            } else {
                self.base_encoder.get_residuals_for_chunk_exact_sse(
                    samples,
                    &residual_sizes,
                    &mut scale_factors,
                    &mut residuals,
                    &mut errors[period * self.channels..],
                );
            }
        }
        self.base_encoder.lms = original_lms;
        self.choose_residual_len_from_errors(input.len(), &errors)
    }
}

impl SeaEncoderTrait for VbrEncoder {
    fn encode(&mut self, samples: &[i16]) -> EncodedSamples {
        let period_samples = self.scale_factor_frames as usize * self.channels;
        let periods = (samples.len() / self.channels).div_ceil(self.scale_factor_frames as usize);
        let mut scale_factors = vec![0u8; periods * self.channels];
        let mut residuals = vec![0u8; samples.len()];
        let residual_bits = self.analyze(samples);
        let mut lms = self.base_encoder.lms.clone();
        let mut greedy_factors = vec![0u8; self.channels];
        let mut greedy_residuals = vec![0u8; period_samples];
        let mut residual_sizes = vec![SeaResidualSize::from(2); self.channels];
        let mut ranks = vec![0u64; self.channels];

        for (period, input) in samples.chunks(period_samples).enumerate() {
            for channel in 0..self.channels {
                residual_sizes[channel] =
                    SeaResidualSize::from(residual_bits[period * self.channels + channel]);
            }
            self.base_encoder.lms.clone_from(&lms);
            if self.fast_mode {
                self.base_encoder.get_residuals_for_chunk(
                    input,
                    &residual_sizes,
                    &mut greedy_factors,
                    &mut greedy_residuals,
                    &mut ranks,
                );
            } else {
                self.base_encoder.get_residuals_for_chunk_exact_sse(
                    input,
                    &residual_sizes,
                    &mut greedy_factors,
                    &mut greedy_residuals,
                    &mut ranks,
                );
            }
            let greedy_lms = self.base_encoder.lms.clone();
            for channel in 0..self.channels {
                let width = residual_sizes[channel] as usize;
                let factor = greedy_factors[channel] as usize;
                let Some(beam) = self.beam.as_ref() else {
                    scale_factors[period * self.channels + channel] = factor as u8;
                    lms[channel] = greedy_lms[channel].clone();
                    for frame in 0..(input.len() / self.channels) {
                        residuals[period * period_samples + frame * self.channels + channel] =
                            greedy_residuals[frame * self.channels + channel];
                    }
                    continue;
                };
                let (error, mut next_lms, mut symbols) = beam.refine_period(
                    input,
                    0,
                    input.len() / self.channels,
                    channel,
                    factor,
                    width,
                    &lms[channel],
                    &greedy_lms[channel],
                    &greedy_residuals,
                );
                let mut chosen_factor = factor;
                let mut chosen_error = error;
                if let Some(second_factor) = beam.ambiguous_neighbor_factor(
                    input,
                    input.len() / self.channels,
                    channel,
                    factor,
                    width,
                    &lms[channel],
                ) {
                    let (second_error, second_lms, second_symbols) = beam.refine_period(
                        input,
                        0,
                        input.len() / self.channels,
                        channel,
                        second_factor,
                        width,
                        &lms[channel],
                        &greedy_lms[channel],
                        &greedy_residuals,
                    );
                    if second_error < error {
                        next_lms = second_lms;
                        symbols = second_symbols;
                        chosen_factor = second_factor;
                        chosen_error = second_error;
                    }
                }
                if chosen_error >= ranks[channel] {
                    next_lms = greedy_lms[channel].clone();
                    symbols = core::array::from_fn(|frame| {
                        greedy_residuals[frame * self.channels + channel]
                    });
                    chosen_factor = factor;
                }
                scale_factors[period * self.channels + channel] = chosen_factor as u8;
                lms[channel] = next_lms;
                for frame in 0..(input.len() / self.channels) {
                    residuals[period * period_samples + frame * self.channels + channel] =
                        symbols[frame];
                }
            }
        }
        self.base_encoder.lms = lms;

        EncodedSamples {
            scale_factors,
            residuals,
            residual_bits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::common::clamp_i16;
    use crate::codec::dqt::SeaDequantTab;

    fn exhaustive_nearest_two(
        levels: &[i32],
        prediction: i32,
        sample: i16,
    ) -> [Option<(u64, usize, i16, i32)>; 2] {
        let mut nearest: [Option<(u64, usize, i16, i32)>; 2] = [None, None];
        for (symbol, &decoded) in levels.iter().enumerate() {
            let reconstructed = clamp_i16(prediction + decoded);
            let difference = sample as i64 - reconstructed as i64;
            let candidate = (
                (difference * difference) as u64,
                symbol,
                reconstructed,
                decoded,
            );
            if nearest[0]
                .as_ref()
                .is_none_or(|best| (candidate.0, candidate.1) < (best.0, best.1))
            {
                nearest[1] = nearest[0].take();
                nearest[0] = Some(candidate);
            } else if nearest[1]
                .as_ref()
                .is_none_or(|best| (candidate.0, candidate.1) < (best.0, best.1))
            {
                nearest[1] = Some(candidate);
            }
        }
        nearest
    }

    #[test]
    fn fast_nearest_symbols_match_exhaustive_search() {
        let dequant = SeaDequantTab::init(4);
        for width in 1..=8 {
            for levels in dequant.get_dqt(width) {
                for prediction in [-40_000, -32_768, -20_000, 0, 20_000, 32_767, 40_000] {
                    for sample in [-32_768, -20_000, -1, 0, 1, 20_000, 32_767] {
                        assert_eq!(
                            ResidualBeamSearch::nearest_two_symbols(levels, prediction, sample),
                            exhaustive_nearest_two(levels, prediction, sample),
                            "width={width}, prediction={prediction}, sample={sample}",
                        );
                    }
                }
            }
        }
    }
}
