use super::{common::clamp_i16, dqt::SeaDequantTab, lms::SeaLMS};

const MAX_RESIDUAL_BEAM_WIDTH: usize = 6;
const MAX_PERIOD_FRAMES: usize = 20;
const ADAPTIVE_SECOND_FACTOR_GAP_PERCENT: u64 = 25;

#[derive(Clone)]
struct ResidualPath {
    error: u64,
    lms: SeaLMS,
    trace: u16,
    symbol: usize,
}

#[derive(Clone, Copy)]
struct TraceNode {
    parent: u16,
    symbol: u8,
}

pub(super) struct BeamPeriod<'a> {
    pub(super) input: &'a [i16],
    pub(super) start: usize,
    pub(super) frames: usize,
    pub(super) channel: usize,
    pub(super) initial_lms: &'a SeaLMS,
    pub(super) fallback_lms: &'a SeaLMS,
    pub(super) fallback_residuals: &'a [u8],
}

/// Bounded residual-path refinement used by non-fast VBR effort levels.
pub(super) struct ResidualBeamSearch {
    channels: usize,
    residual_beam_width: usize,
    dequant: SeaDequantTab,
}

impl ResidualBeamSearch {
    pub(super) fn new(channels: usize, scale_factor_bits: usize, beam_width: usize) -> Self {
        Self {
            channels,
            residual_beam_width: beam_width.clamp(1, MAX_RESIDUAL_BEAM_WIDTH),
            dequant: SeaDequantTab::init(scale_factor_bits),
        }
    }
    pub(super) fn refine_period(
        &self,
        period: &BeamPeriod,
        factor: usize,
        width: usize,
    ) -> (u64, SeaLMS, [u8; 20]) {
        let levels = self.dequant.get_dqt(width);
        let mut paths: [Option<ResidualPath>; MAX_RESIDUAL_BEAM_WIDTH] =
            core::array::from_fn(|_| None);
        paths[0] = Some(ResidualPath {
            error: 0,
            lms: period.initial_lms.clone(),
            trace: u16::MAX,
            symbol: 0,
        });
        let mut path_count = 1;
        let mut trace = [TraceNode {
            parent: u16::MAX,
            symbol: 0,
        }; MAX_RESIDUAL_BEAM_WIDTH * 2 * MAX_PERIOD_FRAMES];
        let mut trace_count = 0;

        for frame in 0..period.frames {
            let sample = period.input[period.start + frame * self.channels + period.channel];
            let mut candidates: [Option<ResidualPath>; MAX_RESIDUAL_BEAM_WIDTH * 2] =
                core::array::from_fn(|_| None);
            let mut candidate_count = 0;
            for path in paths[..path_count].iter().flatten() {
                let Some(prediction) = Self::safe_predict(&path.lms) else {
                    continue;
                };
                debug_assert_eq!(prediction, path.lms.predict());
                let nearest = Self::nearest_two_symbols(&levels[factor], prediction, sample);
                for (error, symbol, reconstructed, decoded) in nearest.into_iter().flatten() {
                    let mut next = path.clone();
                    next.error = next.error.saturating_add(error);
                    if !Self::safe_update(&mut next.lms, reconstructed, decoded) {
                        continue;
                    }
                    let mut decoder_lms = path.lms.clone();
                    decoder_lms.update(reconstructed, decoded);
                    debug_assert_eq!(next.lms.history, decoder_lms.history);
                    debug_assert_eq!(next.lms.weights, decoder_lms.weights);
                    trace[trace_count] = TraceNode {
                        parent: path.trace,
                        symbol: symbol as u8,
                    };
                    next.trace = trace_count as u16;
                    trace_count += 1;
                    next.symbol = symbol;
                    candidates[candidate_count] = Some(next);
                    candidate_count += 1;
                }
            }

            paths.fill(None);
            path_count = 0;
            for _ in 0..self.residual_beam_width {
                let mut best: Option<usize> = None;
                for (index, candidate) in candidates[..candidate_count].iter().enumerate() {
                    if candidate.as_ref().is_some_and(|value| {
                        best.is_none_or(|old| {
                            let old = candidates[old].as_ref().unwrap();
                            (value.error, value.symbol) < (old.error, old.symbol)
                        })
                    }) {
                        best = Some(index);
                    }
                }
                let Some(index) = best else {
                    break;
                };
                paths[path_count] = candidates[index].take();
                path_count += 1;
            }
        }
        if let Some(winner) = paths.iter_mut().find_map(|path| {
            path.take()
                .filter(|path| Self::safe_predict(&path.lms).is_some())
        }) {
            let mut residuals = [0u8; MAX_PERIOD_FRAMES];
            let mut node_index = winner.trace as usize;
            for residual in residuals[..period.frames].iter_mut().rev() {
                let node = trace[node_index];
                *residual = node.symbol;
                node_index = node.parent as usize;
            }
            return (winner.error, winner.lms, residuals);
        }

        let mut residuals = [0u8; MAX_PERIOD_FRAMES];
        for (frame, residual) in residuals.iter_mut().take(period.frames).enumerate() {
            *residual = period.fallback_residuals[frame * self.channels + period.channel];
        }
        (u64::MAX, period.fallback_lms.clone(), residuals)
    }

