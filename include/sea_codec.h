#ifndef SEA_CODEC_H
#define SEA_CODEC_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint8_t scale_factor_bits;
    uint8_t scale_factor_frames;
    float residual_bits;
    uint16_t frames_per_chunk;
    bool vbr;
} CSeaEncoderSettings;

// Helper to get default settings
CSeaEncoderSettings sea_encoder_default_settings();

// Encode
// Returns 0 on success, non-zero on error.
// output_data is allocated by the function and must be freed by sea_free_packet.
// input_length is the number of samples (total, across all channels)
int sea_encode(
    const int16_t* input_samples,
    size_t input_length, 
    uint32_t sample_rate,
    uint32_t channels,
    const CSeaEncoderSettings* settings,
    uint8_t** output_data,
    size_t* output_length
);

// Decode
// output_samples is allocated by the function and must be freed by sea_free_samples.
int sea_decode(
    const uint8_t* encoded_data,
    size_t encoded_length,
    int16_t** output_samples,
    size_t* output_sample_count,
    uint32_t* output_sample_rate,
    uint32_t* output_channels
);

// Memory management
// length must match the size returned by the allocate functions
void sea_free_packet(uint8_t* data, size_t length);
void sea_free_samples(int16_t* samples, size_t length);

#ifdef __cplusplus
}
#endif

#endif // SEA_CODEC_H
