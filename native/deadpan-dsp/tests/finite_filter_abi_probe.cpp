// Raw ABI contracts, output guards, allocation failure cleanup and allocation-
// free tile processing. Run with ASan/UBSan as well as the ordinary optimizer.
#include "finite_filter.h"
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <limits>
#include <new>

namespace {
std::size_t live_allocations = 0;
std::size_t total_allocations = 0;
std::size_t fail_at = std::numeric_limits<std::size_t>::max();
}

void *operator new(std::size_t size) {
    if (total_allocations++ == fail_at) throw std::bad_alloc();
    void *result = std::malloc(size ? size : 1);
    if (!result) throw std::bad_alloc();
    ++live_allocations;
    return result;
}
void *operator new[](std::size_t size) { return ::operator new(size); }
void operator delete(void *pointer) noexcept {
    if (pointer) { --live_allocations; std::free(pointer); }
}
void operator delete[](void *pointer) noexcept { ::operator delete(pointer); }

static void check(bool valid) { if (!valid) std::abort(); }

int main() {
    constexpr std::size_t taps = 4097, input = 5120, output = 1024;
    std::array<double, taps> coefficients{};
    coefficients[2048] = -0.75;
    std::array<double, input> left{}, right{};
    std::array<double, output + 2> a{}, b{};
    a.fill(123); b.fill(456);
    const auto untouched = [&]() {
        for (double value : a) check(value == 123);
        for (double value : b) check(value == 456);
    };
    void *filter = nullptr;
    check(dp_dsp_filter_create(coefficients.data(), taps - 1, &filter) == 1 && !filter);
    check(dp_dsp_filter_create(nullptr, taps, &filter) == 1 && !filter);
    check(dp_dsp_filter_create(coefficients.data(), taps, nullptr) == 1);
    for (double invalid : {std::numeric_limits<double>::quiet_NaN(),
                           std::numeric_limits<double>::infinity(), 16.01, -16.01}) {
        coefficients[0] = invalid;
        check(dp_dsp_filter_create(coefficients.data(), taps, &filter) == 1 && !filter);
    }
    coefficients[0] = 0;
    const auto baseline = live_allocations;
    const auto before_create = total_allocations;
    check(dp_dsp_filter_create(coefficients.data(), taps, &filter) == 0 && filter);
    const auto create_allocations = total_allocations - before_create;
    dp_dsp_filter_destroy(filter);
    filter = nullptr;
    check(live_allocations == baseline);
    for (std::size_t failure = 0; failure < create_allocations; ++failure) {
        fail_at = total_allocations + failure;
        check(dp_dsp_filter_create(coefficients.data(), taps, &filter) == 2 && !filter);
        fail_at = std::numeric_limits<std::size_t>::max();
        check(live_allocations == baseline);
    }
    check(dp_dsp_filter_create(coefficients.data(), taps, &filter) == 0 && filter);
    const auto read = [&](void *handle, const double *l, const double *r,
                          std::uint32_t in, double *out_l, double *out_r,
                          std::uint32_t out) {
        return dp_dsp_filter_read(handle, l, r, in, out_l, out_r, out);
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
    left[2048] = 2;
    right[2048+1023] = -4;
    const auto before_reads = total_allocations;
    for (int iteration = 0; iteration < 100; ++iteration) {
        check(read(filter, left.data(), right.data(), input, a.data()+1, b.data()+1, output) == 0);
        check(a.front() == 123 && a.back() == 123 && b.front() == 456 && b.back() == 456);
        for (std::size_t frame = 0; frame < output; ++frame) {
            check(std::abs(a[frame+1] - (frame == 0 ? -1.5 : 0.0)) < 1e-12);
            check(std::abs(b[frame+1] - (frame == 1023 ? 3.0 : 0.0)) < 1e-12);
        }
    }
    check(total_allocations == before_reads);
    dp_dsp_filter_destroy(filter);
    dp_dsp_filter_destroy(nullptr);
    check(live_allocations == baseline);
    std::printf("{\"creation_allocation_failure_sites\":%zu,\"processed_tiles\":100,"
                "\"processing_allocations\":0,\"leaked_allocations\":0,\"guards\":\"passed\"}\n",
                create_allocations);
}
