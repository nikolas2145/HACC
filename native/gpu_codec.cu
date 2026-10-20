#include "gpu_codec.h"
#include <cuda_runtime.h>
#include <cstdint>
#include <cstring>
#include <vector>

// One CUDA thread encodes an independent 4 KiB chunk. HGP3 tokens are either
// literal (0..127: length - 1, followed by bytes) or LZ match (128..255:
// 0x80 | (length - 4), followed by a little-endian distance). Token 0xff
// instead has an explicit u16 length before the distance, allowing long matches.
// HGP1 and HGP2 decoding remain supported for earlier GPU archives.
static constexpr uint32_t CHUNK = 4096;
static constexpr uint32_t STRIDE = CHUNK * 2;
static constexpr uint32_t HASH_SIZE = 1024;
static constexpr uint8_t MAGIC[4] = {'H', 'G', 'P', '3'};
__host__ __device__ static void put16(uint8_t* p, uint16_t n) { p[0] = uint8_t(n); p[1] = uint8_t(n >> 8); }
static uint16_t get16(const uint8_t* p) { return uint16_t(p[0]) | uint16_t(p[1]) << 8; }
static void put32(uint8_t* p, uint32_t n) { for (int i = 0; i < 4; ++i) p[i] = uint8_t(n >> (8 * i)); }
static uint32_t get32(const uint8_t* p) { return uint32_t(p[0]) | uint32_t(p[1]) << 8 | uint32_t(p[2]) << 16 | uint32_t(p[3]) << 24; }

__device__ static uint32_t hash3(const uint8_t* p) {
    return ((uint32_t(p[0]) * 257u) ^ (uint32_t(p[1]) * 263u) ^
            (uint32_t(p[2]) * 269u)) & (HASH_SIZE - 1);
}

__global__ static void encode(const uint8_t* input, uint32_t total, uint8_t* scratch,
                              uint16_t* lengths, uint16_t* hashes) {
    uint32_t id = blockIdx.x * blockDim.x + threadIdx.x;
    if (id >= (total + CHUNK - 1) / CHUNK) return;
    uint32_t size = min(CHUNK, total - id * CHUNK);
    const uint8_t* src = input + id * CHUNK;
    uint8_t* dst = scratch + id * STRIDE;
    uint16_t* last = hashes + id * HASH_SIZE;
    for (uint32_t i = 0; i < HASH_SIZE; ++i) last[i] = 0;
    uint32_t pos = 0, literal = 0, out = 0;
    while (pos < size) {
        uint32_t match_len = 0, distance = 0;
        if (pos + 3 <= size) {
            uint32_t h = hash3(src + pos);
            uint32_t prev_plus_one = last[h];
            last[h] = uint16_t(pos + 1);
            if (prev_plus_one) {
                uint32_t prev = prev_plus_one - 1;
                if (src[prev] == src[pos] && src[prev + 1] == src[pos + 1] &&
                    src[prev + 2] == src[pos + 2]) {
                    uint32_t limit = size - pos;
                    match_len = 3;
                    while (match_len < limit && src[prev + match_len] == src[pos + match_len])
                        ++match_len;
                    distance = pos - prev;
                }
            }
        }
        if (match_len < 4) { ++pos; continue; }

        while (literal < pos) {
            uint32_t n = min(128u, pos - literal);
            dst[out++] = uint8_t(n - 1);
            for (uint32_t j = 0; j < n; ++j) dst[out++] = src[literal++];
        }
        if (match_len >= 131) {
            dst[out++] = 0xff;
            put16(dst + out, uint16_t(match_len));
            out += 2;
        } else {
            dst[out++] = uint8_t(0x80 | (match_len - 4));
        }
        put16(dst + out, uint16_t(distance));
        out += 2;
        for (uint32_t j = 1; j < match_len && pos + j + 2 < size; ++j)
            last[hash3(src + pos + j)] = uint16_t(pos + j + 1);
        pos += match_len;
        literal = pos;
    }
    while (literal < size) {
        uint32_t n = min(128u, size - literal);
        dst[out++] = uint8_t(n - 1);
        for (uint32_t j = 0; j < n; ++j) dst[out++] = src[literal++];
    }
    lengths[id] = uint16_t(out);
}

