#include "../../include/sea_codec.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

void generate_sine_wave(int16_t* buffer, size_t length, uint32_t sample_rate, float frequency)
{
    for (size_t i = 0; i < length; i++) {
        float t = (float)i / sample_rate;
        buffer[i] = (int16_t)(sinf(2.0f * (float)M_PI * frequency * t) * 32000.0f);
    }
}

int main()
{
    uint32_t sample_rate = 44100;
    uint32_t channels = 1;
    float duration = 1.0f;
    size_t num_samples = (size_t)(sample_rate * duration);

    printf("Generating %zu samples of sine wave...\n", num_samples);
    int16_t* input_samples = (int16_t*)malloc(num_samples * sizeof(int16_t));
    if (!input_samples) {
        fprintf(stderr, "Memory allocation failed\n");
        return 1;
    }

    generate_sine_wave(input_samples, num_samples, sample_rate, 440.0f);

    printf("Encoding...\n");
    CSeaEncoderSettings settings = sea_encoder_default_settings();
    uint8_t* encoded_data = NULL;
    size_t encoded_length = 0;

    int result = sea_encode(input_samples, num_samples, sample_rate, channels, &settings, &encoded_data, &encoded_length);

    if (result != 0) {
        fprintf(stderr, "Encoding failed\n");
        free(input_samples);
        return 1;
    }

    printf("Encoded size: %zu bytes\n", encoded_length);

    printf("Decoding...\n");
    int16_t* decoded_samples = NULL;
    size_t decoded_sample_count = 0;
    uint32_t decoded_sample_rate = 0;
    uint32_t decoded_channels = 0;

    result = sea_decode(encoded_data, encoded_length, &decoded_samples, &decoded_sample_count, &decoded_sample_rate, &decoded_channels);

    if (result != 0) {
        fprintf(stderr, "Decoding failed\n");
        sea_free_packet(encoded_data, encoded_length);
        free(input_samples);
        return 1;
    }

    printf("Decoded info: %zu samples, %u Hz, %u channels\n", decoded_sample_count, decoded_sample_rate, decoded_channels);

    if (decoded_sample_count == num_samples) {
        printf("Sample count matches!\n");
    } else {
        printf("Sample count mismatch! Expected %zu, got %zu\n", num_samples, decoded_sample_count);
    }

    sea_free_packet(encoded_data, encoded_length);
    sea_free_samples(decoded_samples, decoded_sample_count);
    free(input_samples);

    return 0;
}
