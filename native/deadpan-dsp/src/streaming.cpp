#include "adapter.h"

#if defined(__FAST_MATH__) || (defined(__FINITE_MATH_ONLY__) && __FINITE_MATH_ONLY__) \
    || defined(SIGNALSMITH_USE_ACCELERATE) || defined(SIGNALSMITH_USE_IPP) \
    || defined(SIGNALSMITH_USE_PFFFT) || defined(SIGNALSMITH_USE_PFFFT_DOUBLE)
#error "deadpan-dsp requires ordinary floating point and the portable FFT"
#endif

#include "canonical.hpp"
#include <new>

namespace {
constexpr std::uint32_t maximum_window = 65536;
enum Status : std::int32_t {
    ok = 0, invalid_argument = 1, allocation_failed = 2, native_failed = 3,
    nonfinite_output = 4, poisoned = 5
};
struct Window {
    const float *left = nullptr, *right = nullptr;
    std::int64_t start = 0, frames = 0;
    struct Channel {
        const Window &window;
        int channel;
        float operator[](std::int64_t at) const {
            if (at < window.start || at - window.start >= window.frames)
                throw std::out_of_range("stretch input outside admitted window");
            return (channel ? window.right : window.left)[at - window.start];
        }
    };
    Channel operator[](int channel) const { return {*this, channel}; }
};
struct Engine {
    Window source;
    deadpan_audio_probe::CanonicalStretch<Window> renderer;
    bool failed = false;
    Engine(std::uint32_t input, std::uint32_t output,
           deadpan_audio_probe::ExactRate rate, std::int32_t pitch)
        : renderer(source, input, output, rate, pitch) {}
    Engine(const Engine &) = delete;
    Engine &operator=(const Engine &) = delete;
};
struct Borrow {
    Window &window;
    ~Borrow() { window = {}; }
};
} // namespace

extern "C" std::int32_t dp_dsp_stream_create(std::uint32_t input,
    std::uint32_t output, std::uint64_t numerator, std::uint64_t denominator,
    std::int32_t pitch, void **handle) noexcept {
    if (!handle) return invalid_argument;
    *handle = nullptr;
    if (!input || !output || !numerator || !denominator
        || __int128(numerator) > __int128(denominator)*8
        || __int128(denominator) > __int128(numerator)*8 || pitch < -24 || pitch > 24)
        return invalid_argument;
    try {
        *handle = new Engine(input, output, {numerator, denominator}, pitch);
        return ok;
    } catch (const std::bad_alloc &) { return allocation_failed; }
      catch (...) { return native_failed; }
}

extern "C" std::int32_t dp_dsp_stream_required(void *handle,
    std::uint32_t requested, std::uint32_t *start, std::uint32_t *frames) noexcept {
    if (!start || !frames) return invalid_argument;
    *start = *frames = 0;
    if (!handle || requested > 256) return invalid_argument;
    auto &engine = *static_cast<Engine *>(handle);
    if (engine.failed) return poisoned;
    try {
        auto range = engine.renderer.required_input(int(requested));
        if (range[0] < 0 || range[1] < range[0] || range[1] - range[0] > maximum_window)
            return native_failed;
        *start = std::uint32_t(range[0]);
        *frames = std::uint32_t(range[1] - range[0]);
        return ok;
    } catch (...) { return native_failed; }
}

extern "C" std::int32_t dp_dsp_stream_read(void *handle, std::uint32_t input_start,
    const float *input_left, const float *input_right, std::uint32_t input_frames,
    float *left, float *right, std::uint32_t requested, std::uint32_t *written) noexcept {
    if (!written) return invalid_argument;
    *written = 0;
    if (!handle || requested > 256 || input_frames > maximum_window
        || (input_frames && (!input_left || !input_right))
        || (requested && (!left || !right))) return invalid_argument;
    auto &engine = *static_cast<Engine *>(handle);
    if (engine.failed) return poisoned;
    std::uint32_t start = 0, frames = 0;
    auto status = dp_dsp_stream_required(handle, requested, &start, &frames);
    if (status != ok) return status;
    if (start != input_start || frames != input_frames) return invalid_argument;
    for (std::uint32_t i = 0; i < frames; ++i) {
        if (!std::isfinite(input_left[i]) || !std::isfinite(input_right[i])
            || std::abs(input_left[i]) > 16.0f || std::abs(input_right[i]) > 16.0f)
            return invalid_argument;
    }
    engine.source = {input_left, input_right, start, frames};
    Borrow borrow{engine.source};
    try {
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

extern "C" void dp_dsp_stream_destroy(void *handle) noexcept {
    try { delete static_cast<Engine *>(handle); } catch (...) {}
}
