#pragma once

#include <cstdint>

#ifdef GPU_CODEC_STATIC
  #define GPU_CODEC_API
#elif defined(_WIN32)
  #ifdef GPU_CODEC_EXPORTS
    #define GPU_CODEC_API __declspec(dllexport)
  #else
    #define GPU_CODEC_API __declspec(dllimport)
  #endif
#else
  #define GPU_CODEC_API
#endif

extern "C" {

// return 0 = OK, non-zero = error

GPU_CODEC_API int gpu_compress_block(
    const uint8_t* input,
    uint32_t       input_len,
    uint8_t*       output,
    uint32_t       output_capacity,
    uint32_t*      output_len
);

GPU_CODEC_API int gpu_decompress_block(
    const uint8_t* input,
    uint32_t       input_len,
    uint8_t*       output,
    uint32_t       output_capacity,
    uint32_t*      output_len
);

}
