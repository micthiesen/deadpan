// Fixed detector-bank admission, exact lengths, output guards and ownership.
#include "finite_peak.h"
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
    constexpr std::size_t rows = 15;
    constexpr std::size_t taps = 129;
    constexpr std::size_t input_frames = 1152;
    constexpr std::size_t output_frames = 1024;
    std::array<double, rows * taps> coefficients{};
    for (std::size_t row = 0; row < rows; ++row) {
        coefficients[row * taps + 64] = double(row + 1) / 32.0;
    }
    std::array<float, input_frames> left{}, right{};
    left[64 + 200] = 0.5f;
    right[64 + 200] = -0.25f;

    void *bank = nullptr;
    check(dp_dsp_peak_create(coefficients.data(), coefficients.size() - 1, &bank) == 1 && !bank);
    check(dp_dsp_peak_create(nullptr, coefficients.size(), &bank) == 1 && !bank);
    check(dp_dsp_peak_create(coefficients.data(), coefficients.size(), nullptr) == 1);
    coefficients[17] = std::numeric_limits<double>::quiet_NaN();
    check(dp_dsp_peak_create(coefficients.data(), coefficients.size(), &bank) == 1 && !bank);
    coefficients[17] = 16.01;
    check(dp_dsp_peak_create(coefficients.data(), coefficients.size(), &bank) == 1 && !bank);
    coefficients[17] = 0.0;
    check(dp_dsp_peak_create(coefficients.data(), coefficients.size(), &bank) == 0 && bank);

    std::array<double, output_frames + 2> output{};
    output.fill(123.0);
    check(dp_dsp_peak_read(bank, left.data(), right.data(), input_frames - 1,
                           output.data() + 1, output_frames) == 1);
    check(dp_dsp_peak_read(bank, left.data(), right.data(), input_frames,
                           output.data() + 1, output_frames - 1) == 1);
    check(dp_dsp_peak_read(bank, nullptr, right.data(), input_frames,
                           output.data() + 1, output_frames) == 1);
    for (double value : output) check(value == 123.0);

    left[3] = std::numeric_limits<float>::quiet_NaN();
    check(dp_dsp_peak_read(bank, left.data(), right.data(), input_frames,
                           output.data() + 1, output_frames) == 1);
    left[3] = 16.01f;
    check(dp_dsp_peak_read(bank, left.data(), right.data(), input_frames,
                           output.data() + 1, output_frames) == 1);
    left[3] = 0.0f;
    for (double value : output) check(value == 123.0);

    check(dp_dsp_peak_read(bank, left.data(), right.data(), input_frames,
                           output.data() + 1, output_frames) == 0);
    check(output.front() == 123.0 && output.back() == 123.0);
    for (std::size_t frame = 0; frame < output_frames; ++frame) {
        const double expected = frame == 200 ? 15.0 / 32.0 * 0.5 : 0.0;
        check(std::abs(output[frame + 1] - expected) < 1e-12);
    }

    std::array<double, rows * taps> silent_coefficients{};
    void *silent_bank = nullptr;
    check(dp_dsp_peak_create(silent_coefficients.data(), silent_coefficients.size(), &silent_bank) == 0);
    left[64 + 200] = 0.0f;
    output.fill(123.0);
    check(dp_dsp_peak_read(silent_bank, left.data(), right.data(), input_frames,
                           output.data() + 1, output_frames) == 0);
    for (std::size_t frame = 0; frame < output_frames; ++frame)
        check(output[frame + 1] == 0.0);
    check(dp_dsp_peak_read(bank, left.data(), right.data(), input_frames,
                           output.data() + 1, output_frames) == 0);

    dp_dsp_peak_destroy(silent_bank);
    dp_dsp_peak_destroy(bank);
    dp_dsp_peak_destroy(nullptr);
    std::cout << "fixed peak bank admission, finite convolution, guards and independent plans passed\n";
}
