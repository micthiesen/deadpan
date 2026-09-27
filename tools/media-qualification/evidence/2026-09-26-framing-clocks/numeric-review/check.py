from fractions import Fraction
from pathlib import Path
import hashlib, json, random, runpy, shutil

ROOT = Path('/Users/michael/Code/deadpan')
OUT = Path('/tmp/deadpan-splice-representation-20260926/numeric-review')
MASK = (1 << 64) - 1
LIMIT = 1 << 320
MAX = (1 << 127) - 1
Q = 1 << 32
rng = random.Random(0xF12A320)

def limbs(x):
    return [(x >> (64*i)) & MASK for i in range(5)]

def integer(x):
    return sum(v << (64*i) for i,v in enumerate(x))

def mul(x, value):
    out=[]
    carry=0
    for limb in limbs(x):
        product=limb*value+carry
        assert product < 1 << 128
        out.append(product & MASK)
        carry=product >> 64
    if carry:
        raise OverflowError
    return integer(out)

def mul_u128(x, value):
    low=limbs(mul(x, value & MASK))
    high=limbs(mul(x, value >> 64))
    if high[4]:
        raise OverflowError
    carry=False
    for i, addend in zip(range(1,5),high):
        first=low[i]+addend
        c1=first > MASK
        first &= MASK
        second=first+int(carry)
        c2=second > MASK
        low[i]=second & MASK
        carry=c1 or c2
    if carry:
        raise OverflowError
    return integer(low)

def checked_product(x,v):
    expected=x*v
    try:
        actual=mul_u128(x,v)
    except OverflowError:
        assert expected >= LIMIT, (x,v,'false overflow')
        return 'overflow'
    assert expected < LIMIT and actual == expected, (x,v,actual,expected)
    return 'success'

multiplication={'success':0,'overflow':0}
pairs=[]
for b in range(321):
    for c in (0,1,63,64,65,127,128):
        for x in set((0,(1<<b)-1,1<<max(0,b-1))):
            for v in set((0,(1<<c)-1,1<<max(0,c-1))):
                if x < LIMIT and v < 1<<128:
                    pairs.append((x,v))
for _ in range(40000):
    pairs.append((rng.getrandbits(rng.randrange(321)),rng.getrandbits(rng.randrange(129))))
for x,v in pairs:
    multiplication[checked_product(x,v)] += 1

max_product_bits=0
max_division_bits=0

def fraction(n,d):
    global max_division_bits
    assert 0 <= n <= d and d > 0
    if n==d:
        return Q
    result=0
    for _ in range(32):
        n=mul(n,2)
        max_division_bits=max(max_division_bits,n.bit_length())
        result*=2
        if n>=d:
            n-=d
            result+=1
    complement=d-n
    if n>complement or n==complement and result%2:
        result+=1
    return result

def segment(local,duration,start,end):
    global max_product_bits
    n,d=local.numerator,local.denominator
    u,v=duration.numerator,duration.denominator
    a,A=start.numerator,start.denominator
    b,B=end.numerator,end.denominator
    ud=mul_u128(u,d)
    left=mul(mul_u128(n,v),A)
    right=mul(ud,a)
    assert left >= right
    numerator=mul(left-right,B)
    denominator=mul(ud,b*A-a*B)
    max_product_bits=max(max_product_bits,left.bit_length(),right.bit_length(),numerator.bit_length(),denominator.bit_length())
    for p in (start,end):
        l=mul(mul_u128(n,v),p.denominator)
        r=mul(mul_u128(u,d),p.numerator)
        exact=(local/duration > p)-(local/duration < p)
        assert (l>r)-(l<r) == exact
    actual=fraction(numerator,denominator)
    oracle=round((local/duration-start)/(end-start)*Q)
    assert actual==oracle, (local,duration,start,end,actual,oracle)
    return actual

def containing(p):
    A=rng.randrange(1,1000001)
    B=rng.randrange(1,1000001)
    start=Fraction((p*A).__floor__(),A)
    end=min(Fraction((p*B).__floor__()+1,B),Fraction(1))
    if start==end:
        start=Fraction(A-1,A)
    return start,end

rational_cases=0
for _ in range(6000):
    local=Fraction(rng.randrange(1,MAX+1),rng.randrange(1,MAX+1))
    duration=Fraction(rng.randrange(1,MAX+1),rng.randrange(1,MAX+1))
    if local>duration:
        local,duration=duration,local
    start,end=containing(local/duration)
    segment(local,duration,start,end)
    rational_cases+=1

# Near-maximal 294-bit products with wide, independently reduced endpoints.
for offset in range(1,65):
    local=Fraction(MAX-offset,MAX-offset-1)
    duration=Fraction(MAX-offset-2,(MAX-offset-3)//2)
    start=Fraction(1,999983)
    end=Fraction(999999,1000000)
    assert start < local/duration < end
    segment(local,duration,start,end)
    rational_cases+=1

integer_cases=0
for _ in range(6000):
    D=rng.randrange(1,(1<<63))
    d=rng.randrange(1,1<<rng.randrange(1,128))
    local=Fraction(rng.randrange(min(MAX,D*d)+1),d)
    start,end=containing(local/D)
    new=segment(local,Fraction(D),start,end)
    n,d=local.numerator,local.denominator
    a,A=start.numerator,start.denominator
    b,B=end.numerator,end.denominator
    old_n=(n*A-D*d*a)*B
    old_d=D*d*(b*A-a*B)
    assert max(old_n.bit_length(),old_d.bit_length())<=231
    old=round(Fraction(old_n,old_d)*Q)
    assert new==old
    assert (local > end*D)-(local < end*D) == (local/D > end)-(local/D < end)
    assert (local == start*D) == (local/D == start)
    integer_cases+=1

producer=ROOT/'crates/deadpan-core/tests/fixtures/produce-framing-clock-oracles.py'
shutil.copyfile(producer,OUT/producer.name)
runpy.run_path(str(OUT/producer.name))
fixture_path=ROOT/'crates/deadpan-core/tests/fixtures/framing-clock-oracles.json'
regenerated=(OUT/fixture_path.name).read_bytes()
assert regenerated==fixture_path.read_bytes()
fixture=json.loads(regenerated)
def ratio(obj):
    return Fraction(int(obj['numerator']),int(obj['denominator']))
for case in fixture['cases']:
    assert segment(*(ratio(case[key]) for key in ('local','duration','start','end')))==case['progress']

report={
  'limb_mul_u128_cases':len(pairs),
  'limb_mul_u128_outcomes':multiplication,
  'rational_segment_cases':rational_cases,
  'integer_frozen_formula_comparisons':integer_cases,
  'fixture_cases':len(fixture['cases']),
  'fixture_reproduced_byte_for_byte':True,
  'fixture_sha256':hashlib.sha256(regenerated).hexdigest(),
  'quotient_overflow_cases':fixture['quotient_overflow_cases'],
  'endpoint_overflow_cases':fixture['endpoint_overflow_cases'],
  'observed_max_product_bits':max_product_bits,
  'observed_max_division_shift_bits':max_division_bits,
  'source_sha256': {str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [ROOT/'crates/deadpan-core/src/framing.rs',ROOT/'crates/deadpan-core/src/framing/numeric.rs',producer,fixture_path]},
  'scope':'Independent Python model of limb operations and exact Fraction formulas; no Cargo or Rust execution.'
}
(OUT/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