__global__ static void decode(const uint8_t* payload, const uint32_t* offsets,
                              const uint16_t* lengths, uint32_t total, uint32_t count,
                              uint8_t* output, uint8_t* errors, uint8_t version) {
    uint32_t id = blockIdx.x * blockDim.x + threadIdx.x;
    if (id >= count) return;
    const uint8_t* src = payload + offsets[id];
    uint8_t* dst = output + id * CHUNK;
    uint32_t expected = min(CHUNK, total - id * CHUNK);
    uint32_t in = 0, out = 0, size = lengths[id];
    while (in < size) {
        uint8_t token = src[in++];
        uint32_t n = (token & 0x7f) + ((token & 0x80) && version == '2' ? 4 : 1);
        if ((token & 0x80) && version == '3') {
            n = (token & 0x7f) + 4;
            if (token == 0xff) {
                if (in + 2 > size) { errors[id] = 1; return; }
                n = uint32_t(src[in]) | uint32_t(src[in + 1]) << 8;
                in += 2;
                if (n < 131) { errors[id] = 1; return; }
            }
        }
        if (out + n > expected) { errors[id] = 1; return; }
        if (token & 0x80) {
            if (version == '1') {
                if (in >= size) { errors[id] = 1; return; }
                uint8_t value = src[in++];
                for (uint32_t j = 0; j < n; ++j) dst[out++] = value;
            } else {
                if (in + 2 > size) { errors[id] = 1; return; }
                uint32_t distance = uint32_t(src[in]) | uint32_t(src[in + 1]) << 8;
                in += 2;
                if (!distance || distance > out) { errors[id] = 1; return; }
                for (uint32_t j = 0; j < n; ++j) {
                    dst[out] = dst[out - distance];
                    ++out;
                }
            }
        } else {
            if (in + n > size) { errors[id] = 1; return; }
            for (uint32_t j = 0; j < n; ++j) dst[out++] = src[in++];
        }
    }
    errors[id] = out == expected ? 0 : 1;
}

extern "C" GPU_CODEC_API int gpu_compress_block(const uint8_t* input, uint32_t input_len,
    uint8_t* output, uint32_t output_capacity, uint32_t* output_len) {
    if (!input || !output || !output_len || !input_len) return 1;
    uint32_t count = (input_len + CHUNK - 1) / CHUNK;
    if (count > UINT16_MAX) return 1;
    uint8_t *d_input = nullptr, *d_scratch = nullptr;
    uint16_t *d_lengths = nullptr, *d_hashes = nullptr;
    std::vector<uint16_t> lengths(count);
    std::vector<uint8_t> scratch(size_t(count) * STRIDE);
    int result = 2;
    if (cudaMalloc(&d_input, input_len) != cudaSuccess ||
        cudaMalloc(&d_scratch, scratch.size()) != cudaSuccess ||
        cudaMalloc(&d_lengths, count * sizeof(uint16_t)) != cudaSuccess ||
        cudaMalloc(&d_hashes, size_t(count) * HASH_SIZE * sizeof(uint16_t)) != cudaSuccess) goto done;
    if (cudaMemcpy(d_input, input, input_len, cudaMemcpyHostToDevice) != cudaSuccess) goto done;
    encode<<<(count + 127) / 128, 128>>>(d_input, input_len, d_scratch, d_lengths, d_hashes);
    if (cudaGetLastError() != cudaSuccess || cudaDeviceSynchronize() != cudaSuccess) goto done;
    if (cudaMemcpy(lengths.data(), d_lengths, count * sizeof(uint16_t), cudaMemcpyDeviceToHost) != cudaSuccess ||
        cudaMemcpy(scratch.data(), d_scratch, scratch.size(), cudaMemcpyDeviceToHost) != cudaSuccess) goto done;
    {
        uint64_t needed = 10 + 2 * uint64_t(count);
        for (uint16_t len : lengths) needed += len;
        if (needed > output_capacity) { result = 3; goto done; }
        std::memcpy(output, MAGIC, 4);
        put32(output + 4, input_len);
        put16(output + 8, uint16_t(count));
        uint32_t cursor = 10 + 2 * count;
        for (uint32_t i = 0; i < count; ++i) {
            put16(output + 10 + 2 * i, lengths[i]);
            std::memcpy(output + cursor, scratch.data() + size_t(i) * STRIDE, lengths[i]);
            cursor += lengths[i];
        }
        *output_len = cursor;
        result = 0;
    }
done:
    if (d_hashes) cudaFree(d_hashes);
    if (d_lengths) cudaFree(d_lengths);
    if (d_scratch) cudaFree(d_scratch);
    if (d_input) cudaFree(d_input);
    return result;
}

