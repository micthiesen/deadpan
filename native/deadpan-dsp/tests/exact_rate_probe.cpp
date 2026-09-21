// Exact-rate ABI admission and count-independent scheduling under sanitizers.
#include "adapter.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <limits>
#include <vector>

static void check(bool condition) {
    if (!condition) std::abort();
}

static std::vector<float> render(const std::vector<float> &input, std::uint32_t frames,
                                std::uint64_t numerator, std::uint64_t denominator,
                                std::uint32_t request) {
    void *engine = nullptr;
    check(dp_dsp_create_exact_rate(input.data(), input.data(), std::uint32_t(input.size()),
                                   frames, numerator, denominator, 0, &engine) == 0 && engine);
    std::vector<float> result;
    result.reserve(frames);
    while (result.size() < frames) {
        std::array<float, 258> left{}, right{};
        left.fill(123); right.fill(456);
        std::uint32_t written = 99;
        check(dp_dsp_read(engine, left.data() + 1, right.data() + 1, request, &written) == 0);
        check(written == std::min(request, frames - std::uint32_t(result.size())));
        check(left.front() == 123 && right.front() == 456);
        for (std::uint32_t i = 1; i <= written; ++i) {
            check(std::isfinite(left[i]) && std::isfinite(right[i]));
            result.push_back(left[i]);
        }
        for (std::size_t i = written + 1; i < left.size(); ++i)
            check(left[i] == 123 && right[i] == 456);
    }
    std::array<float, 1> left{{123}}, right{{456}};
    std::uint32_t written = 99;
    check(dp_dsp_read(engine, left.data(), right.data(), 1, &written) == 0 && !written);
    check(left[0] == 123 && right[0] == 456);
    dp_dsp_destroy(engine);
    return result;
}

int main() {
    std::vector<float> input(1003, 0.0f);
    input[334] = .8f;
    void *engine = nullptr;
    check(dp_dsp_create_exact_rate(input.data(), input.data(), 1003, 1337, 2, 3, 0, nullptr) == 1);
    check(dp_dsp_create_exact_rate(nullptr, input.data(), 1003, 1337, 2, 3, 0, &engine) == 1 && !engine);
    check(dp_dsp_create_exact_rate(input.data(), nullptr, 1003, 1337, 2, 3, 0, &engine) == 1 && !engine);
    for (const auto &rate : std::array<std::array<std::uint64_t, 2>, 6>{
             {{0, 1}, {1, 0}, {1, 9}, {9, 1}, {UINT64_MAX, 1}, {1, UINT64_MAX}}})
        check(dp_dsp_create_exact_rate(input.data(), input.data(), 1003, 1337,
                                       rate[0], rate[1], 0, &engine) == 1 && !engine);
    // Invalid lengths must be rejected before touching the tiny real arrays.
    for (const auto &sizes : std::array<std::array<std::uint32_t, 2>, 5>{
             {{0, 1}, {1, 0}, {1048577, 1}, {1, 8388609}, {UINT32_MAX, UINT32_MAX}}})
        check(dp_dsp_create_exact_rate(input.data(), input.data(), sizes[0], sizes[1],
                                       1, 1, 0, &engine) == 1 && !engine);
    for (int pitch : {-25, 25})
        check(dp_dsp_create_exact_rate(input.data(), input.data(), 1003, 1337,
                                       2, 3, pitch, &engine) == 1 && !engine);
    for (float bad : {std::numeric_limits<float>::quiet_NaN(),
                      std::numeric_limits<float>::infinity(), -16.01f, 16.01f}) {
        input[17] = bad;
        check(dp_dsp_create_exact_rate(input.data(), input.data(), 1003, 1337,
                                       2, 3, 0, &engine) == 1 && !engine);
    }
    input[17] = 0;
    const auto whole = render(input, 1337, 2, 3, 256);
    check(whole == render(input, 1337, 200, 300, 17));
    const auto prefix = render(input, 1037, 2, 3, 127);
    check(std::equal(prefix.begin(), prefix.end(), whole.begin()));
    auto padded = input;
    padded.resize(1111, 0.0f);
    check(whole == render(padded, 1337, 2, 3, 256));
    const std::uint64_t center = std::uint64_t(257) << 54;
    const std::uint64_t denominator = std::uint64_t(1) << 63;
    check(double(center - 1)/double(denominator) == double(center + 1)/double(denominator));
    check(render(input, 2003, center - 1, denominator, 256)
          != render(input, 2003, center + 1, denominator, 17));
    check(render(input, 1337, UINT64_MAX, UINT64_MAX, 256)
          == render(input, 1337, 1, 1, 17));
    // Array/output allocation may differ far from the exact rate; zero-extended
    // context remains bounded, including at both admitted rate extremes.
    for (const auto &rate : std::array<std::array<std::uint64_t, 2>, 2>{{{1, 8}, {8, 1}}}) {
        std::vector<float> tiny(1, 0.5f);
        const auto output = render(tiny, 257, rate[0], rate[1], 256);
        check(output.size() == 257);
    }
    std::cout << "exact-rate admission, u64 boundaries, allocation independence and guards passed\n";
}
