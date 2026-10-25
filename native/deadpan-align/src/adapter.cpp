#include "adapter.h"
#include <onnxruntime_cxx_api.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstring>
#include <memory>
#include <stdexcept>
#include <thread>
#include <vector>

namespace {
constexpr size_t kMaxFeatures = 3001;
constexpr size_t kLabels = 41;
void message(char *buffer, size_t capacity, const char *text) noexcept {
    if (!buffer || capacity == 0) return;
    const size_t length = std::min(capacity - 1, std::strlen(text));
    std::memcpy(buffer, text, length);
    buffer[length] = '\0';
}

// The callback's borrowed context remains live until this joined watcher ends.
// It cannot escape the FFI call, including on an exception from Run or copying.
struct RunWatch {
    std::atomic<bool> done{false};
    std::thread thread;
    RunWatch(Ort::RunOptions &options, DpAlignCancelled cancelled, const void *context)
        : thread([this, &options, cancelled, context] {
            while (!done.load(std::memory_order_relaxed)) {
                if (cancelled(context)) {
                    // This API is documented thread-safe during Run. Consume
                    // a possible status locally; no exception leaves a thread.
                    OrtStatus *status = Ort::GetApi().RunOptionsSetTerminate(options);
                    if (status) Ort::GetApi().ReleaseStatus(status);
                    return;
                }
                std::this_thread::sleep_for(std::chrono::milliseconds(2));
            }
        }) {}
    ~RunWatch() {
        done.store(true, std::memory_order_relaxed);
        if (thread.joinable()) thread.join();
    }
};
}

struct DpAlignSession {
    Ort::Env environment{ORT_LOGGING_LEVEL_FATAL, "deadpan-phoneme"};
    Ort::SessionOptions options;
    Ort::Session session{nullptr};
    DpAlignSession(const uint8_t *graph, size_t bytes) {
        if (std::strcmp(OrtGetApiBase()->GetVersionString(), "1.31.0") != 0)
            throw std::runtime_error("ONNX Runtime must be exactly 1.31.0");
        environment.DisableTelemetryEvents();
        options.SetIntraOpNumThreads(4);
        options.SetInterOpNumThreads(1);
        options.SetExecutionMode(ExecutionMode::ORT_SEQUENTIAL);
        options.SetGraphOptimizationLevel(GraphOptimizationLevel::ORT_ENABLE_EXTENDED);
        options.SetDeterministicCompute(true);
        options.DisableProfiling();
        options.AddConfigEntry("session.intra_op.allow_spinning", "0");
        options.AddConfigEntry("session.inter_op.allow_spinning", "0");
        // No provider is appended: ORT uses only its built-in CPU provider.
        // No custom-op library, external initializer, path or URL is accepted.
        session = Ort::Session(environment, graph, bytes, options);
        if (session.GetInputCount() != 2 || session.GetOutputCount() != 2)
            throw std::runtime_error("phoneme graph input/output count mismatch");
        Ort::AllocatorWithDefaultOptions allocator;
        const std::array<const char *, 2> inputs{"features", "feature_lengths"};
        const std::array<const char *, 2> outputs{"log_probs", "output_lengths"};
        for (size_t i = 0; i < 2; ++i) {
            auto input = session.GetInputNameAllocated(i, allocator);
            auto output = session.GetOutputNameAllocated(i, allocator);
            if (std::strcmp(input.get(), inputs[i]) || std::strcmp(output.get(), outputs[i]))
                throw std::runtime_error("phoneme graph input/output names mismatch");
        }
    }
};

extern "C" DpAlignSession *dp_align_create(const uint8_t *graph, size_t graph_len,
                                           char *error, size_t error_len) {
    try {
        if (!graph || graph_len != 19219161) throw std::runtime_error("invalid phoneme graph buffer");
        return new DpAlignSession(graph, graph_len);
    } catch (const std::exception &e) { message(error, error_len, e.what()); }
      catch (...) { message(error, error_len, "unknown native phoneme creation failure"); }
    return nullptr;
}

extern "C" void dp_align_destroy(DpAlignSession *session) { delete session; }

extern "C" int dp_align_run(DpAlignSession *session, const float *features, size_t frames,
                             float *output, size_t output_capacity, size_t *output_frames,
                             DpAlignCancelled cancelled, const void *context,
                             char *error, size_t error_len) {
    try {
        if (!session || !features || !output || !output_frames || !cancelled || !context
            || frames < 2 || frames > kMaxFeatures || output_capacity < ((frames + 1) / 2) * kLabels)
            throw std::runtime_error("invalid phoneme inference buffers");
        if (cancelled(context)) return 2;
        for (size_t i = 0; i < frames * 80; ++i)
            if (!std::isfinite(features[i])) throw std::runtime_error("nonfinite phoneme features");
        const std::array<int64_t, 3> shape{1, static_cast<int64_t>(frames), 80};
        const std::array<int64_t, 1> length_shape{1};
        int64_t length = static_cast<int64_t>(frames);
        auto memory = Ort::MemoryInfo::CreateCpu(OrtArenaAllocator, OrtMemTypeDefault);
        std::array<Ort::Value, 2> tensors{
            Ort::Value::CreateTensor<float>(memory, const_cast<float *>(features), frames * 80, shape.data(), shape.size()),
            Ort::Value::CreateTensor<int64_t>(memory, &length, 1, length_shape.data(), length_shape.size())};
        constexpr std::array<const char *, 2> input_names{"features", "feature_lengths"};
        constexpr std::array<const char *, 2> output_names{"log_probs", "output_lengths"};
        Ort::RunOptions run;
        RunWatch watch(run, cancelled, context);
        auto results = session->session.Run(run, input_names.data(), tensors.data(), tensors.size(), output_names.data(), output_names.size());
        if (cancelled(context)) return 2;
        if (results.size() != 2 || !results[0].IsTensor() || !results[1].IsTensor())
            throw std::runtime_error("phoneme output tensors missing");
        const auto probabilities = results[0].GetTensorTypeAndShapeInfo();
        const auto lengths = results[1].GetTensorTypeAndShapeInfo();
        const size_t expected = (frames + 1) / 2;
        if (probabilities.GetElementType() != ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT
            || probabilities.GetShape() != std::vector<int64_t>{static_cast<int64_t>(expected), 1, 41}
            || probabilities.GetElementCount() != expected * kLabels
            || lengths.GetElementType() != ONNX_TENSOR_ELEMENT_DATA_TYPE_FLOAT
            || lengths.GetShape() != std::vector<int64_t>{1} || lengths.GetElementCount() != 1)
            throw std::runtime_error("phoneme output dimensions or type mismatch");
        const float reported = *results[1].GetTensorData<float>();
        if (!std::isfinite(reported) || reported != static_cast<float>(expected))
            throw std::runtime_error("phoneme output length mismatch");
        const float *data = results[0].GetTensorData<float>();
        for (size_t i = 0; i < expected * kLabels; ++i)
            if (!std::isfinite(data[i])) throw std::runtime_error("nonfinite phoneme output");
        std::copy_n(data, expected * kLabels, output);
        *output_frames = expected;
        return 0;
    } catch (const std::exception &e) { message(error, error_len, e.what()); }
      catch (...) { message(error, error_len, "unknown native phoneme run failure"); }
    return 1;
}