extern "C" GPU_CODEC_API int gpu_decompress_block(const uint8_t* input, uint32_t input_len,
    uint8_t* output, uint32_t output_capacity, uint32_t* output_len) {
    if (!input || !output || !output_len || input_len < 12 ||
        std::memcmp(input, MAGIC, 3) ||
        (input[3] != '1' && input[3] != '2' && input[3] != '3')) return 1;
    uint32_t total = get32(input + 4), count = get16(input + 8);
    if (!total || total > output_capacity || count != (total + CHUNK - 1) / CHUNK ||
        10 + 2 * uint64_t(count) > input_len) return 1;
    std::vector<uint16_t> lengths(count);
    std::vector<uint32_t> offsets(count);
    uint64_t cursor = 10 + 2 * uint64_t(count);
    for (uint32_t i = 0; i < count; ++i) {
        lengths[i] = get16(input + 10 + 2 * i);
        offsets[i] = uint32_t(cursor - (10 + 2 * count));
        cursor += lengths[i];
        if (cursor > input_len) return 1;
    }
    if (cursor != input_len) return 1;
    const uint8_t* payload = input + 10 + 2 * count;
    uint32_t payload_len = input_len - 10 - 2 * count;
    uint8_t *d_payload = nullptr, *d_output = nullptr, *d_errors = nullptr;
    uint32_t* d_offsets = nullptr;
    uint16_t* d_lengths = nullptr;
    std::vector<uint8_t> errors(count);
    int result = 2;
    if (cudaMalloc(&d_payload, payload_len) != cudaSuccess ||
        cudaMalloc(&d_output, total) != cudaSuccess ||
        cudaMalloc(&d_errors, count) != cudaSuccess ||
        cudaMalloc(&d_offsets, count * sizeof(uint32_t)) != cudaSuccess ||
        cudaMalloc(&d_lengths, count * sizeof(uint16_t)) != cudaSuccess) goto done;
    if (cudaMemcpy(d_payload, payload, payload_len, cudaMemcpyHostToDevice) != cudaSuccess ||
        cudaMemcpy(d_offsets, offsets.data(), count * sizeof(uint32_t), cudaMemcpyHostToDevice) != cudaSuccess ||
        cudaMemcpy(d_lengths, lengths.data(), count * sizeof(uint16_t), cudaMemcpyHostToDevice) != cudaSuccess) goto done;
    decode<<<(count + 127) / 128, 128>>>(d_payload, d_offsets, d_lengths, total, count,
                                         d_output, d_errors, input[3]);
    if (cudaGetLastError() != cudaSuccess || cudaDeviceSynchronize() != cudaSuccess) goto done;
    if (cudaMemcpy(errors.data(), d_errors, count, cudaMemcpyDeviceToHost) != cudaSuccess) goto done;
    for (uint8_t error : errors) if (error) { result = 3; goto done; }
    if (cudaMemcpy(output, d_output, total, cudaMemcpyDeviceToHost) != cudaSuccess) goto done;
    *output_len = total;
    result = 0;
done:
    if (d_lengths) cudaFree(d_lengths);
    if (d_offsets) cudaFree(d_offsets);
    if (d_errors) cudaFree(d_errors);
    if (d_output) cudaFree(d_output);
    if (d_payload) cudaFree(d_payload);
    return result;
}
