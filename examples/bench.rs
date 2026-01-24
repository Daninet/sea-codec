#[path = "../tests/helpers.rs"]
mod helpers;

#[path = "../tests/wav.rs"]
mod wav;

use std::io::Write;
use std::{fs, io::BufWriter, path::Path, time::Instant};

use sea_codec::{encoder::EncoderSettings, sea_decode, sea_encode};
use wav::{read_wav, write_wav, Wave};

const SAMPLE_NAME: &str = "ivertita_short";
const BASE_PATH: &str = "E:/audio_samples/mixed";
const RESIDUAL_SIZE: f32 = 3.23;

fn encode_decode() {
    let input_wave_path = &format!("{}/{}.wav", BASE_PATH, SAMPLE_NAME);
    let input_wave_full = read_wav(&Path::new(input_wave_path)).unwrap();
    let input_wave = Wave {
        samples: input_wave_full.samples, // [256..256 + 128].to_vec(),
        sample_rate: input_wave_full.sample_rate,
        channels: input_wave_full.channels,
    };

    let settings = EncoderSettings {
        residual_bits: RESIDUAL_SIZE,
        scale_factor_frames: 20,
        scale_factor_bits: 4,
        vbr: true,
        // frames_per_chunk: 5120,
        ..Default::default()
    };

    let now = Instant::now();
    let sea_file = sea_encode(
        &input_wave.samples,
        input_wave.sample_rate,
        input_wave.channels,
        settings.clone(),
    );
    let bits_per_sample = (sea_file.len() as f32 * 8.0) / input_wave.samples.len() as f32;
    println!("Encoding took {}ms", now.elapsed().as_millis());

    println!(
        "Compression ratio {:.2}",
        (input_wave.samples.len() as f64 * 2.0) / sea_file.len() as f64
    );

    let sea_output_filename = format!(
        "{}/{}_after_sea_{}.sea",
        BASE_PATH, SAMPLE_NAME, RESIDUAL_SIZE
    );

    // write sea file to disk in raw format
    let file = fs::File::create(sea_output_filename.as_str()).unwrap();
    BufWriter::new(file).write_all(&sea_file).unwrap();

    let now = Instant::now();
    let decoded = sea_decode(&sea_file);
    println!("Decoding took {}ms", now.elapsed().as_millis());
    assert_eq!(input_wave.samples.len(), decoded.samples.len());

    let vbr_string = if settings.vbr { "_vbr" } else { "" };

    let wav_output_filename = format!(
        "{}/{}_after_sea_{}{}.wav",
        BASE_PATH, SAMPLE_NAME, RESIDUAL_SIZE, vbr_string
    );

    write_wav(
        &decoded.samples,
        decoded.channels as u16,
        decoded.sample_rate as u32,
        wav_output_filename.as_str(),
    )
    .unwrap();

    let sample_difference = input_wave
        .samples
        .iter()
        .zip(decoded.samples.iter())
        .map(|(a, b)| a - b)
        .collect::<Vec<i16>>();

    let diff_wav_output_filename = format!(
        "{}/{}_after_sea_diff_{}{}.wav",
        BASE_PATH, SAMPLE_NAME, RESIDUAL_SIZE, vbr_string
    );

    write_wav(
        &sample_difference,
        decoded.channels as u16,
        decoded.sample_rate as u32,
        diff_wav_output_filename.as_str(),
    )
    .unwrap();

    let quality = helpers::get_audio_quality(&input_wave.samples, &decoded.samples);
    println!("Bits per sample: {:.2}", bits_per_sample);
    println!(
        "RMS: {:.4}% PSNR {:.2} dB",
        quality.rms * 100.0,
        quality.psnr
    );
}

fn main() {
    encode_decode();
}
