from pathlib import Path
import hashlib
import json
import math
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
report = json.loads((root/'response.log').read_text())
assert not report['editor_integrated']
assert report['taps'] == 4097 and report['compensated_delay_frames'] == 0
cases = {case['name']:case for case in report['cases']}
def at(name,hz): return next(point['amplitude'] for point in cases[name]['response'] if point['hz'] == hz)
for case in cases.values():
    assert case['maximum_coefficient'] <= 16 and len(case['kernel_blake3']) == 64
    for point in case['response']:
        assert all(math.isfinite(point[key]) for key in ['hz','real','imaginary','amplitude'])
        assert abs(point['imaginary']) < 1e-11
for point in cases['flat']['response']: assert point['amplitude'] == 1
for hz,db in [(100,6),(1000,-6),(12000,3)]:
    assert abs(at('three_bands',hz)/10**(db/20)-1) < 3e-5
assert at('speech_band',100) < 1e-4 and at('speech_band',12000) < 1e-4
assert abs(at('speech_band',1000)-1) < 1e-4
assert abs(at('lowest_high_cut',80)-0.5) < 0.001
assert abs(at('highest_low_cut',20000)-0.5) < 0.001
assert abs(at('narrow_passband',1040)-1) < 0.002
assert at('narrow_passband',920) < 1e-4 and at('narrow_passband',1160) < 1e-4
summary = {name:{str(hz):at(name,hz) for hz in frequencies} for name,frequencies in {
    'three_bands':[100,1000,12000], 'speech_band':[100,1000,12000],
    'lowest_high_cut':[0,80,170], 'highest_low_cut':[0,19920,20000,20080],
    'narrow_passband':[920,1000,1040,1080,1160],
}.items()}
(root/'response-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
binary = repo/'target/release/examples/equalizer_probe'
files = subprocess.check_output(['git','diff','--cached','--name-only','--','crates'],cwd=repo,text=True).splitlines()
files += ['Cargo.lock','rust-toolchain.toml','native/deadpan-dsp/src/finite_filter.cpp',
          'native/deadpan-dsp/src/finite_filter.rs','native/deadpan-dsp/vendor/pins.json']
identity = dict(base_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
    binary=str(binary.relative_to(repo)),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
    source_sha256={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in files},
    os=subprocess.check_output(['sw_vers'],text=True),rust=subprocess.check_output(['rustc','--version'],text=True),
    hardware=subprocess.check_output(['sysctl','hw.model','machdep.cpu.brand_string','hw.physicalcpu','hw.memsize'],text=True))
(root/'identity.json').write_text(json.dumps(identity,indent=2)+'\n')
print(json.dumps(summary,indent=2))
