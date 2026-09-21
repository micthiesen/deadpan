
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <stdexcept>
#include <iostream>
#include "signalsmith-stretch/signalsmith-stretch.h"
// Inspection-only harness: upstream and standard headers are already parsed.
#define private public
#include "canonical.hpp"
#undef private
struct Source {
    const float *operator[](int) const { return values; }
    float values[1] = {0.0f};
};
int main() {
    std::uint64_t numerator, denominator;
    std::int64_t output;
    Source source;
    while (std::cin >> numerator >> denominator >> output) {
        deadpan_audio_probe::CanonicalStretch<Source> renderer(source, 1, 1,
            deadpan_audio_probe::ExactRate{numerator, denominator}, 0);
        std::cout << renderer.boundary(output) << '\n';
    }
}
