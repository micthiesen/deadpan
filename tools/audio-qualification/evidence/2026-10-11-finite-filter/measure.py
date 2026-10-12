from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
binary = repo / 'target/release/examples/finite_filter_probe'
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
identity = digest(binary)
source = {}
for path in (repo / 'native/deadpan-dsp').rglob('*'):
    if path.is_file():
        source[str(path.relative_to(repo))] = digest(path)
source['Cargo.lock'] = digest(repo / 'Cargo.lock')
source['rust-toolchain.toml'] = digest(repo / 'rust-toolchain.toml')
record = dict(binary=str(binary.relative_to(repo)), binary_sha256=identity,
              source_base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
              source_sha256=source, runs=[])
for label, command in {
    'os':['sw_vers'],
    'hardware':['sysctl','hw.model','machdep.cpu.brand_string','hw.physicalcpu','hw.memsize'],
    'rust':['rustc','--version'],
    'cxx':['clang++','--version'],
}.items():
    record[label] = subprocess.check_output(command, text=True)
for index in range(1,4):
    name = f'final-measurement-{index}'
    subprocess.run([sys.executable,str(root/'run.py'),name,'time','-l',str(binary)],check=True)
    result = json.loads((root/(name+'.log')).read_text().splitlines()[0])
    assert result['maximum_direct_error'] < 2e-12 and result['tile_count'] == 5000
    record['runs'].append(dict(name=name,result=result))
assert identity == digest(binary)
(root/'measurements.json').write_text(json.dumps(record,indent=2)+'\n')
