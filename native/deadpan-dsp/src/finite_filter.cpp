#include "finite_filter.h"

#if defined(__FAST_MATH__) || (defined(__FINITE_MATH_ONLY__) && __FINITE_MATH_ONLY__) \
    || defined(SIGNALSMITH_USE_ACCELERATE) || defined(SIGNALSMITH_USE_IPP) \
    || defined(SIGNALSMITH_USE_PFFFT) || defined(SIGNALSMITH_USE_PFFFT_DOUBLE)
#error "deadpan-dsp requires ordinary floating point and the portable FFT"
#endif

#include <signalsmith-linear/fft.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <new>

namespace {
constexpr std::size_t radius = 2048;
constexpr std::size_t tap_count = 2 * radius + 1;
constexpr std::size_t output_frames = 1024;
constexpr std::size_t input_frames = output_frames + 2 * radius;
constexpr std::size_t fft_size = input_frames;
constexpr std::size_t bin_count = fft_size / 2;
constexpr double maximum_sample = 1e100;
constexpr double maximum_coefficient = 16.0;

using Fft = signalsmith::linear::RealFFT<double>;
using Complex = Fft::Complex;

enum FilterStatus : std::int32_t {
    filter_ok = 0,
    filter_invalid_argument = 1,
    filter_allocation_failed = 2,
    filter_native_failed = 3,
    filter_nonfinite_output = 4,
    filter_poisoned = 5,
};

struct FiniteFilter {
    Fft fft{fft_size};
    std::array<Complex, bin_count> kernel{};
    std::array<double, fft_size> time{};
    std::array<Complex, bin_count> spectrum{};
    std::array<Complex, bin_count> product{};
    std::array<std::array<double, output_frames>, 2> staged{};
    bool failed = false;

    explicit FiniteFilter(const double *coefficients) {
        // Desired correlation y[c] = sum(h[t] * input[c+t]). Reverse h for
        // convolution, and discard the first tap_count-1 circular outputs.
        // The remaining 1024 are exactly the alias-free overlap-save region.
        for (std::size_t tap = 0; tap < tap_count; ++tap) {
            time[tap] = coefficients[tap_count - 1 - tap];
        }
        fft.fft(time.data(), kernel.data());
    }

    std::int32_t read(const double *left, const double *right,
                      double *output_left, double *output_right) {
        if (failed) return filter_poisoned;
        for (std::size_t frame = 0; frame < input_frames; ++frame) {
            if (!std::isfinite(left[frame]) || !std::isfinite(right[frame])
                || std::abs(left[frame]) > maximum_sample
                || std::abs(right[frame]) > maximum_sample) {
                return filter_invalid_argument;
            }
        }
        const double *inputs[2] = {left, right};
        for (std::size_t channel = 0; channel < 2; ++channel) {
            fft.fft(inputs[channel], spectrum.data());
            // RealFFT packs DC and Nyquist as two independent real bins.
            product[0] = {spectrum[0].real() * kernel[0].real(),
                          spectrum[0].imag() * kernel[0].imag()};
            for (std::size_t bin = 1; bin < bin_count; ++bin) {
                product[bin] = spectrum[bin] * kernel[bin];
            }
            fft.ifft(product.data(), time.data());
            for (std::size_t frame = 0; frame < output_frames; ++frame) {
                const double value = time[2 * radius + frame] / double(fft_size);
                if (!std::isfinite(value)) {
                    failed = true;
                    return filter_nonfinite_output;
                }
                staged[channel][frame] = value;
            }
        }
        std::copy(staged[0].begin(), staged[0].end(), output_left);
        std::copy(staged[1].begin(), staged[1].end(), output_right);
        return filter_ok;
    }
};
} // namespace

extern "C" std::int32_t dp_dsp_filter_create(const double *coefficients,
    std::size_t coefficient_count, void **output) noexcept {
    if (!output) return filter_invalid_argument;
    *output = nullptr;
    if (!coefficients || coefficient_count != tap_count) return filter_invalid_argument;
    for (std::size_t tap = 0; tap < tap_count; ++tap) {
        if (!std::isfinite(coefficients[tap])
            || std::abs(coefficients[tap]) > maximum_coefficient) {
            return filter_invalid_argument;
        }
    }
    try {
        *output = new FiniteFilter(coefficients);
        return filter_ok;
    } catch (const std::bad_alloc &) {
        return filter_allocation_failed;
    } catch (...) {
        return filter_native_failed;
    }
}

extern "C" std::int32_t dp_dsp_filter_read(void *handle, const double *left,
    const double *right, std::uint32_t actual_input_frames, double *output_left,
    double *output_right, std::uint32_t actual_output_frames) noexcept {
    if (!handle || !left || !right || !output_left || !output_right
        || actual_input_frames != input_frames || actual_output_frames != output_frames) {
        return filter_invalid_argument;
    }
    auto &filter = *static_cast<FiniteFilter *>(handle);
    try {
        return filter.read(left, right, output_left, output_right);
    } catch (const std::bad_alloc &) {
        filter.failed = true;
        return filter_allocation_failed;
    } catch (...) {
        filter.failed = true;
        return filter_native_failed;
    }
}

extern "C" void dp_dsp_filter_destroy(void *handle) noexcept {
    try {
        delete static_cast<FiniteFilter *>(handle);
    } catch (...) {
    }
}
