use alloc::vec::Vec;

#[cfg(feature = "resample")]
pub fn resample(input: &[i16], source_rate: u32, target_rate: u32, channels: u32) -> Vec<i16> {
    if source_rate == target_rate {
        return input.to_vec();
    }

    use audioadapter_buffers::direct::InterleavedSlice;
    use rubato::{Fft, FixedSync, Resampler};

    let mut resampler = Fft::<f64>::new(
        source_rate as usize,
        target_rate as usize,
        1024,
        1,
        channels as usize,
        FixedSync::Both,
    )
    .unwrap();

    let nbr_input_frames = input.len() / channels as usize;
    let input_f64: Vec<f64> = input.iter().map(|&s| s as f64 / 32768.0).collect();
    let input_adapter =
        InterleavedSlice::new(&input_f64, channels as usize, nbr_input_frames).unwrap();

    let output_len = resampler.process_all_needed_output_len(nbr_input_frames);
    let mut output_f64 = vec![0.0f64; output_len * channels as usize];
    let mut output_adapter = audioadapter_buffers::direct::InterleavedSlice::new_mut(
        &mut output_f64,
        channels as usize,
        output_len,
    )
    .unwrap();

    let (_read, written) = resampler
        .process_all_into_buffer(&input_adapter, &mut output_adapter, nbr_input_frames, None)
        .unwrap();

    let mut output = Vec::with_capacity(written * channels as usize);
    for i in 0..written {
        for ch in 0..channels as usize {
            let sample =
                (output_f64[i * channels as usize + ch] * 32768.0).clamp(-32768.0, 32767.0) as i16;
            output.push(sample);
        }
    }

    output
}

#[cfg(not(feature = "resample"))]
pub fn resample(_input: &[i16], _source_rate: u32, _target_rate: u32, _channels: u32) -> Vec<i16> {
    // If resample feature is not enabled, just return original but it's better to panic or warn if target differs
    _input.to_vec()
}