    fn factor_sse(
        &self,
        input: &[i16],
        frames: usize,
        channel: usize,
        factor: usize,
        width: usize,
        initial_lms: &SeaLMS,
    ) -> Option<u64> {
        let levels = self.dequant.get_dqt(width);
        let mut lms = initial_lms.clone();
        let mut error = 0u64;
        for frame in 0..frames {
            let prediction = Self::safe_predict(&lms)?;
            let sample = input[frame * self.channels + channel];
            let nearest =
                Self::nearest_two_symbols(&levels[factor], prediction, sample)[0].unwrap();
            error = error.saturating_add(nearest.0);
            if !Self::safe_update(&mut lms, nearest.2, nearest.3) {
                return None;
            }
        }
        Self::safe_predict(&lms)?;
        Some(error)
    }

    pub(super) fn ambiguous_neighbor_factor(
        &self,
        input: &[i16],
        frames: usize,
        channel: usize,
        factor: usize,
        width: usize,
        initial_lms: &SeaLMS,
    ) -> Option<usize> {
        let current = self.factor_sse(input, frames, channel, factor, width, initial_lms)?;
        let mut second: Option<(usize, u64)> = None;
        for candidate in [
            factor.checked_sub(1),
            (factor + 1 < self.dequant.get_dqt(width).len()).then_some(factor + 1),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(error) =
                self.factor_sse(input, frames, channel, candidate, width, initial_lms)
            {
                if second.is_none_or(|(_, best)| error < best) {
                    second = Some((candidate, error));
                }
            }
        }
        second.and_then(|(candidate, error)| {
            (error.saturating_sub(current).saturating_mul(100)
                <= current.saturating_mul(ADAPTIVE_SECOND_FACTOR_GAP_PERCENT))
            .then_some(candidate)
        })
    }

    /// Reject speculative paths that would overflow the decoder's signed LMS
    /// arithmetic.  The ordinary encoder never needs this guard because its
    /// scalar quantizer naturally avoids those paths; a beam explicitly sees
    /// the second-nearest symbol as well.
    #[inline(always)]
    fn safe_predict(lms: &SeaLMS) -> Option<i32> {
        // Beam updates keep both arrays in the signed 16-bit domain, so four
        // products cannot overflow this i64 accumulator.
        let prediction = lms.weights[0] as i64 * lms.history[0] as i64
            + lms.weights[1] as i64 * lms.history[1] as i64
            + lms.weights[2] as i64 * lms.history[2] as i64
            + lms.weights[3] as i64 * lms.history[3] as i64;
        i32::try_from(prediction).ok().map(|value| value >> 13)
    }

    #[inline(always)]
    fn safe_update(lms: &mut SeaLMS, sample: i16, residual: i32) -> bool {
        let delta = residual >> 4;
        let mut weights = lms.weights;
        for (weight, &history) in weights.iter_mut().zip(&lms.history) {
            let adjustment = if history < 0 { -delta } else { delta };
            let next = *weight + adjustment;
            if !(i16::MIN as i32..=i16::MAX as i32).contains(&next) {
                return false;
            }
            *weight = next;
        }
        lms.weights = weights;
        lms.history.copy_within(1.., 0);
        lms.history[3] = sample as i32;
        true
    }

    /// Finds the two nearest decoded residuals.  The legacy codebook stores
    /// +/- pairs in ascending magnitude order, allowing an exact small-window
    /// search whenever no decoded value can saturate the reconstructed sample.
    #[inline(always)]
    pub(super) fn nearest_two_symbols(
        levels: &[i32],
        prediction: i32,
        sample: i16,
    ) -> [Option<(u64, usize, i16, i32)>; 2] {
        let max_magnitude = levels[levels.len() - 2];
        let unclipped = prediction - max_magnitude >= i16::MIN as i32
            && prediction + max_magnitude <= i16::MAX as i32;
        let target = (sample as i32 - prediction).unsigned_abs() as i32;
        let pair_count = levels.len() / 2;
        let mut first = 0usize;
        let mut last = pair_count;
        while first < last {
            let middle = (first + last) / 2;
            if levels[middle * 2] < target {
                first = middle + 1;
            } else {
                last = middle;
            }
        }
        let (start, end) = if unclipped {
            // With no reconstruction clipping, only the magnitudes directly
            // around the insertion point can be among the nearest two.
            (first.saturating_sub(2), (first + 2).min(pair_count))
        } else {
            (0, pair_count)
        };
        let mut nearest: [Option<(u64, usize, i16, i32)>; 2] = [None, None];
        for pair in start..end {
            for symbol in [pair * 2, pair * 2 + 1] {
                let decoded = levels[symbol];
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
        }
        nearest
    }
}
