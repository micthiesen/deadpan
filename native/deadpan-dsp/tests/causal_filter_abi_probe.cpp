// Raw ABI guards, every construction allocation failure, bounded native memory,
// allocation-free processing and measured tile time. Also run with ASan/UBSan.
#include "causal_filter.h"
#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <limits>
#include <new>
#include <vector>

namespace {
std::size_t live_allocations = 0, total_allocations = 0, live_bytes = 0, peak_bytes = 0;
std::size_t fail_at = std::numeric_limits<std::size_t>::max();
// The replacement allocator must honor ordinary operator new alignment, which
// exceeds alignof(max_align_t) on this arm64 toolchain (16 versus 8).
struct alignas(__STDCPP_DEFAULT_NEW_ALIGNMENT__) Header { std::size_t bytes; };
}
void *operator new(std::size_t size) {
    if (total_allocations++ == fail_at) throw std::bad_alloc();
    if (size > std::numeric_limits<std::size_t>::max() - sizeof(Header)) throw std::bad_alloc();
    auto *result = static_cast<Header *>(std::malloc(sizeof(Header) + size));
    if (!result) throw std::bad_alloc();
    result->bytes = size;
    ++live_allocations;
    live_bytes += size;
    peak_bytes = std::max(peak_bytes, live_bytes);
    return result + 1;
}
void *operator new[](std::size_t size) { return ::operator new(size); }
void operator delete(void *pointer) noexcept {
    if (pointer) {
        auto *allocation = static_cast<Header *>(pointer) - 1;
        --live_allocations;
        live_bytes -= allocation->bytes;
        std::free(allocation);
    }
}
void operator delete[](void *pointer) noexcept { ::operator delete(pointer); }
static void check(bool valid) { if (!valid) std::abort(); }

