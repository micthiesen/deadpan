// Standalone bridge ownership and error-path sanitizer probe. No device I/O.
#include "adapter.h"
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <limits>

static void check(bool condition) {
    if (!condition) std::abort();
}
int main() {
    std::array<float, 1003> input{};
    input[334] = .8f;
    void *engine = nullptr;
    check(dp_dsp_create(input.data(), input.data(), 1003, 1337, 7, nullptr) == 1);
    check(dp_dsp_create(nullptr, input.data(), 1003, 1337, 7, &engine) == 1 && !engine);
    check(dp_dsp_create(input.data(), nullptr, 1003, 1337, 7, &engine) == 1 && !engine);
    for (const auto &recipe : std::array<std::array<std::uint32_t, 2>, 6>{
            {{0, 1}, {1, 0}, {1, 9}, {9, 1}, {1048577, 1}, {1, UINT32_MAX}}}) {
        check(dp_dsp_create(input.data(), input.data(), recipe[0], recipe[1], 0, &engine) == 1 && !engine);
    }
    for (int pitch : {-25, 25})
        check(dp_dsp_create(input.data(), input.data(), 1003, 1337, pitch, &engine) == 1 && !engine);
    for (float bad : {std::numeric_limits<float>::quiet_NaN(), std::numeric_limits<float>::infinity(),
                      -std::numeric_limits<float>::infinity(), std::numeric_limits<float>::max(), 16.01f, -16.01f}) {
        input[17] = bad;
        check(dp_dsp_create(input.data(), input.data(), 1003, 1337, 7, &engine) == 1 && !engine);
    }
    input[17] = 0;
    for (int iteration = 0; iteration < 16; ++iteration) {
        check(dp_dsp_create(input.data(), input.data(), 1003, 1337, 7, &engine) == 0 && engine);
        std::array<float, 258> left{}, right{};
        left.fill(123); right.fill(456);
        std::uint32_t written = 42;
        check(dp_dsp_read(nullptr, left.data() + 1, right.data() + 1, 256, &written) == 1 && !written);
        check(dp_dsp_read(engine, nullptr, right.data() + 1, 1, &written) == 1 && !written);
        check(dp_dsp_read(engine, left.data() + 1, nullptr, 1, &written) == 1 && !written);
        check(dp_dsp_read(engine, left.data() + 1, right.data() + 1, 1, nullptr) == 1);
        check(dp_dsp_read(engine, left.data() + 1, right.data() + 1, 257, &written) == 1 && !written);
        check(dp_dsp_read(engine, nullptr, nullptr, 0, &written) == 0 && !written);
        for (auto sample : left) check(sample == 123);
        for (auto sample : right) check(sample == 456);
        std::uint32_t total = 0;
        while (total < 1337) {
            auto count = iteration % 2 ? 17U : 256U;
            left.fill(123); right.fill(456);
            check(dp_dsp_read(engine, left.data() + 1, right.data() + 1, count, &written) == 0);
            check(written == std::min(count, 1337 - total));
            check(left.front() == 123 && right.front() == 456);
            for (std::size_t i = 1; i <= written; ++i)
                check(std::isfinite(left[i]) && std::isfinite(right[i]));
            for (std::size_t i = written + 1; i < left.size(); ++i)
                check(left[i] == 123 && right[i] == 456);
            total += written;
        }
        check(dp_dsp_read(engine, left.data() + 1, right.data() + 1, 256, &written) == 0 && !written);
        dp_dsp_destroy(engine);
        engine = nullptr;
    }
    // Admitted extreme ratios and pitches still use bounded reads.
    for (const auto &recipe : std::array<std::array<std::uint32_t, 2>, 2>{{{1, 8}, {8, 1}}}) {
        for (int pitch : {-24, 24}) {
            input.fill(16.0f);
            check(dp_dsp_create(input.data(), input.data(), recipe[0], recipe[1], pitch, &engine) == 0);
            std::array<float, 256> left{}, right{};
            std::uint32_t written = 0;
            check(dp_dsp_read(engine, left.data(), right.data(), 256, &written) == 0);
            check(written == recipe[1]);
            dp_dsp_destroy(engine);
            engine = nullptr;
        }
    }
    dp_dsp_destroy(nullptr);
    std::cout << "bridge ownership, guards, admission, EOF and extreme recipes passed\n";
}
