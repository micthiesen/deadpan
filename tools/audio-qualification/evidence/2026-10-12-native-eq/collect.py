from pathlib import Path
import gzip, hashlib, json, platform, shutil, subprocess

scratch = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
dest = repo/'tools/audio-qualification/evidence/2026-10-12-native-eq'
dest.mkdir(parents=True,exist_ok=True)
def packed(source, name):
    (dest/name).write_bytes(gzip.compress(source.read_bytes(),mtime=0))
for source in scratch.glob('*.json'):
    shutil.copy2(source,dest/source.name)
for source in scratch.glob('*.log'):
    packed(source,source.name+'.gz')
for name in ['run.py','checks.py','replays.py','freeze.py','collect.py']:
    shutil.copy2(scratch/name,dest/name)
for name in ['app-tests','registry-update','registry-update2','registry-update3']:
    source = Path('/tmp/deadpan-native-eq-'+name+'.log')
    packed(source,name+'.log.gz')
for name in ['eq-first','gain','equalizer','gain-final','audio-treatments','equalizer-performance']:
    packed(scratch/name/'report.json',name+'-report.json.gz')
for image, label in [('equalizer-060.png','eq-default.png'),('equalizer-079.png','eq-minimum.png'),('equalizer-112.png','eq-saved-minimum.png')]:
    shutil.copy2(scratch/'equalizer'/image,dest/label)
def command(args):
    return subprocess.check_output(args,cwd=repo,text=True).strip()
environment = dict(os=command(['sw_vers']),machine=platform.machine(),hardware=command(['sysctl','-n','machdep.cpu.brand_string']),physical_cores=command(['sysctl','-n','hw.physicalcpu']),memory_bytes=command(['sysctl','-n','hw.memsize']),rust=command(['rustc','--version']),base_commit=command(['git','rev-parse','HEAD']),ffmpeg_prefix='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
(dest/'environment.json').write_text(json.dumps(environment,indent=2)+'\n')
