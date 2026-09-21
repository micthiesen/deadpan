// Developer-only, offline Signalsmith Stretch qualification. No audio device I/O.
#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <new>
#include <stdexcept>
#include <string>
#include <vector>
#include "signalsmith-stretch/signalsmith-stretch.h"

// Count C++ allocations only while library calls run. This does not intercept
// direct malloc calls, OS allocators, or allocations on other threads.
static thread_local bool count_allocations = false;
static thread_local std::uint64_t allocation_calls = 0, allocation_bytes = 0;
static void record_allocation(std::size_t bytes) {
    if (count_allocations) { ++allocation_calls; allocation_bytes += bytes; }
}
void *operator new(std::size_t bytes) {
    record_allocation(bytes);
    if (void *memory = std::malloc(std::max<std::size_t>(bytes, 1))) return memory;
    throw std::bad_alloc();
}
void *operator new[](std::size_t bytes) { return ::operator new(bytes); }
void operator delete(void *memory) noexcept { std::free(memory); }
void operator delete[](void *memory) noexcept { std::free(memory); }
void operator delete(void *memory, std::size_t) noexcept { std::free(memory); }
void operator delete[](void *memory, std::size_t) noexcept { std::free(memory); }
void *operator new(std::size_t bytes, std::align_val_t alignment) {
    record_allocation(bytes);
    void *memory = nullptr;
    if (posix_memalign(&memory, static_cast<std::size_t>(alignment), std::max<std::size_t>(bytes, 1)) == 0) return memory;
    throw std::bad_alloc();
}
void *operator new[](std::size_t bytes, std::align_val_t alignment) { return ::operator new(bytes, alignment); }
void operator delete(void *memory, std::align_val_t) noexcept { std::free(memory); }
void operator delete[](void *memory, std::align_val_t) noexcept { std::free(memory); }
void operator delete(void *memory, std::size_t, std::align_val_t) noexcept { std::free(memory); }
void operator delete[](void *memory, std::size_t, std::align_val_t) noexcept { std::free(memory); }

using Stretch = signalsmith::stretch::SignalsmithStretch<float>;
using Clock = std::chrono::steady_clock;
constexpr int SAMPLE_RATE = 48000, INPUT_FRAMES = 192192, GUARD = 16;
constexpr double PI = 3.14159265358979323846;
constexpr float CANARY = 12345.25f;

static void require(bool condition, const char *message) { if (!condition) throw std::runtime_error(message); }

