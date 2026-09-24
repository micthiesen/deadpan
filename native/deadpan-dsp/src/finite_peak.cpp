#include "finite_peak.h"

#if defined(__FAST_MATH__) || (defined(__FINITE_MATH_ONLY__) && __FINITE_MATH_ONLY__) \
    || defined(SIGNALSMITH_USE_ACCELERATE) || defined(SIGNALSMITH_USE_IPP) \
    || defined(SIGNALSMITH_USE_PFFFT) || defined(SIGNALSMITH_USE_PFFFT_DOUBLE)
#error "deadpan-dsp requires ordinary floating point and the portable FFT"
#endif

#include <signalsmith-linear/fft.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <complex>
#include <cstddef>
#include <cstdint>
#include <new>

namespace {
constexpr std::size_t row_count = 15;
constexpr std::size_t tap_count = 129;
constexpr std::size_t radius = 64;
constexpr std::size_t input_frames = 1152;
constexpr std::size_t output_frames = 1024;
constexpr std::size_t fft_size = 1280;
constexpr std::size_t bin_count = fft_size / 2;
constexpr float maximum_input_peak = 16.0f;
constexpr double maximum_coefficient_magnitude = 16.0;

using Fft = signalsmith::linear::RealFFT<double>;
using Complex = Fft::Complex;

enum PeakStatus : std::int32_t {
    peak_ok = 0,
    peak_invalid_argument = 1,
    peak_allocation_failed = 2,
    peak_native_failed = 3,
    peak_nonfinite_output = 4,
    peak_poisoned = 5,
};

struct FixedPeakBank {
    Fft fft;
    std::array<std::array<double, tap_count>, row_count> weights{};
    std::array<std::array<Complex, bin_count>, row_count> kernels{};
    std::array<double, fft_size> kernel_time{};
    std::array<double, fft_size> input_left{};
    std::array<double, fft_size> input_right{};
    std::array<double, fft_size> inverse_left{};
    std::array<double, fft_size> inverse_right{};
    std::array<Complex, bin_count> spectrum_left{};
    std::array<Complex, bin_count> spectrum_right{};
    std::array<Complex, bin_count> product_left{};
    std::array<Complex, bin_count> product_right{};
    bool failed = false;

    explicit FixedPeakBank(const double *coefficients) : fft(fft_size) {
        for (std::size_t row = 0; row < row_count; ++row) {
            for (std::size_t tap = 0; tap < tap_count; ++tap) {
                weights[row][tap] = coefficients[row * tap_count + tap];
            }
            std::fill(kernel_time.begin(), kernel_time.end(), 0.0);
            for (std::size_t tap = 0; tap < tap_count; ++tap) {
                kernel_time[tap] = weights[row][tap_count - 1 - tap];
            }
            fft.fft(kernel_time.data(), kernels[row].data());
        }
    }

    std::int32_t read(const float *left, const float *right, double *output) {
        if (failed) return peak_poisoned;
        for (std::size_t frame = 0; frame < input_frames; ++frame) {
            if (!std::isfinite(left[frame]) || !std::isfinite(right[frame])
                || std::abs(left[frame]) > maximum_input_peak
                || std::abs(right[frame]) > maximum_input_peak) {
                return peak_invalid_argument;
            }
        }

        std::array<double, output_frames> staged{};
        std::fill(input_left.begin(), input_left.end(), 0.0);
        std::fill(input_right.begin(), input_right.end(), 0.0);
        for (std::size_t frame = 0; frame < input_frames; ++frame) {
            input_left[frame] = static_cast<double>(left[frame]);
            input_right[frame] = static_cast<double>(right[frame]);
        }
        fft.fft(input_left.data(), spectrum_left.data());
        fft.fft(input_right.data(), spectrum_right.data());

        for (std::size_t row = 0; row < row_count; ++row) {
            const auto &kernel = kernels[row];
            // RealFFT packs DC and Nyquist into the real and imaginary parts
            // of bin zero. They are independent real multiplications.
            product_left[0] = {spectrum_left[0].real() * kernel[0].real(),
                               spectrum_left[0].imag() * kernel[0].imag()};
            product_right[0] = {spectrum_right[0].real() * kernel[0].real(),
                                spectrum_right[0].imag() * kernel[0].imag()};
            for (std::size_t bin = 1; bin < bin_count; ++bin) {
                product_left[bin] = spectrum_left[bin] * kernel[bin];
                product_right[bin] = spectrum_right[bin] * kernel[bin];
            }
            fft.ifft(product_left.data(), inverse_left.data());
            fft.ifft(product_right.data(), inverse_right.data());
            for (std::size_t frame = 0; frame < output_frames; ++frame) {
                const std::size_t at = 2 * radius + frame;
                const double left_peak = std::abs(inverse_left[at] / double(fft_size));
                const double right_peak = std::abs(inverse_right[at] / double(fft_size));
                if (!std::isfinite(left_peak) || !std::isfinite(right_peak)) {
                    failed = true;
                    return peak_nonfinite_output;
                }
                staged[frame] = std::max(staged[frame], std::max(left_peak, right_peak));
            }
        }
        for (double value : staged) {
            if (!std::isfinite(value) || value < 0.0) {
                failed = true;
                return peak_nonfinite_output;
            }
        }
        std::copy(staged.begin(), staged.end(), output);
        return peak_ok;
    }
};
} // namespace

extern "C" std::int32_t dp_dsp_peak_create(const double *coefficients,
    std::size_t coefficient_count, void **output) noexcept {
    if (!output) return peak_invalid_argument;
    *output = nullptr;
    if (!coefficients || coefficient_count != row_count * tap_count) {
        return peak_invalid_argument;
    }
    for (std::size_t i = 0; i < coefficient_count; ++i) {
        if (!std::isfinite(coefficients[i])
            || std::abs(coefficients[i]) > maximum_coefficient_magnitude) {
            return peak_invalid_argument;
        }
    }
    try {
        *output = new FixedPeakBank(coefficients);
        return peak_ok;
    } catch (const std::bad_alloc &) {
        return peak_allocation_failed;
    } catch (...) {
        return peak_native_failed;
    }
}

extern "C" std::int32_t dp_dsp_peak_read(void *handle, const float *left,
    const float *right, std::uint32_t actual_input_frames, double *output,
    std::uint32_t actual_output_frames) noexcept {
    if (!handle || !left || !right || !output || actual_input_frames != input_frames
        || actual_output_frames != output_frames) {
        return peak_invalid_argument;
    }
    auto &bank = *static_cast<FixedPeakBank *>(handle);
    try {
        return bank.read(left, right, output);
    } catch (const std::bad_alloc &) {
        bank.failed = true;
        return peak_allocation_failed;
    } catch (...) {
        bank.failed = true;
        return peak_native_failed;
    }
}

extern "C" void dp_dsp_peak_destroy(void *handle) noexcept {
    try {
        delete static_cast<FixedPeakBank *>(handle);
    } catch (...) {
    }
}
