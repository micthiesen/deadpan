#include "adapter.h"

// These switches change the qualified DSP or its finite-value checks.
#if defined(__FAST_MATH__) || (defined(__FINITE_MATH_ONLY__) && __FINITE_MATH_ONLY__) \
    || defined(SIGNALSMITH_USE_ACCELERATE) || defined(SIGNALSMITH_USE_IPP) \
    || defined(SIGNALSMITH_USE_PFFFT) || defined(SIGNALSMITH_USE_PFFFT_DOUBLE)
#error "deadpan-dsp requires ordinary floating point and the portable FFT"
#endif

#include "canonical.hpp"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <new>

namespace {
constexpr std::uint32_t maximum_input_frames = 1U << 20;
constexpr float maximum_input_peak = 16.0f;
enum Status : std::int32_t {
    ok = 0, invalid_argument = 1, allocation_failed = 2, native_failed = 3,
    nonfinite_output = 4, poisoned = 5
};

struct Source {
    const float *left, *right;
    const float *operator[](int channel) const { return channel == 0 ? left : right; }
};
struct Engine {
    // The renderer references this stable member, which is destroyed after it.
    Source source;
    deadpan_audio_probe::CanonicalStretch<Source> renderer;
    bool failed = false;
    Engine(const float *left, const float *right, std::uint32_t input_frames,
           std::uint32_t output_frames, std::int32_t pitch)
        : source{left, right}, renderer(source, input_frames, output_frames, pitch) {}
    Engine(const Engine &) = delete;
    Engine &operator=(const Engine &) = delete;
};
} // namespace

extern "C" std::int32_t dp_dsp_create(const float *left, const float *right,
    std::uint32_t input_frames, std::uint32_t output_frames, std::int32_t pitch,
    void **output) noexcept {
    if (!output) return invalid_argument;
    *output = nullptr;
    if (!left || !right || !input_frames || input_frames > maximum_input_frames
        || !output_frames || output_frames > std::uint64_t(input_frames)*8
        || input_frames > std::uint64_t(output_frames)*8 || pitch < -24 || pitch > 24)
        return invalid_argument;
    try {
        for (std::uint32_t i = 0; i < input_frames; ++i) {
            if (!std::isfinite(left[i]) || !std::isfinite(right[i])
                || std::abs(left[i]) > maximum_input_peak || std::abs(right[i]) > maximum_input_peak)
                return invalid_argument;
        }
        *output = new Engine(left, right, input_frames, output_frames, pitch);
        return ok;
    } catch (const std::bad_alloc &) { return allocation_failed; }
      catch (...) { return native_failed; }
}

extern "C" std::int32_t dp_dsp_read(void *handle, float *left, float *right,
    std::uint32_t requested, std::uint32_t *written) noexcept {
    if (!written) return invalid_argument;
    *written = 0;
    if (!handle || requested > 256 || (requested && (!left || !right))) return invalid_argument;
    auto &engine = *static_cast<Engine *>(handle);
    if (engine.failed) return poisoned;
    try {
        // Stage the result so errors never expose partially written output.
        std::array<float, 256> first{}, second{};
        int count = engine.renderer.read(first.data(), second.data(), int(requested));
        if (count < 0 || std::uint32_t(count) > requested) {
            engine.failed = true;
            return native_failed;
        }
        for (int i = 0; i < count; ++i) {
            if (!std::isfinite(first[i]) || !std::isfinite(second[i])) {
                engine.failed = true;
                return nonfinite_output;
            }
        }
        if (count) {
            std::copy_n(first.data(), count, left);
            std::copy_n(second.data(), count, right);
        }
        *written = std::uint32_t(count);
        return ok;
    } catch (const std::bad_alloc &) { engine.failed = true; return allocation_failed; }
      catch (...) { engine.failed = true; return native_failed; }
}

extern "C" void dp_dsp_destroy(void *handle) noexcept {
    // The engine has only nonthrowing standard-library and DSP destructors.
    // Still keep every C entrypoint an exception boundary.
    try { delete static_cast<Engine *>(handle); } catch (...) {}
}