struct Buffer {
    int frames;
    std::array<std::vector<float>, 2> storage;
    explicit Buffer(int count, float fill = 0) : frames(count) {
        require(count > 0 && count <= 2000000, "bounded buffer length");
        for (auto &channel : storage) {
            channel.assign(count + GUARD * 2, CANARY);
            std::fill(channel.begin() + GUARD, channel.end() - GUARD, fill);
        }
    }
    float *operator[](int channel) { return storage[channel].data() + GUARD; }
    const float *operator[](int channel) const { return storage[channel].data() + GUARD; }
    bool guards() const {
        for (auto &channel : storage) for (int i = 0; i < GUARD; ++i)
            if (channel[i] != CANARY || channel[frames + GUARD + i] != CANARY) return false;
        return true;
    }
};
struct Offset {
    const Buffer &buffer;
    int offset;
    const float *operator[](int channel) const { return buffer[channel] + offset; }
};
struct Output {
    Buffer &buffer;
    int offset;
    float *operator[](int channel) { return buffer[channel] + offset; }
};
struct Stats {
    std::uint64_t calls = 0, allocations = 0, bytes = 0;
    double milliseconds = 0, maximum_call_ms = 0;
};
template<class Function> static void measure(Stats &stats, Function function) {
    allocation_calls = allocation_bytes = 0;
    auto start = Clock::now();
    count_allocations = true;
    try { function(); } catch (...) { count_allocations = false; throw; }
    count_allocations = false;
    double elapsed = std::chrono::duration<double, std::milli>(Clock::now() - start).count();
    ++stats.calls;
    stats.allocations += allocation_calls;
    stats.bytes += allocation_bytes;
    stats.milliseconds += elapsed;
    stats.maximum_call_ms = std::max(stats.maximum_call_ms, elapsed);
}
static void stats_json(const Stats &stats) {
    std::cout << "{\"calls\":" << stats.calls << ",\"new_calls\":" << stats.allocations
              << ",\"new_bytes\":" << stats.bytes << ",\"milliseconds\":" << stats.milliseconds
              << ",\"maximum_call_ms\":" << stats.maximum_call_ms << '}';
}
static int boundary(int position, int input, int output) {
    std::int64_t numerator = std::int64_t(position) * input;
    std::int64_t whole = numerator / output, remainder = numerator % output;
    return static_cast<int>(whole + (remainder > output - remainder || (remainder == output - remainder && whole % 2)));
}
static void save(const Buffer &buffer, const std::string &path) {
    std::ofstream file(path, std::ios::binary);
    require(bool(file), "create PCM fixture");
    for (int frame = 0; frame < buffer.frames; ++frame) {
        float samples[] = {buffer[0][frame], buffer[1][frame]};
        file.write(reinterpret_cast<const char *>(samples), sizeof(samples));
    }
    require(bool(file), "write complete PCM fixture");
}
static Buffer fixture() {
    Buffer result(INPUT_FRAMES);
    for (int n = 0; n < result.frames; ++n) {
        double time = n / double(SAMPLE_RATE);
        if (time >= .25 && time < 1) {
            result[0][n] = float(.6 * std::sin(2 * PI * 440 * time));
            result[1][n] = float(.15 * std::sin(2 * PI * 660 * time + .3));
        } else if (time >= 1.25 && time < 2) {
            double t = time - 1.25;
            result[0][n] = float(.4 * std::sin(2 * PI * (200 * t + 1200 * t * t)));
            result[1][n] = float(.1 * std::sin(2 * PI * (900 * t - 400 * t * t)));
        } else if (time >= 2.5 && time < 3) {
            result[0][n] = float(.1 * std::sin(2 * PI * 880 * time));
            result[1][n] = float(.4 * std::sin(2 * PI * 880 * time));
        } else if (time >= 3.25 && time < 3.75) {
            result[0][n] = float(.06 * std::sin(2 * PI * 440 * time));
            result[1][n] = float(.015 * std::sin(2 * PI * 660 * time + .3));
        }
    }
    result[0][108000] = .8f;
    result[1][110400] = -.2f;
    return result;
}
struct Difference { double peak = 0, rms = 0, reference_rms = 0, normalized_rms = 0; };
static Difference difference(const Buffer &a, int offset_a, const Buffer &b, int offset_b, int count) {
    require(offset_a >= 0 && offset_b >= 0 && offset_a + count <= a.frames && offset_b + count <= b.frames, "comparison bounds");
    Difference result;
    for (int c = 0; c < 2; ++c) for (int n = 0; n < count; ++n) {
        double original = a[c][offset_a + n], delta = original - b[c][offset_b + n];
        result.peak = std::max(result.peak, std::abs(delta));
        result.rms += delta * delta;
        result.reference_rms += original * original;
    }
    result.rms = std::sqrt(result.rms / (2 * count));
    result.reference_rms = std::sqrt(result.reference_rms / (2 * count));
    result.normalized_rms = result.rms / std::max(result.reference_rms, 1e-15);
    return result;
}
static void difference_json(const Difference &value) {
    std::cout << "{\"peak_error\":" << value.peak << ",\"rms_error\":" << value.rms
              << ",\"reference_rms\":" << value.reference_rms << ",\"normalized_rms_error\":" << value.normalized_rms << '}';
}
struct RenderStats { Stats configure, seek, process, flush; int seek_input = 0, process_output = 0, flush_output = 0; };
static void configure(Stretch &stretch, int pitch, Stats &stats) {
    measure(stats, [&] { stretch.presetDefault(2, float(SAMPLE_RATE)); stretch.setTransposeSemitones(float(pitch)); });
}
static RenderStats blocked(Stretch &stretch, const Buffer &input, Buffer &output, const std::vector<int> &pattern) {
    RenderStats result;
    float rate = input.frames / float(output.frames);
    result.seek_input = stretch.outputSeekLength(rate);
    require(input.frames >= result.seek_input, "fixture covers input/output preroll");
    result.process_output = static_cast<int>(output.frames - result.seek_input / rate);
    result.flush_output = output.frames - result.process_output;
    measure(result.seek, [&] { stretch.outputSeek(input, result.seek_input); });
    int consumed = 0, produced = 0;
    std::size_t block = 0;
    int input_frames = input.frames - result.seek_input;
    while (produced < result.process_output) {
        int next_output = std::min(result.process_output, produced + pattern[block++ % pattern.size()]);
        int next_input = boundary(next_output, input_frames, result.process_output);
        Offset in{input, result.seek_input + consumed};
        Output out{output, produced};
        measure(result.process, [&] { stretch.process(in, next_input - consumed, out, next_output - produced); });
        consumed = next_input;
        produced = next_output;
    }
    require(consumed == input_frames && produced + result.flush_output == output.frames, "exact cumulative block boundaries");
    Output tail{output, produced};
    measure(result.flush, [&] { stretch.flush(tail, result.flush_output, rate); });
    require(input.guards() && output.guards(), "audio buffer guards preserved");
    return result;
}
static void render_stats_json(const RenderStats &value) {
    std::cout << "{\"seek_input_frames\":" << value.seek_input << ",\"process_output_frames\":" << value.process_output
              << ",\"flush_output_frames\":" << value.flush_output << ",\"seek\":";
    stats_json(value.seek); std::cout << ",\"process\":"; stats_json(value.process);
    std::cout << ",\"flush\":"; stats_json(value.flush); std::cout << '}';
}
static int peak_position(const Buffer &buffer, int channel) {
    int index = 0;
    for (int n = 1; n < buffer.frames; ++n) if (std::abs(buffer[channel][n]) > std::abs(buffer[channel][index])) index = n;
    return index;
}
static Buffer subsection(const Buffer &input, int start, int frames) {
    require(start >= 0 && start + frames <= input.frames, "subsection bounds");
    Buffer result(frames);
    for (int c = 0; c < 2; ++c) std::copy(input[c] + start, input[c] + start + frames, result[c]);
    return result;
}

