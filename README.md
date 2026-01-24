# SEA - Simple Embedded Audio Codec

SEA is a low-complexity, lossy audio codec designed for embedded devices, inspired by the awesome [QOA codec](https://qoaformat.org/). Like QOA, SEA utilizes the Least Mean Squares Filter (LMS) algorithm, but it introduces variable bitrate (VBR) support and features slightly modified quantization tables. The reference implementation is written in Rust, and a compact ~250-line [decoder written in C](https://github.com/Daninet/sea-codec/blob/master/c/sea.h) is also available for demonstration purposes.

You can test SEA in your browser here: [https://daninet.github.io/sea-codec/](https://daninet.github.io/sea-codec/)

### Key Features

- **Fast, low complexity, time-domain compression.** The decoder fits into ~250 lines of C code.
- Ideal for **low-power** embedded devices, game assets, and live streaming. Supports `no-std` environments.
- **Flat frequency response:** No low-pass filtering is applied, preserving the full frequency range.
- **Variable bitrate:** 1.2 - 8.5 bits per sample
- Constant and variable bitrate (**CBR** and **VBR**) modes.
- **Fixed frame length:** Enables constant-time seeking.
- **Multi-channel support**: Handles up to 255 channels.
- **Metadata storage**: Allows embedding additional information.
- **MIT License**

### CLI tool

You can build it with `cargo build --example seaconv --release` or you can download a pre-built binary from the [releases page](https://github.com/Daninet/sea-codec/releases).

#### Examples

`seaconv.exe input.wav encoded.sea --bitrate 3`

`seaconv.exe encoded.sea decoded.wav`

```
Usage: seaconv.exe [OPTIONS] <input> <output>

Arguments:
  <input>   The input file in LPCM LE .wav or .sea format
  <output>  The output file to save the conversion result (.sea or .wav)

Options:
  -c, --chunk-size <chunk-size>
          Sets the number of frames within a chunk [default: 5120]
  -b, --bitrate <bitrate>
          Sets the bitrate for the conversion [default: 3]
  -s, --scalefactor-bits <scalefactor-bits>
          Sets the bitrate for scale factors [default: 4]
  -d, --scalefactor-distance <scalefactor-distance>
          Sets the distance between scale factors in frames [default: 20]
  -v, --vbr
          Enables Variable Bit Rate (VBR)
  -r, --resample <resample>
          Sets the target sample rate for resampling
  -h, --help
          Print help
```

### Other implementations

- The Rust reference implementation exposes C API for integrating SEA into C/C++ projects. See the [C API README](examples/c_api/README.md) for usage details and build instructions.
- [C/C++ implementation](https://github.com/tobybear/sea-codec-c) from @tobybear
- [Minimal C decoder](c/) living inside this repo (decode only)

# SEA file specification

A SEA file consists of a file header followed by a series of chunks. Samples are stored as 16-bit signed integers in interleaved format. All values are stored in little-endian order.

### File header

```c
struct SEA_FILE_HEADER {
  char[4] magic; // "SEAC"
  uint8_t version; // currently 0x01
  uint8_t number_of_channels; // 1 - 255
  uint16_t chunk_size; // size of each chunk in bytes
  uint16_t frames_per_chunk; // number of frames per chunk (a frame includes samples for all channels)
  uint32_t sample_rate; // sampling rate in Hz
  uint32_t total_frames; // total frames per channel (0 indicates streaming until EOF)
  uint32_t metadata_size; // size of metadata in bytes (can be zero)
  char* metadata[metadata_size]; // metadata (UTF-8 encoded string, key=value pairs separated by newline character)
}
```

### Metadata

- **Format**: UTF-8 encoded string
- **Structure**: Key-value pairs separated by newline characters (`\n`)
- Key and value are separated by `=`
- Key is case-insensitive and cannot contain `=` or `\n`
- Value is case sensitive and can contain any characters except `\n`

#### Example metadata

```
author=John Doe
title=My Song
```

### Chunk

- **Fixed size**: Each chunk has a fixed byte size (specified in the file header) and contains a fixed number of frames, enabling constant-time seeking.
- **Padding**: If a chunk is smaller than the specified size in the file header, it is padded with zeroes.
- **Bitpacking**: Scale factors, VBR residual lengths, and residuals are stored using bitpacking.

```c
struct SEA_CHUNK {
  uint8_t type; // CBR(0x01) or VBR(0x02)
  uint8_t scale_factor_and_residual_size; // scale_factor_size (4 bits) | residual_size (4 bits)
  uint8_t scale_factor_frames; // distance between scalefactor values
  uint8_t reserved; // currently set to 0x5A

  struct {
    int16_t history[4];
    int16_t weights[4];
  } lms_state[channels_count]; // LMS filter state for each channel

  uint8_t bitpacked_scale_factors[...]; // bitpacked scale factors (bit count specified by scale_factor_size)

  uint8_t bitpacked_vbr_residual_lengths[...]; // only for VBR mode. stores residual length differences (2 bits per value) compared to reference stored in chunk header

  uint8_t bitpacked_residuals[...] // bitpacked residuals (bit count specified by residual_size or VBR residual lengths)
}
```

- **Interleaved Order**: All packed values are stored in interleaved order (e.g., ch0, ch1, ch2, ch0, ch1, ch2, ...).
- **Scale Factor Frames**: The scale_factor_frames field determines the interval between scale factor values. For example, a value of 20 means one scale factor is applied to 20 samples.
- **VBR Residual Lengths**: In VBR mode, bitpacked_vbr_residual_lengths stores the difference from the standard residual length defined in the chunk header. The offset is -1:

```
0 = chunk_residual - 1
1 = chunk_residual
2 = chunk_residual + 1
3 = chunk_residual + 2
```

# Future plans

- **Seeking Support**: Add seeking functionality to the Rust implementation.
- **Optimization and Benchmarking**: Optimize the implementation and benchmark against other codecs.
- **C Encoder**: Implement an encoder in C for broader compatibility.

# License

MIT License

Copyright (c) 2025 Dani Biró
