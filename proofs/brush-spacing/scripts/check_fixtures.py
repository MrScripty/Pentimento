"""Independent Fraction oracle plus exact-arithmetic models of two Rust bugs.
The legacy models describe inspected code; they do not execute Rust or f32.
"""
from fractions import Fraction as F


def step(h, length, state):
    count, a = state
    h, length, a = F(h), F(length), F(a)
    assert h > 0 and length >= 0 and 0 <= a < h
    q = (a + length) // h
    result = (count + q, a + length - q * h)
    assert 0 <= result[1] < h
    assert result[0] * h + result[1] == count * h + a + length
    return result


def run(h, lengths, state=(0, F(0))):
    for length in lengths:
        state = step(h, length, state)
    return state


fixtures = [
    ("sculpt_correct_phase", 10, [3, 3, 4], (0, 0), (1, 0)),
    ("sculpt_no_early_crossing", 10, [3, 3, 3], (0, 0), (0, 9)),
    ("paint_preserves_carry", 10, [3, 3, 4], (1, 0), (2, 0)),
    ("rational_split", F(5, 2), [F(1, 2), F(3, 4), F(7, 4)], (0, 0), (1, F(1, 2))),
    ("rational_unsplit", F(5, 2), [3], (0, 0), (1, F(1, 2))),
    ("multiple_crossings", 10, [37], (0, 4), (4, 1)),
    ("zero_segment", 10, [0], (7, 4), (7, 4)),
    ("exact_boundary_once", 10, [10, 0, 0], (0, 0), (1, 0)),
    ("no_forced_endpoint", 10, [9], (0, 0), (0, 9)),
    ("empty_stroke", 10, [], (0, 0), (0, 0)),
]
for name, h, lengths, initial, expected in fixtures:
    assert run(h, lengths, initial) == expected, name

# Bounded grid exercises chunking, arbitrary phase, zero segments, both policies.
cases = 0
for h in [F(1, 2), F(1), F(5, 2), F(10)]:
    for fraction in [F(0), F(1, 4), F(1, 2), F(3, 4)]:
        for x in [F(0), F(1, 3), F(1), F(3), F(10)]:
            for y in [F(0), F(1, 3), F(1), F(3), F(10)]:
                for n in [0, 1, 7]:
                    initial = (n, h * fraction)
                    assert run(h, [x, y], initial) == run(h, [x + y], initial)
                    cases += 1

# Sculpt old loop at h=10, x=0 -> 3 -> 6 -> 9:
# reads distance from last DAB (0 until first emission), not last input.
a, count, last_dab = F(0), 0, F(0)
for x in [F(3), F(6), F(9)]:
    a += abs(x - last_dab)
    while a >= 10:
        a -= 10
        last_dab += 10
        count += 1
assert (count, a) == (1, 8)  # Incorrect: a crossing before x reaches 10.
assert run(10, [3, 3, 3]) == (0, 9)

# Paint old no-dab loop at h=10, x=0 -> 3 -> 6 -> 10.
a, count = F(0), 1
for length in [F(3), F(3), F(4)]:
    a += length
    dab_start = max(F(0), F(10) - (a - length))
    current_distance = F(0)
    while dab_start <= length:
        count += 1
        current_distance = dab_start
        dab_start += 10
    a = length - current_distance
assert (count, a) == (1, 4)  # Incorrect: loses old residual on no-dab segments.
assert run(10, [3, 3, 4], (1, 0)) == (2, 0)
print(f"PASS: {len(fixtures)} fixtures, {cases} composition cases, 2 legacy divergences")
