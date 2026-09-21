// Reuse the frozen candidate fixture, guards, and measurement instrumentation.
// Its standalone experiment remains unchanged and its entrypoint is not run.
#define main deadpan_raw_candidate_entrypoint
#include "probe.cpp"
#undef main
#include "canonical.hpp"
#include <cerrno>
#include <fcntl.h>
#include <optional>
#include <unistd.h>

using Renderer = deadpan_audio_probe::CanonicalStretch<Buffer>;
using AnalysisWindow = deadpan_audio_probe::AnalysisWindow;
static AnalysisWindow selected_window = AnalysisWindow::Dense120_15;
struct ConsumerStats {
    Stats configure, consume;
    std::uint64_t dsp_calls = 0;
    int leading_output = 0, lookahead_input = 0;
};
static void consumer_stats_json(const ConsumerStats &stats) {
    std::cout << "{\"calls\":" << stats.consume.calls << ",\"new_calls\":" << stats.consume.allocations
              << ",\"new_bytes\":" << stats.consume.bytes << ",\"milliseconds\":" << stats.consume.milliseconds
              << ",\"maximum_call_ms\":" << stats.consume.maximum_call_ms
              << ",\"dsp_calls\":" << stats.dsp_calls << ",\"maximum_buffer_frames\":" << Renderer::quantum
              << ",\"leading_output_frames\":" << stats.leading_output << ",\"lookahead_input_frames\":" << stats.lookahead_input
              << ",\"configure\":";
    stats_json(stats.configure);
    std::cout << '}';
}
static ConsumerStats render(const Buffer &input, Buffer &output, int pitch, const std::vector<int> &requests) {
    ConsumerStats stats;
    std::optional<Renderer> renderer;
    measure(stats.configure, [&] { renderer.emplace(input, input.frames, output.frames, pitch, selected_window); });
    int position = 0;
    std::size_t request = 0;
    while (position < output.frames) {
        int count = std::min(output.frames - position, requests[request++%requests.size()]);
        measure(stats.consume, [&] {
            require(renderer->read(output[0] + position, output[1] + position, count) == count,
                    "canonical render wrote the requested count");
        });
        position += count;
    }
    require(renderer->read(output[0], output[1], 1) == 0, "canonical EOF");
    require(renderer->read(nullptr, nullptr, 0) == 0, "empty consumer request");
    require(input.guards() && output.guards(), "canonical render guards");
    stats.dsp_calls = renderer->dsp_calls();
    stats.leading_output = renderer->leading_output();
    stats.lookahead_input = renderer->lookahead();
    return stats;
}
static ConsumerStats replay(const Buffer &input, int output_frames, int pitch, int start, Buffer &window) {
    ConsumerStats stats;
    std::optional<Renderer> renderer;
    measure(stats.configure, [&] { renderer.emplace(input, input.frames, output_frames, pitch, selected_window); });
    measure(stats.consume, [&] {
        renderer->replay_to(start);
        require(renderer->read(window[0], window[1], window.frames) == window.frames, "replay seek count");
    });
    require(input.guards() && window.guards(), "replay guards");
    stats.dsp_calls = renderer->dsp_calls();
    stats.leading_output = renderer->leading_output();
    stats.lookahead_input = renderer->lookahead();
    return stats;
}
class PreparedPcm {
public:
    explicit PreparedPcm(const std::string &path, int frames) : frames_(frames), fd_(open(path.c_str(), O_RDONLY)) {
        require(fd_ >= 0, "open prepared canonical PCM");
    }
    ~PreparedPcm() { if (fd_ >= 0) close(fd_); }
    PreparedPcm(const PreparedPcm &) = delete;
    PreparedPcm &operator=(const PreparedPcm &) = delete;
    void read(int start, Buffer &output) {
        require(start >= 0 && start + output.frames <= frames_, "prepared seek range");
        for (int copied = 0; copied < output.frames;) {
            int count = std::min(Renderer::quantum, output.frames - copied);
            std::size_t needed = count*2*sizeof(float), received = 0;
            while (received < needed) {
                ssize_t got = pread(fd_, reinterpret_cast<char *>(scratch_.data()) + received,
                                    needed - received, off_t(start + copied)*2*sizeof(float) + received);
                if (got < 0 && errno == EINTR) continue;
                require(got > 0, "read complete prepared PCM slice");
                received += std::size_t(got);
            }
            for (int frame = 0; frame < count; ++frame) for (int channel = 0; channel < 2; ++channel)
                output[channel][copied + frame] = scratch_[frame*2 + channel];
            copied += count;
        }
        require(output.guards(), "prepared PCM output guards");
    }
private:
    int frames_, fd_;
    std::array<float, Renderer::quantum*2> scratch_{};
};
static void qualify_case(const Buffer &input, const std::string &input_file, const std::string &kind,
                         int output_frames, int pitch, const std::string &name, const std::string &directory) {
    std::cerr << "Canonical " << name << '\n';
    Buffer preview(output_frames, std::numeric_limits<float>::quiet_NaN());
    Buffer irregular(output_frames, std::numeric_limits<float>::quiet_NaN());
    Buffer exported(output_frames, std::numeric_limits<float>::quiet_NaN());
    auto preview_stats = render(input, preview, pitch, {256});
    auto irregular_stats = render(input, irregular, pitch, {1, 257, 509, 127, 1024});
    auto export_stats = render(input, exported, pitch, {4096});
    save(preview, directory + "/" + name + "-preview.f32");
    save(irregular, directory + "/" + name + "-irregular.f32");
    save(exported, directory + "/" + name + "-export.f32");
    std::cout << "{\"name\":\"" << name << "\",\"fixture_kind\":\"" << kind << "\",\"input_file\":\"" << input_file
              << "\",\"input_frames\":" << input.frames << ",\"output_frames\":" << output_frames
              << ",\"pitch_semitones\":" << pitch << ",\"renders\":{\"preview\":";
    consumer_stats_json(preview_stats);
    std::cout << ",\"irregular\":"; consumer_stats_json(irregular_stats);
    std::cout << ",\"export\":"; consumer_stats_json(export_stats);
    std::cout << "},\"seeks\":[";
    PreparedPcm prepared(directory + "/" + name + "-preview.f32", output_frames);
    int index = 0;
    for (int start : {output_frames*4/5, output_frames/7, output_frames - 1, output_frames/2}) {
        int frames = std::min(4800, output_frames - start);
        Buffer replayed(frames, std::numeric_limits<float>::quiet_NaN());
        Buffer cached(frames, std::numeric_limits<float>::quiet_NaN());
        auto replay_stats = replay(input, output_frames, pitch, start, replayed);
        ConsumerStats cache_stats;
        measure(cache_stats.consume, [&] { prepared.read(start, cached); });
        save(replayed, directory + "/" + name + "-replay-" + std::to_string(index) + ".f32");
        save(cached, directory + "/" + name + "-cached-" + std::to_string(index) + ".f32");
        if (index++) std::cout << ',';
        std::cout << "{\"start_frame\":" << start << ",\"frames\":" << frames << ",\"replay\":";
        consumer_stats_json(replay_stats);
        std::cout << ",\"cached\":"; consumer_stats_json(cache_stats);
        std::cout << '}';
    }
    std::cout << "]}";
}
static int invalid_recipe_checks() {
    Buffer source(32);
    int rejected = 0;
    for (const auto &recipe : std::vector<std::array<std::int64_t, 3>>{
             {0, 32, 0}, {32, 0, 0}, {-1, 32, 0}, {32, -1, 0}, {32, 32, -25},
             {32, 32, 25}, {32, 289, 0}, {289, 32, 0}, {Renderer::maximum_frames + 1, 32, 0}}) {
        try { Renderer renderer(source, recipe[0], recipe[1], int(recipe[2])); }
        catch (const std::invalid_argument &) { ++rejected; }
    }
    require(rejected == 9, "reject invalid recipes before processing");
    Renderer renderer(source, 32, 32, 0);
    renderer.replay_to(20);
    for (int target : {-1, 19, 33}) {
        try { renderer.replay_to(target); }
        catch (const std::invalid_argument &) { ++rejected; }
        require(renderer.position() == 20, "invalid seek preserves position");
    }
    require(rejected == 12, "reject invalid replay targets");
    return rejected;
}
int main(int argc, char **argv) {
    try {
        require(argc == 3, "usage: canonical_audio_probe OUTPUT_DIRECTORY WINDOW (120-30, 120-15, or 60-15)");
        std::string directory = argv[1];
        std::string window = argv[2];
        if (window == "120-30") selected_window = AnalysisWindow::Default120_30;
        else if (window == "120-15") selected_window = AnalysisWindow::Dense120_15;
        else if (window == "60-15") selected_window = AnalysisWindow::Short60_15;
        else throw std::invalid_argument("unsupported analysis window");
        int rejected = invalid_recipe_checks();
        Buffer input = fixture();
        save(input, directory + "/input.f32");
        std::cout << std::setprecision(12) << "{\"sample_rate\":48000,\"channels\":2,\"quantum\":256,\"analysis_window_ms\":\"" << window
                  << "\",\"invalid_requests_rejected\":"
                  << rejected << ",\"cases\":[";
        const int speeds[][2] = {{1, 2}, {3, 4}, {1, 1}, {3, 2}, {2, 1}};
        bool first = true;
        for (const auto &speed : speeds) for (int pitch : {-7, 0, 7}) {
            if (!first) std::cout << ',';
            first = false;
            std::string name = "mixed-" + std::to_string(speed[0]) + "-" + std::to_string(speed[1]) + "-pitch-" + std::to_string(pitch);
            qualify_case(input, "input.f32", "mixed", boundary(input.frames, speed[1], speed[0]), pitch, name, directory);
        }
        for (int length : {31, 1003}) {
            Buffer tiny(length);
            tiny[0][length/3] = .8f; tiny[1][length/3] = -.2f;
            std::string input_file = "short-" + std::to_string(length) + "-input.f32";
            save(tiny, directory + "/" + input_file);
            for (const auto &speed : speeds) for (int pitch : {-7, 0, 7}) {
                std::cout << ',';
                std::string name = "short-" + std::to_string(length) + "-" + std::to_string(speed[0]) + "-" + std::to_string(speed[1]) + "-pitch-" + std::to_string(pitch);
                qualify_case(tiny, input_file, "impulse", boundary(length, speed[1], speed[0]), pitch, name, directory);
            }
        }
        for (int length : {1, 2, 5759, 5760, 5761}) {
            Buffer tiny(length);
            tiny[0][length/3] = .8f; tiny[1][length/3] = -.2f;
            std::string name = "edge-" + std::to_string(length);
            std::string input_file = name + "-input.f32";
            save(tiny, directory + "/" + input_file);
            std::cout << ',';
            qualify_case(tiny, input_file, "impulse", length, 0, name, directory);
        }
        std::cout << "]}\n";
    } catch (const std::exception &error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
