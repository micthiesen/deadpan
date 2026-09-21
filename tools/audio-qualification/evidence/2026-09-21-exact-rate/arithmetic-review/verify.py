from pathlib import Path
import subprocess
from fractions import Fraction

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-exact-rate-arithmetic-review')
scratch.mkdir(exist_ok=True)
source = scratch / 'boundary.cpp'
source.write_text(r'''
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
''')
compile_command = [
    'clang++', '-std=c++17', '-O1', '-g', '-fsanitize=address,undefined',
    '-fno-sanitize-recover=all', '-Wall', '-Wextra', '-Werror', str(source),
    '-I' + str(repo / 'native/deadpan-dsp/src'),
    '-I' + str(repo / 'native/deadpan-dsp/vendor/signalsmith-stretch/include'),
    '-I' + str(repo / 'native/deadpan-dsp/vendor/signalsmith-linear/include'),
    '-o', str(scratch / 'boundary'),
]
r = subprocess.run(compile_command, capture_output=True, text=True)
if r.returncode:
    print(r.stderr)
    raise SystemExit(r.returncode)
center = 257 << 54
denominator = 1 << 63
maximum = (1 << 64) - 1
rates = [
    (1, 8), (8, 1), (1, 1), (2, 3), (200, 300), (257, 512), (259, 512),
    (center - 1, denominator), (center, denominator), (center + 1, denominator),
    (maximum, maximum), (maximum - 1, maximum), (maximum, maximum - 1),
]
outputs = [
    -52000, -768, -512, -256, -3, -2, -1, 0, 1, 2, 3, 256, 512, 768,
    52000, (1 << 23), (1 << 48), (1 << 48) + 255,
]
cases = [(n, d, x) for n, d in rates for x in outputs]
payload = ''.join(f'{n} {d} {x}\n' for n, d, x in cases)
r = subprocess.run([str(scratch / 'boundary')], input=payload, capture_output=True, text=True)
(scratch / 'stderr.txt').write_text(r.stderr)
if r.returncode:
    print(r.stderr)
    raise SystemExit(r.returncode)
actual = list(map(int, r.stdout.splitlines()))
expected = [round(Fraction(x * n, d)) for n, d, x in cases]
assert len(actual) == len(expected)
for case, a, e in zip(cases, actual, expected):
    assert a == e, (case, a, e)
summary = (
    f'{len(cases)} native boundaries equal independent Fraction nearest-even oracle; '
    'ASan/UBSan clean. Includes signed ties, u64-scale normalized ratios and '
    '2^48+255 output coordinate.\n'
)
(scratch / 'result.txt').write_text(summary)
print(summary)
