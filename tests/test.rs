use helpers::{encode_decode, gen_test_signal, TEST_SAMPLE_RATE};
use sea_codec::encoder::EncoderSettings;

extern crate sea_codec;

mod helpers;

#[test]
fn test_sample_len() {
    for vbr in [false, true] {
        for channels in [1, 2, 3] {
            let frame_size: i32 = 100;
            for mul in [1, 2, 3, 100] {
                let start = ((mul * frame_size) - 2).max(0);
                for sample_len in start..(mul * frame_size + 2) {
                    println!("Testing channels={} sample_len={}", channels, sample_len);
                    let input = gen_test_signal(channels, sample_len as usize);
                    let output = encode_decode(
                        &input,
                        TEST_SAMPLE_RATE,
                        channels,
                        EncoderSettings {
                            scale_factor_bits: 4,
                            vbr,
                            ..Default::default()
                        },
                    );
                    assert_eq!(input.len(), output.decoded.len());
                    let quality = helpers::get_audio_quality(&input, &output.decoded);
                    println!("Quality: {:?}", quality);
                    // assert!(quality.psnr < -18.0);
                }
            }
        }
    }
}

#[test]
fn test_parameters() {
    for channels in [1, 2, 3] {
        let input = gen_test_signal(channels, TEST_SAMPLE_RATE as usize);
        for frame_size in [5, 10, 20] {
            for scale_factor_bits in 3..=5 {
                for residual_bits in 1..=8 {
                    println!(
                        "Testing frame_size={} channels={} scale_factor_bits={} residual_bits={}",
                        frame_size, channels, scale_factor_bits, residual_bits
                    );
                    let output = encode_decode(
                        &input,
                        TEST_SAMPLE_RATE,
                        channels,
                        EncoderSettings {
                            residual_bits: residual_bits as f32,
                            scale_factor_bits,
                            ..Default::default()
                        },
                    );
                    assert_eq!(input.len(), output.decoded.len());
                    let quality = helpers::get_audio_quality(&input, &output.decoded);
                    println!("Quality: {:?}", quality);
                    assert!(quality.psnr < -20.0);
                }
            }
        }
    }
}
