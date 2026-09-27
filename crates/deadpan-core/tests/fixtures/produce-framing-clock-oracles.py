"""Development-only exact oracle; the Rust tests consume the checked-in JSON."""

from fractions import Fraction
import json
from pathlib import Path
import random

MAX = 2**127 - 1
Q = 2**32
random_source = random.Random(20260926)
cases = []


def add(local, duration, start, end):
    assert 0 <= start <= local / duration <= end <= 1
    assert start < end
    values = (local, duration, start, end)
    assert all(abs(v.numerator) <= MAX and v.denominator <= MAX for v in values)
    progress = (local / duration - start) / (end - start)
    # Fraction.__round__ specifies ties-to-even and uses unbounded integers.
    cases.append({
        **{name: {"numerator": str(value.numerator), "denominator": str(value.denominator)}
           for name, value in zip(("local", "duration", "start", "end"), values)},
        "progress": round(progress * Q),
    })


for duration in (Fraction(9, 2), Fraction(1, MAX), Fraction(MAX, MAX - 1), Fraction(MAX)):
    for position in (Fraction(0), Fraction(1, 4), Fraction(1, 2), Fraction(1)):
        local = position * duration
        if local.numerator <= MAX and local.denominator <= MAX:
            add(local, duration, Fraction(0), Fraction(1))

# Neither local/duration nor several duration*endpoint products fit ExactRatio.
for offset in range(1, 40):
    duration = Fraction(MAX - offset, MAX - offset - 2)
    local = Fraction(MAX - offset - 1, MAX - offset)
    add(local, duration, Fraction(999_999, 1_000_000), Fraction(1))
    duration = Fraction(MAX - offset, MAX - offset - 2)
    local = Fraction((MAX - offset) // 2, MAX - offset - 1)
    add(local, duration, Fraction(499_999, 1_000_000), Fraction(500_001, 1_000_000))

# Half-even progress ties and their two neighbors.
for lower in (0, 1, 17, Q // 2, Q - 2, Q - 1):
    duration = Fraction(7, 13)
    for epsilon in (-1, 0, 1):
        progress = Fraction((2 * lower + 1) * 3 + epsilon, 6 * Q)
        add(progress * duration, duration, Fraction(0), Fraction(1))

for _ in range(192):
    duration = Fraction(random_source.randrange(1, 10**18), random_source.randrange(1, 10**18))
    denominator = random_source.randrange(2, 1_000_001)
    a = random_source.randrange(denominator)
    b = random_source.randrange(a + 1, denominator + 1)
    start, end = Fraction(a, denominator), Fraction(b, denominator)
    progress = Fraction(random_source.randrange(1, 10**9), 10**9)
    local = duration * (start + (end - start) * progress)
    add(local, duration, start, end)

destination = Path(__file__).with_name("framing-clock-oracles.json")
def read_ratio(value):
    return Fraction(int(value["numerator"]), int(value["denominator"]))


def exceeds_ratio(value):
    return abs(value.numerator) > MAX or value.denominator > MAX


destination.write_text(json.dumps({
    "generator": "Python standard-library fractions.Fraction; seed20260926; exact half-even rounding",
    "quotient_overflow_cases": sum(exceeds_ratio(read_ratio(c["local"]) / read_ratio(c["duration"])) for c in cases),
    "endpoint_overflow_cases": sum(exceeds_ratio(read_ratio(c["duration"]) * read_ratio(c["start"])) for c in cases),
    "cases": cases,
}, indent=2) + "\n")