int main(int argc, char **argv) {
    try {
        require(argc == 2, "usage: audio_probe OUTPUT_DIRECTORY");
        std::string directory = argv[1];
        Buffer input = fixture();
        save(input, directory + "/input.f32");
        Stretch inventory(1337);
        inventory.presetDefault(2, float(SAMPLE_RATE));
        std::cout << std::setprecision(12) << "{\"version\":[" << Stretch::version[0] << ',' << Stretch::version[1] << ',' << Stretch::version[2]
                  << "],\"sample_rate\":" << SAMPLE_RATE << ",\"channels\":2,\"input_frames\":" << INPUT_FRAMES
                  << ",\"seed\":1337,\"block_samples\":" << inventory.blockSamples() << ",\"interval_samples\":" << inventory.intervalSamples()
                  << ",\"input_latency\":" << inventory.inputLatency() << ",\"output_latency\":" << inventory.outputLatency()
                  << ",\"seek_length\":" << inventory.seekLength() << ",\"cases\":[";
        const int speeds[][2] = {{1, 2}, {3, 4}, {1, 1}, {3, 2}, {2, 1}};
        bool first = true;
        for (const auto &speed : speeds) for (int pitch : {-7, 0, 7}) {
            int output_frames = boundary(INPUT_FRAMES, speed[1], speed[0]);
            std::string name = "speed-" + std::to_string(speed[0]) + "-" + std::to_string(speed[1]) + "-pitch-" + std::to_string(pitch);
            std::cerr << "Qualifying " << name << '\n';
            Buffer exact(output_frames, std::numeric_limits<float>::quiet_NaN());
            Buffer blocks(output_frames, std::numeric_limits<float>::quiet_NaN());
            Buffer alternate(output_frames, std::numeric_limits<float>::quiet_NaN());
            Buffer reset(output_frames, std::numeric_limits<float>::quiet_NaN());
            Stretch stretch(1337), partitioned(1337), other(1337);
            Stats config, exact_stats, reset_stats;
            configure(stretch, pitch, config); configure(partitioned, pitch, config); configure(other, pitch, config);
            bool success = false;
            measure(exact_stats, [&] { success = stretch.exact(input, input.frames, exact, exact.frames); });
            require(success && exact.guards(), "exact whole-file output");
            RenderStats block_stats = blocked(partitioned, input, blocks, {256});
            RenderStats alternate_stats = blocked(other, input, alternate, {257, 509, 127, 1024});
            measure(reset_stats, [&] { partitioned.reset(); });
            RenderStats reset_render = blocked(partitioned, input, reset, {256});
            save(exact, directory + "/" + name + "-exact.f32");
            save(blocks, directory + "/" + name + "-blocks.f32");
            save(alternate, directory + "/" + name + "-alternate.f32");
            save(reset, directory + "/" + name + "-reset.f32");
            if (!first) std::cout << ',';
            first = false;
            std::cout << "{\"name\":\"" << name << "\",\"speed\":[" << speed[0] << ',' << speed[1] << "],\"pitch_semitones\":" << pitch
                      << ",\"output_frames\":" << output_frames << ",\"configure\":";
            stats_json(config); std::cout << ",\"exact\":"; stats_json(exact_stats);
            std::cout << ",\"blocks\":"; render_stats_json(block_stats);
            std::cout << ",\"alternate_blocks\":"; render_stats_json(alternate_stats);
            std::cout << ",\"reset_call\":"; stats_json(reset_stats);
            std::cout << ",\"reset_render\":"; render_stats_json(reset_render);
            std::cout << ",\"exact_vs_blocks\":"; difference_json(difference(exact, 0, blocks, 0, output_frames));
            std::cout << ",\"blocks_vs_alternate\":"; difference_json(difference(blocks, 0, alternate, 0, output_frames));
            std::cout << ",\"blocks_vs_reset\":"; difference_json(difference(blocks, 0, reset, 0, output_frames));
            std::cout << ",\"seeks\":[";
            // A local outputSeek uses only 300 ms of source history. Compare it
            // honestly with linear playback; this is not a cached-state restore.
            const int targets[] = {124800, 28800, 168000, 76800};
            for (int request = 0; request < 4; ++request) {
                int target = targets[request], start = target - 14400;
                Buffer tail = subsection(input, start, input.frames - start);
                Buffer sought(boundary(tail.frames, speed[1], speed[0]), std::numeric_limits<float>::quiet_NaN());
                Stretch seeking(1337); Stats seek_config, seek_stats;
                configure(seeking, pitch, seek_config);
                bool seek_success = false;
                measure(seek_stats, [&] { seek_success = seeking.exact(tail, tail.frames, sought, sought.frames); });
                require(seek_success && sought.guards(), "bounded local-preroll seek output");
                int reference_offset = boundary(target, speed[1], speed[0]);
                int local_offset = boundary(14400, speed[1], speed[0]);
                int count = std::min({4800, output_frames - reference_offset, sought.frames - local_offset});
                Buffer excerpt = subsection(sought, local_offset, count);
                save(excerpt, directory + "/" + name + "-seek-" + std::to_string(request) + ".f32");
                if (request) std::cout << ',';
                std::cout << "{\"target_input_sample\":" << target << ",\"history_input_samples\":14400,\"compared_output_frames\":" << count << ",\"difference\":";
                difference_json(difference(exact, reference_offset, sought, local_offset, count));
                std::cout << ",\"render\":"; stats_json(seek_stats); std::cout << '}';
            }
            std::cout << "]}";
        }
        Buffer impulse(SAMPLE_RATE); impulse[0][12000] = .8f; impulse[1][15000] = -.2f;
        int input_latency = inventory.inputLatency(), output_latency = inventory.outputLatency();
        Buffer padded(SAMPLE_RATE + input_latency), raw(SAMPLE_RATE + input_latency + output_latency, std::numeric_limits<float>::quiet_NaN());
        for (int c = 0; c < 2; ++c) std::copy(impulse[c], impulse[c] + impulse.frames, padded[c]);
        Stretch raw_stretch(1337); raw_stretch.presetDefault(2, float(SAMPLE_RATE));
        raw_stretch.process(padded, padded.frames, raw, padded.frames);
        Output raw_tail{raw, padded.frames}; raw_stretch.flush(raw_tail, output_latency, 1.f);
        Buffer aligned(SAMPLE_RATE, std::numeric_limits<float>::quiet_NaN()); Stretch aligned_stretch(1337); aligned_stretch.presetDefault(2, float(SAMPLE_RATE));
        require(aligned_stretch.exact(impulse, impulse.frames, aligned, aligned.frames), "aligned impulse output");
        save(raw, directory + "/impulse-raw.f32"); save(aligned, directory + "/impulse-aligned.f32");
        std::cout << "],\"latency_impulses\":{\"input_peaks\":[12000,15000],\"raw_peaks\":[" << peak_position(raw, 0) << ',' << peak_position(raw, 1)
                  << "],\"aligned_peaks\":[" << peak_position(aligned, 0) << ',' << peak_position(aligned, 1) << "],\"raw_output_frames\":" << raw.frames << "}";
        Buffer silence(INPUT_FRAMES), silent_output(INPUT_FRAMES, std::numeric_limits<float>::quiet_NaN()); Stretch silent(1337); silent.presetDefault(2, float(SAMPLE_RATE)); silent.setTransposeSemitones(7);
        require(silent.exact(silence, silence.frames, silent_output, silent_output.frames), "silent output");
        save(silent_output, directory + "/silence.f32");
        Buffer isolated = input; std::fill(isolated[1], isolated[1] + isolated.frames, 0.f);
        Buffer isolated_output(INPUT_FRAMES, std::numeric_limits<float>::quiet_NaN()); Stretch isolation(1337); isolation.presetDefault(2, float(SAMPLE_RATE)); isolation.setTransposeSemitones(7);
        require(isolation.exact(isolated, isolated.frames, isolated_output, isolated_output.frames), "isolated channel output");
        save(isolated_output, directory + "/left-only.f32");
        Buffer short_input(1003), short_output(1003, 1.f); short_input[0][500] = .8f;
        Stretch tiny(1337); tiny.presetDefault(2, float(SAMPLE_RATE));
        bool tiny_success = tiny.exact(short_input, short_input.frames, short_output, short_output.frames);
        save(short_output, directory + "/short-output.f32");
        std::cout << ",\"short_input\":{\"input_frames\":1003,\"output_frames\":1003,\"exact_success\":" << (tiny_success ? "true" : "false") << "},\"continuous_controls\":[";
        // Diagnostic control: remove digital-silence spans while preserving the
        // tested processing schedules. This is additional evidence, not a
        // replacement fixture or an exception to failed partition targets.
        Buffer continuous(INPUT_FRAMES);
        for (int n = 0; n < INPUT_FRAMES; ++n) {
            continuous[0][n] = float(.6 * std::sin(2 * PI * 440 * n / SAMPLE_RATE));
            continuous[1][n] = float(.15 * std::sin(2 * PI * 660 * n / SAMPLE_RATE + .3));
        }
        const int controls[][3] = {{1, 1, 7}, {3, 4, 0}};
        for (int i = 0; i < 2; ++i) {
            const auto &control = controls[i];
            int frames = boundary(INPUT_FRAMES, control[1], control[0]);
            Buffer whole(frames, std::numeric_limits<float>::quiet_NaN()), chunks(frames, std::numeric_limits<float>::quiet_NaN());
            Stretch a(1337), b(1337); Stats setup;
            configure(a, control[2], setup); configure(b, control[2], setup);
            require(a.exact(continuous, INPUT_FRAMES, whole, frames), "continuous control exact output");
            blocked(b, continuous, chunks, {256});
            save(whole, directory + "/continuous-" + std::to_string(i) + "-exact.f32");
            save(chunks, directory + "/continuous-" + std::to_string(i) + "-blocks.f32");
            if (i) std::cout << ',';
            std::cout << "{\"speed\":[" << control[0] << ',' << control[1] << "],\"pitch_semitones\":" << control[2]
                      << ",\"output_frames\":" << frames << ",\"exact_vs_blocks\":";
            difference_json(difference(whole, 0, chunks, 0, frames)); std::cout << '}';
            require(continuous.guards() && whole.guards() && chunks.guards(), "continuous control guards");
        }
        std::cout << "]}\n";
        require(raw.guards() && aligned.guards() && silent_output.guards() && isolated_output.guards() && short_output.guards(), "special buffer guards");
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "native harness failed: " << error.what() << '\n';
        return 2;
    }
}
