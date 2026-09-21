#pragma once

// Measured worker-side adapter prototype. This is not an audio device callback.
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <stdexcept>
#include "signalsmith-stretch/signalsmith-stretch.h"

namespace deadpan_audio_probe {

enum class AnalysisWindow { Default120_30, Dense120_15, Short60_15 };

// A source supplies planar 48 kHz stereo samples through source[channel][frame].
// It must stay alive for this renderer's lifetime. Out-of-range context is zero.
template<class Source> class CanonicalStretch {
public:
    static constexpr int quantum = 256;
    static constexpr std::int64_t maximum_frames = std::int64_t(1) << 48;

    CanonicalStretch(const Source &source, std::int64_t input_frames,
                     std::int64_t output_frames, int pitch_semitones,
                     AnalysisWindow window = AnalysisWindow::Dense120_15)
        : source_(source), input_frames_(input_frames), output_frames_(output_frames), engine_(1337) {
        if (input_frames <= 0 || output_frames <= 0 || input_frames > maximum_frames
            || output_frames > maximum_frames || input_frames > output_frames*8
            || output_frames > input_frames*8 || pitch_semitones < -24 || pitch_semitones > 24)
            throw std::invalid_argument("unsupported stretch recipe");
        switch (window) {
            case AnalysisWindow::Default120_30: engine_.presetDefault(2, 48000); break;
            case AnalysisWindow::Dense120_15: engine_.configure(2, 5760, 720); break;
            case AnalysisWindow::Short60_15: engine_.configure(2, 2880, 720); break;
            default: throw std::invalid_argument("unknown analysis window");
        }
        engine_.setTransposeSemitones(float(pitch_semitones));
        float rate = float(input_frames)/float(output_frames);
        lookahead_ = engine_.outputSeekLength(rate);
        // Pad on both sides by reading zero-extended source context. Align the
        // authored origin to the fixed DSP grid; consumers never set this grid.
        int context = int(std::ceil(2*lookahead_/double(rate)));
        leading_output_ = ((context + quantum - 1)/quantum)*quantum;
        cursor_ = -leading_output_;
        View input{source_, input_frames_, boundary(cursor_)};
        engine_.outputSeek(input, lookahead_);
    }

    // Fill a caller-owned planar buffer. End-of-stream may return fewer frames.
    // Calls may request different sizes, but every DSP call stays 256 frames.
    int read(float *left, float *right, int requested) {
        if (requested < 0 || (requested && (!left || !right)))
            throw std::invalid_argument("invalid audio output buffer");
        int count = int(std::min<std::int64_t>(requested, output_frames_ - position_));
        advance(left, right, count);
        return count;
    }

    // Exact cold seek by canonical replay. Work grows with target position.
    // A prepared PCM artifact is the random-access strategy; this is not a
    // finite local-preroll approximation or a copied internal DSP checkpoint.
    void replay_to(std::int64_t target) {
        if (target < position_ || target > output_frames_)
            throw std::invalid_argument("replay target outside remaining output");
        advance(nullptr, nullptr, target - position_);
    }

    std::uint64_t dsp_calls() const { return dsp_calls_; }
    int leading_output() const { return leading_output_; }
    int lookahead() const { return lookahead_; }
    std::int64_t position() const { return position_; }

private:
    struct View {
        const Source &source;
        std::int64_t frames, base;
        struct Channel {
            const Source &source;
            std::int64_t frames, base;
            int channel;
            float operator[](int offset) const {
                std::int64_t frame = base + offset;
                return frame >= 0 && frame < frames ? source[channel][frame] : 0;
            }
        };
        Channel operator[](int channel) const { return {source, frames, base, channel}; }
    };

    std::int64_t boundary(std::int64_t output) const {
        bool negative = output < 0;
        __int128 numerator = __int128(negative ? -output : output)*input_frames_;
        __int128 whole = numerator/output_frames_, remainder = numerator%output_frames_;
        whole += remainder > output_frames_ - remainder
            || (remainder == output_frames_ - remainder && whole%2);
        return negative ? -std::int64_t(whole) : std::int64_t(whole);
    }

    void prepare() {
        do {
            std::int64_t start = boundary(cursor_), end = boundary(cursor_ + quantum);
            View input{source_, input_frames_, lookahead_ + start};
            engine_.process(input, int(end - start), output_, quantum);
            cursor_ += quantum;
            ++dsp_calls_;
        } while (cursor_ <= 0);
        buffered_ = int(std::min<std::int64_t>(quantum, output_frames_ - position_));
        consumed_ = 0;
    }

    void advance(float *left, float *right, std::int64_t count) {
        std::int64_t copied = 0;
        while (copied < count) {
            if (consumed_ == buffered_) prepare();
            int take = int(std::min<std::int64_t>(buffered_ - consumed_, count - copied));
            if (left) {
                std::copy_n(output_[0].data() + consumed_, take, left + copied);
                std::copy_n(output_[1].data() + consumed_, take, right + copied);
            }
            copied += take;
            position_ += take;
            consumed_ += take;
        }
    }

    const Source &source_;
    std::int64_t input_frames_, output_frames_, cursor_ = 0, position_ = 0;
    signalsmith::stretch::SignalsmithStretch<float> engine_;
    std::array<std::array<float, quantum>, 2> output_{};
    int leading_output_ = 0, lookahead_ = 0, buffered_ = 0, consumed_ = 0;
    std::uint64_t dsp_calls_ = 0;
};

} // namespace deadpan_audio_probe
