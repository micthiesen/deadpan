#pragma once

// Measured worker-side adapter prototype. This is not an audio device callback.
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include "signalsmith-stretch/signalsmith-stretch.h"

namespace deadpan_audio_probe {

enum class AnalysisWindow { Default120_30, Dense120_15, Short60_15 };

struct ExactRate {
    std::uint64_t numerator, denominator;
};

// A pitch-preserving speed ramp. Output sample n reads input position
// q(n) = (linear*n + quadratic*n^2)/denominator, an exact rational, on
// 0 <= n <= output_frames, extended linearly with its endpoint slopes outside
// (zero-extended priming before, flush after). Every DSP call still emits one
// 256-sample quantum and consumes exactly the input between its two rounded
// boundaries, so the cumulative input position equals the exact quadratic at
// every quantum boundary; only the rate inside one call is linearised.
struct QuadraticSchedule {
    __int128 linear, quadratic, denominator;
};

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
        // Preserve the original float division for legacy initialization.
        float rate = float(input_frames)/float(output_frames);
        initialize(pitch_semitones, window, rate);
    }

    CanonicalStretch(const Source &source, std::int64_t input_frames,
                     std::int64_t output_frames, ExactRate rate, int pitch_semitones,
                     AnalysisWindow window = AnalysisWindow::Dense120_15)
        : source_(source), input_frames_(input_frames), output_frames_(output_frames),
          rate_numerator_(rate.numerator), rate_denominator_(rate.denominator), engine_(1337) {
        if (input_frames <= 0 || output_frames <= 0 || input_frames > maximum_frames
            || output_frames > maximum_frames || !rate.numerator || !rate.denominator
            || __int128(rate.numerator) > __int128(rate.denominator)*8
            || __int128(rate.denominator) > __int128(rate.numerator)*8
            || pitch_semitones < -24 || pitch_semitones > 24)
            throw std::invalid_argument("unsupported exact-rate stretch recipe");
        std::uint64_t a = rate_numerator_, b = rate_denominator_;
        while (b) {
            std::uint64_t remainder = a%b;
            a = b;
            b = remainder;
        }
        rate_numerator_ /= a;
        rate_denominator_ /= a;
        // Only upstream latency initialization uses a floating approximation.
        // Every input boundary below uses the exact rational directly.
        initialize(pitch_semitones, window,
                   float(double(rate_numerator_)/double(rate_denominator_)));
    }

    CanonicalStretch(const Source &source, std::int64_t input_frames,
                     std::int64_t output_frames, QuadraticSchedule schedule,
                     AnalysisWindow window = AnalysisWindow::Dense120_15)
        : source_(source), input_frames_(input_frames), output_frames_(output_frames),
          ramp_(true), schedule_(schedule), engine_(1337) {
        constexpr std::int64_t maximum_ramp_frames = std::int64_t(1) << 32;
        auto slope = [&](std::int64_t n) {
            return checked_add(schedule.linear,
                               checked_mul(checked_mul(2, schedule.quadratic), n));
        };
        if (input_frames <= 0 || output_frames <= 0 || input_frames > maximum_ramp_frames
            || output_frames > maximum_ramp_frames || schedule.denominator <= 0)
            throw std::invalid_argument("unsupported ramp stretch recipe");
        // The first slope is the authored starting speed, within 1/8..8. The
        // slope at output_frames may overshoot the authored end speed by the
        // part of one sample past the exact end, so it is admitted in 1/9..9.
        auto first = slope(0), last = slope(output_frames);
        auto denominator = schedule.denominator;
        if (checked_mul(first, 8) < denominator || first > checked_mul(denominator, 8)
            || checked_mul(last, 9) < denominator || last > checked_mul(denominator, 9))
            throw std::invalid_argument("ramp speeds outside the admitted range");
        initialize(0, window, float(double(first)/double(denominator)));
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

    // Real source samples needed by the next bounded read. Priming may run
    // many negative quanta, but their union ends at the first output quantum.
    // The source itself is zero-extended by View, so callers never supply pad.
    std::array<std::int64_t, 2> required_input(int requested) const {
        if (requested < 0 || requested > quantum)
            throw std::invalid_argument("invalid streamed output count");
        auto count = std::min<std::int64_t>(requested, output_frames_ - position_);
        if (count <= buffered_ - consumed_) return {0, 0};
        auto first = feed(cursor_);
        auto last = feed(std::max<std::int64_t>(cursor_, 0) + quantum);
        return {std::clamp(first, std::int64_t(0), input_frames_),
                std::clamp(last, std::int64_t(0), input_frames_)};
    }

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

    static __int128 checked_mul(__int128 a, __int128 b) {
        __int128 result;
        if (__builtin_mul_overflow(a, b, &result)) throw std::overflow_error("ramp schedule");
        return result;
    }
    static __int128 checked_add(__int128 a, __int128 b) {
        __int128 result;
        if (__builtin_add_overflow(a, b, &result)) throw std::overflow_error("ramp schedule");
        return result;
    }

    // The input sample fed before the call that emits output `cursor`. A
    // constant rate keeps its original lookahead arithmetic. A ramp feeds the
    // analysis latency plus the exact position one output latency ahead,
    // because output sample n leaves synthesis output-latency samples after
    // the call that analysed it.
    std::int64_t feed(std::int64_t cursor) const {
        return ramp_ ? input_latency_ + boundary(cursor + output_latency_)
                     : lookahead_ + boundary(cursor);
    }

    // q(n) rounded half-even, with exact checked arithmetic throughout.
    std::int64_t ramp_boundary(std::int64_t output) const {
        const auto &s = schedule_;
        __int128 n = output, end = output_frames_, numerator;
        if (output <= 0) {
            numerator = checked_mul(s.linear, n);
        } else if (output <= output_frames_) {
            numerator = checked_add(checked_mul(s.linear, n),
                                    checked_mul(checked_mul(s.quadratic, n), n));
        } else {
            __int128 at_end = checked_add(checked_mul(s.linear, end),
                                          checked_mul(checked_mul(s.quadratic, end), end));
            __int128 slope = checked_add(s.linear, checked_mul(checked_mul(2, s.quadratic), end));
            numerator = checked_add(at_end, checked_mul(slope, n - end));
        }
        __int128 whole = numerator/s.denominator, remainder = numerator%s.denominator;
        if (remainder < 0) {
            whole -= 1;
            remainder += s.denominator;
        }
        whole += remainder > s.denominator - remainder
            || (remainder == s.denominator - remainder && (whole & 1));
        if (whole > std::numeric_limits<std::int64_t>::max()
            || whole < std::numeric_limits<std::int64_t>::min())
            throw std::overflow_error("ramp schedule");
        return std::int64_t(whole);
    }

    std::int64_t boundary(std::int64_t output) const {
        if (ramp_) return ramp_boundary(output);
        bool negative = output < 0;
        __int128 rate_numerator = rate_denominator_ ? __int128(rate_numerator_) : input_frames_;
        __int128 rate_denominator = rate_denominator_ ? __int128(rate_denominator_) : output_frames_;
        // Output is bounded by 2^48 plus one quantum, and numerator by u64.
        // Their product fits signed int128, including negative priming context.
        __int128 numerator = __int128(negative ? -output : output)*rate_numerator;
        __int128 whole = numerator/rate_denominator, remainder = numerator%rate_denominator;
        whole += remainder > rate_denominator - remainder
            || (remainder == rate_denominator - remainder && whole%2);
        return negative ? -std::int64_t(whole) : std::int64_t(whole);
    }

    void initialize(int pitch_semitones, AnalysisWindow window, float rate) {
        switch (window) {
            case AnalysisWindow::Default120_30: engine_.presetDefault(2, 48000); break;
            case AnalysisWindow::Dense120_15: engine_.configure(2, 5760, 720); break;
            case AnalysisWindow::Short60_15: engine_.configure(2, 2880, 720); break;
            default: throw std::invalid_argument("unknown analysis window");
        }
        engine_.setTransposeSemitones(float(pitch_semitones));
        lookahead_ = engine_.outputSeekLength(rate);
        input_latency_ = engine_.inputLatency();
        output_latency_ = engine_.outputLatency();
        // Pad on both sides by reading zero-extended source context. Align the
        // authored origin to the fixed DSP grid; consumers never set this grid.
        int context = int(std::ceil(2*lookahead_/double(rate)));
        leading_output_ = ((context + quantum - 1)/quantum)*quantum;
        cursor_ = -leading_output_;
        View input{source_, input_frames_, feed(cursor_) - lookahead_};
        engine_.outputSeek(input, lookahead_);
    }

    void prepare() {
        do {
            std::int64_t start = feed(cursor_), end = feed(cursor_ + quantum);
            View input{source_, input_frames_, start};
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
    std::uint64_t rate_numerator_ = 0, rate_denominator_ = 0;
    bool ramp_ = false;
    QuadraticSchedule schedule_{0, 0, 1};
    std::int64_t input_latency_ = 0, output_latency_ = 0;
    signalsmith::stretch::SignalsmithStretch<float> engine_;
    std::array<std::array<float, quantum>, 2> output_{};
    int leading_output_ = 0, lookahead_ = 0, buffered_ = 0, consumed_ = 0;
    std::uint64_t dsp_calls_ = 0;
};

} // namespace deadpan_audio_probe
