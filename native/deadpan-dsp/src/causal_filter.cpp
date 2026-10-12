#include "causal_filter.h"

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
#include <vector>

namespace {
constexpr std::size_t maximum_taps = 384001;
constexpr std::size_t output_frames = 4096;

std::size_t fft_length(std::size_t taps) {
    std::size_t result = output_frames;
    while (result < taps - 1 + output_frames) result *= 2;
    return result;
}
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

struct CausalFilter {
    const std::size_t history, input_frames, fft_size, bin_count;
    Fft fft;
    std::array<std::vector<Complex>, 2> kernels;
    std::vector<double> time;
    std::vector<Complex> spectrum, product;
    std::array<std::array<double, output_frames>, 2> staged{};
    bool failed = false;

    CausalFilter(const double *left, const double *right, std::size_t taps)
        : history(taps - 1), input_frames(history + output_frames),
          fft_size(fft_length(taps)), bin_count(fft_size / 2), fft(fft_size),
          time(fft_size), spectrum(bin_count), product(bin_count) {
        const double *coefficients[2] = {left, right};
        for (std::size_t channel = 0; channel < 2; ++channel) {
            kernels[channel].resize(bin_count);
            std::fill(time.begin(), time.end(), 0.0);
            std::copy_n(coefficients[channel], taps, time.begin());
            fft.fft(time.data(), kernels[channel].data());
        }
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
            // h[j] is the weight at lag j, with no look-ahead. Zero fill the
            // unused FFT suffix, then discard the history circular outputs.
            std::copy_n(inputs[channel], input_frames, time.begin());
            std::fill(time.begin() + input_frames, time.end(), 0.0);
            fft.fft(time.data(), spectrum.data());
            const auto &kernel = kernels[channel];
            // RealFFT packs DC and Nyquist as two independent real bins.
            product[0] = {spectrum[0].real() * kernel[0].real(),
                          spectrum[0].imag() * kernel[0].imag()};
            for (std::size_t bin = 1; bin < bin_count; ++bin) {
                product[bin] = spectrum[bin] * kernel[bin];
            }
            fft.ifft(product.data(), time.data());
            for (std::size_t frame = 0; frame < output_frames; ++frame) {
                const double value = time[history + frame] / double(fft_size);
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

extern "C" std::int32_t dp_dsp_causal_create(const double *left, const double *right,
    std::size_t coefficient_count, void **output) noexcept {
    if (!output) return filter_invalid_argument;
    *output = nullptr;
    if (!left || !right || !coefficient_count || coefficient_count > maximum_taps) return filter_invalid_argument;
    for (std::size_t tap = 0; tap < coefficient_count; ++tap) {
        if (!std::isfinite(left[tap]) || !std::isfinite(right[tap])
            || std::abs(left[tap]) > maximum_coefficient
            || std::abs(right[tap]) > maximum_coefficient) {
            return filter_invalid_argument;
        }
    }
    try {
        *output = new CausalFilter(left, right, coefficient_count);
        return filter_ok;
    } catch (const std::bad_alloc &) {
        return filter_allocation_failed;
    } catch (...) {
        return filter_native_failed;
    }
}

extern "C" std::int32_t dp_dsp_causal_read(void *handle, const double *left,
    const double *right, std::uint32_t actual_input_frames, double *output_left,
    double *output_right, std::uint32_t actual_output_frames) noexcept {
    if (!handle || !left || !right || !output_left || !output_right
        || actual_output_frames != output_frames) {
        return filter_invalid_argument;
    }
    auto &filter = *static_cast<CausalFilter *>(handle);
    if (actual_input_frames != filter.input_frames) return filter_invalid_argument;
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

extern "C" void dp_dsp_causal_destroy(void *handle) noexcept {
    try {
        delete static_cast<CausalFilter *>(handle);
    } catch (...) {
    }
}