int main() {
    constexpr std::size_t output = 4096;
    std::size_t failure_sites = 0, processed = 0;
    std::printf("{\"cases\":[");
    bool first = true;
    for (std::size_t taps : {1, 4096, 4097, 8192, 16001, 48001, 192001, 384001}) {
        const auto history = taps - 1, input = history + output;
        std::vector<double> coefficients(taps), other(taps), left(input), right(input);
        coefficients[history] = -0.75;
        other[0] = 0.5;
        std::array<double, output + 2> a{}, b{};
        a.fill(123); b.fill(456);
        const auto untouched = [&]() {
            for (double value : a) check(value == 123);
            for (double value : b) check(value == 456);
        };
        void *filter = reinterpret_cast<void *>(1);
        check(dp_dsp_causal_create(coefficients.data(), other.data(), 0, &filter) == 1 && !filter);
        check(dp_dsp_causal_create(coefficients.data(), other.data(), 384002, &filter) == 1 && !filter);
        check(dp_dsp_causal_create(nullptr, other.data(), taps, &filter) == 1 && !filter);
        check(dp_dsp_causal_create(coefficients.data(), nullptr, taps, &filter) == 1 && !filter);
        check(dp_dsp_causal_create(coefficients.data(), other.data(), taps, nullptr) == 1);
        for (double invalid : {std::numeric_limits<double>::quiet_NaN(),
                               std::numeric_limits<double>::infinity(), 16.01, -16.01}) {
            coefficients[history] = invalid;
            check(dp_dsp_causal_create(coefficients.data(), other.data(), taps, &filter) == 1 && !filter);
            coefficients[history] = -0.75;
            other[0] = invalid;
            check(dp_dsp_causal_create(coefficients.data(), other.data(), taps, &filter) == 1 && !filter);
            other[0] = 0.5;
        }
        const auto baseline = live_allocations, bytes_baseline = live_bytes;
        peak_bytes = bytes_baseline;
        const auto before_create = total_allocations;
        check(dp_dsp_causal_create(coefficients.data(), other.data(), taps, &filter) == 0 && filter);
        const auto create_allocations = total_allocations - before_create;
        const auto native_peak = peak_bytes - bytes_baseline;
        const auto native_resident = live_bytes - bytes_baseline;
        std::size_t fft = 4096;
        while (fft < input) fft *= 2;
        check(native_peak <= fft * 128 + 1048576);
        dp_dsp_causal_destroy(filter);
        filter = nullptr;
        check(live_allocations == baseline && live_bytes == bytes_baseline);
        for (std::size_t failure = 0; failure < create_allocations; ++failure) {
            fail_at = total_allocations + failure;
            check(dp_dsp_causal_create(coefficients.data(), other.data(), taps, &filter) == 2 && !filter);
            fail_at = std::numeric_limits<std::size_t>::max();
            check(live_allocations == baseline && live_bytes == bytes_baseline);
            ++failure_sites;
        }
        check(dp_dsp_causal_create(coefficients.data(), other.data(), taps, &filter) == 0 && filter);
        const auto read = [&](void *handle, const double *l, const double *r,
                              std::uint32_t in, double *out_l, double *out_r,
                              std::uint32_t out) {
            return dp_dsp_causal_read(handle, l, r, in, out_l, out_r, out);
        };
        check(read(nullptr, left.data(), right.data(), input, a.data()+1, b.data()+1, output) == 1);
        check(read(filter, nullptr, right.data(), input, a.data()+1, b.data()+1, output) == 1);
        check(read(filter, left.data(), nullptr, input, a.data()+1, b.data()+1, output) == 1);
        check(read(filter, left.data(), right.data(), input, nullptr, b.data()+1, output) == 1);
        check(read(filter, left.data(), right.data(), input, a.data()+1, nullptr, output) == 1);
        check(read(filter, left.data(), right.data(), input-1, a.data()+1, b.data()+1, output) == 1);
        check(read(filter, left.data(), right.data(), input+1, a.data()+1, b.data()+1, output) == 1);
        check(read(filter, left.data(), right.data(), input, a.data()+1, b.data()+1, output-1) == 1);
        check(read(filter, left.data(), right.data(), input, a.data()+1, b.data()+1, output+1) == 1);
        untouched();
        for (double invalid : {std::numeric_limits<double>::quiet_NaN(),
                               std::numeric_limits<double>::infinity(), 1.01e100, -1.01e100}) {
            left.back() = invalid;
            check(read(filter, left.data(), right.data(), input, a.data()+1, b.data()+1, output) == 1);
            left.back() = 0;
            right.back() = invalid;
            check(read(filter, left.data(), right.data(), input, a.data()+1, b.data()+1, output) == 1);
            right.back() = 0;
            untouched();
        }
        left[0] = 2;
        right[input-1] = -4;
        const auto before_reads = total_allocations;
        std::array<double, 12> elapsed{};
        for (auto &milliseconds : elapsed) {
            const auto start = std::chrono::steady_clock::now();
            check(read(filter, left.data(), right.data(), input, a.data()+1, b.data()+1, output) == 0);
            milliseconds = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
            check(a.front() == 123 && a.back() == 123 && b.front() == 456 && b.back() == 456);
            for (std::size_t frame = 0; frame < output; ++frame) {
                check(std::abs(a[frame+1] - (frame == 0 ? -1.5 : 0.0)) < 1e-12);
                check(std::abs(b[frame+1] - (frame == output-1 ? -2.0 : 0.0)) < 1e-12);
            }
            ++processed;
        }
        check(total_allocations == before_reads);
        dp_dsp_causal_destroy(filter);
        dp_dsp_causal_destroy(nullptr);
        check(live_allocations == baseline && live_bytes == bytes_baseline);
        std::sort(elapsed.begin(), elapsed.end());
        std::printf("%s{\"taps\":%zu,\"native_peak_bytes\":%zu,\"native_resident_bytes\":%zu,"
                    "\"admission_bytes\":%zu,\"median_tile_ms\":%.6f,\"max_tile_ms\":%.6f}",
                    first ? "" : ",", taps, native_peak, native_resident, fft * 128 + 1048576,
                    (elapsed[5] + elapsed[6]) / 2, elapsed.back());
        first = false;
    }
    check(live_allocations == 0 && live_bytes == 0);
    std::printf("],\"creation_allocation_failure_sites\":%zu,\"processed_tiles\":%zu,"
                "\"processing_allocations\":0,\"leaked_allocations\":0,\"guards\":\"passed\"}\n",
                failure_sites, processed);
}
