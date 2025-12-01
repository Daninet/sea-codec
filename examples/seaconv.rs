use clap::{Arg, ArgAction, ArgMatches, Command};
use sea_codec::{
    decoder::SeaDecoder,
    encoder::{EncoderSettings, SeaEncoder},
};
use std::{
    io::{Read, Write},
    path::Path,
};
use wav::{read_wav, write_wav};

#[path = "../tests/wav.rs"]
mod wav;

fn get_encoder_settings(matches: &ArgMatches) -> EncoderSettings {
    let frames_per_chunk = matches
        .get_one::<String>("chunk-size")
        .unwrap()
        .parse::<u16>()
        .unwrap_or_else(|_| {
            eprintln!("Error: Failed to parse chunk size");
            std::process::exit(1);
        });

    if frames_per_chunk < 200 || frames_per_chunk > 32000 {
        eprintln!("Error: Chunk size must be between 200 and 32000");
        std::process::exit(1);
    }

    let scale_factor_bits = matches
        .get_one::<String>("scalefactor-bits")
        .unwrap()
        .parse::<u8>()
        .unwrap_or_else(|_| {
            eprintln!("Error: Failed to parse scale factor bits");
            std::process::exit(1);
        });

    if scale_factor_bits < 3 || scale_factor_bits > 5 {
        eprintln!("Error: Scale factor bits must be between 3 and 5");
        std::process::exit(1);
    }

    let scale_factor_frames = matches
        .get_one::<String>("scalefactor-distance")
        .unwrap()
        .parse::<u8>()
        .unwrap_or_else(|_| {
            eprintln!("Error: Failed to parse scale factor frames");
            std::process::exit(1);
        });

    if scale_factor_frames < 1 || frames_per_chunk % scale_factor_frames as u16 != 0 {
        eprintln!("Error: Scale factor frames must be a divisor of chunk size");
        std::process::exit(1);
    }

    let residual_bits = matches
        .get_one::<String>("bitrate")
        .unwrap()
        .parse::<f32>()
        .unwrap_or_else(|_| {
            eprintln!("Error: Failed to parse residual bits");
            std::process::exit(1);
        });

    if residual_bits < 1.0 || residual_bits > 8.0 {
        eprintln!("Error: Bitrate must be between 1.0 and 8.0");
        std::process::exit(1);
    }

    let vbr = matches.get_flag("vbr");

    if vbr {
        if !(1.5..=8.0).contains(&residual_bits) {
            eprintln!("Error: With VBR, bitrate must be between 1.5 and 8.0");
            std::process::exit(1);
        }
    } else {
        if residual_bits.fract() != 0.0 || !(1..=8).contains(&(residual_bits as i32)) {
            eprintln!("Error: Without VBR, bitrate must be an integer between 1 and 8");
            std::process::exit(1);
        }
    }

    EncoderSettings {
        scale_factor_bits,
        scale_factor_frames,
        residual_bits,
        vbr,
        frames_per_chunk,
        ..Default::default()
    }
}

fn main() {
    let matches = Command::new("seaconv")
        .about("Converts between .wav and .sea files")
        .arg(
            Arg::new("input")
                .help("The input file in LPCM LE .wav or .sea format")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("output")
                .help("The output file to save the conversion result (.sea or .wav)")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::new("chunk-size")
                .long("chunk-size")
                .short('c')
                .help("Sets the number of frames within a chunk")
                .default_value("5120"),
        )
        .arg(
            Arg::new("bitrate")
                .long("bitrate")
                .short('b')
                .help("Sets the bitrate for the conversion")
                .default_value("3"),
        )
        .arg(
            Arg::new("scalefactor-bits")
                .long("scalefactor-bits")
                .short('s')
                .help("Sets the bitrate for scale factors")
                .default_value("4"),
        )
        .arg(
            Arg::new("scalefactor-distance")
                .long("scalefactor-distance")
                .short('d')
                .help("Sets the distance between scale factors in frames")
                .default_value("20"),
        )
        .arg(
            Arg::new("vbr")
                .long("vbr")
                .short('v')
                .action(ArgAction::SetTrue)
                .help("Enables Variable Bit Rate (VBR)"),
        )
        .get_matches();

    let settings = get_encoder_settings(&matches);

    let input = matches.get_one::<String>("input").unwrap();
    let output = matches.get_one::<String>("output").unwrap();

    let input_ext = Path::new(input).extension().and_then(|ext| ext.to_str());
    let output_ext = Path::new(output).extension().and_then(|ext| ext.to_str());

    match (input_ext, output_ext) {
        (Some("wav"), Some("sea")) => {
            let input_wave = read_wav(&Path::new(input)).unwrap_or_else(|_| {
                eprintln!("Error: Failed to decode .wav file");
                std::process::exit(1);
            });

            let mut output_file = std::fs::File::create(output).unwrap_or_else(|_| {
                eprintln!("Error: Failed to create output file");
                std::process::exit(1);
            });

            let mut sea_encoder = SeaEncoder::from_slice(
                input_wave.channels as u8,
                input_wave.sample_rate,
                Some(input_wave.samples.len() as u32 / input_wave.channels),
                settings,
                &input_wave.samples,
            )
            .unwrap_or_else(|_| {
                eprintln!("Error: Failed to create encoder");
                std::process::exit(1);
            });

            let mut buf = Vec::new();
            while sea_encoder.encode_frame(&mut buf).unwrap_or_else(|_| {
                eprintln!("Error: Failed to encode frame");
                std::process::exit(1);
            }) {
                output_file.write_all(&buf).unwrap_or_else(|_| {
                    eprintln!("Error: Failed to write to output file");
                    std::process::exit(1);
                });
                buf.clear();
            }

            sea_encoder.finalize().unwrap_or_else(|_| {
                eprintln!("Error: Failed to finalize encoder");
                std::process::exit(1);
            });
        }
        (Some("sea"), Some("wav")) => {
            let mut input_file = std::fs::File::open(input).unwrap_or_else(|_| {
                eprintln!("Error: Failed to open input file");
                std::process::exit(1);
            });

            let mut content = Vec::new();
            input_file.read_to_end(&mut content).unwrap();

            let mut sea_decoded = Vec::<i16>::with_capacity(64 * 1024 * 1024);
            let mut sea_decoder = SeaDecoder::from_slice(&content).unwrap();

            while sea_decoder
                .decode_frame(&mut sea_decoded)
                .unwrap_or_else(|_| {
                    eprintln!("Error: Failed to decode frame");
                    std::process::exit(1);
                })
            {}

            let info = sea_decoder.get_header();

            write_wav(
                sea_decoded.as_slice(),
                info.channels as u16,
                info.sample_rate,
                output,
            )
            .unwrap_or_else(|_| {
                eprintln!("Error: Failed to encode wav file");
                std::process::exit(1);
            });
        }
        _ => {
            eprintln!("Error: Invalid file extensions. Supported conversions are .wav to .sea and .sea to .wav");
            std::process::exit(1);
        }
    }
}
