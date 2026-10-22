from pathlib import Path
import hashlib,json,subprocess
scratch=Path('/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21')
def output(args):
    return subprocess.check_output(args,text=True).strip()
fixtures={}
for name in ['offset-bframes.mp4','cfr-bframes.mp4']:
    path=Path('native/deadpan-source/tests/fixtures')/name
    fixtures[str(path)]={'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
record={
    'base':'9b0c9e620f5a56af37ed5b7d7535acdae7e168a6',
    'previous_code_milestone':'d7b84aed4f943f5276a7e3592f641ba93e16d9a5',
    'hardware':output(['sysctl','-n','hw.model']),
    'cpu':output(['sysctl','-n','machdep.cpu.brand_string']),
    'memory_bytes':int(output(['sysctl','-n','hw.memsize'])),
    'os':output(['sw_vers']),
    'rustc':output(['rustc','--version']),
    'cargo':output(['cargo','--version']),
    'cargo_lock_sha256':hashlib.sha256(Path('Cargo.lock').read_bytes()).hexdigest(),
    'ffmpeg_prefix':'/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix',
    'profile_dev_opt_level':2,
    'physical_mac_required':True,
    'synthetic_worker_required':True,
    'fixtures':fixtures,
}
(scratch/'environment.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
