from pathlib import Path
import hashlib,json,shutil,subprocess
r=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
binary=r/'compare_ffv1_threads'
shutil.copy2(repo/'target/release/examples/compare_ffv1_threads',binary)
sources=['native/deadpan-source/src/decoder.c','native/deadpan-source/src/decoder.h','native/deadpan-source/src/lib.rs','native/deadpan-source/tests/threaded_seek.rs','crates/deadpan-media/examples/compare_ffv1_threads.rs']
pins=dict(binary=str(binary),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),source={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in sources})
(r/'compare-pins.json').write_text(json.dumps(pins,indent=2)+'\n')
for name in sources:
 p=r/'compare-source'/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((repo/name).read_bytes())
for size,root in [('1080p',Path('/tmp/deadpan-generated-transitions-20261011')),('4k',Path('/tmp/deadpan-generated-loops-20261011/4k'))]:
 document=json.loads((root/'accepted.stdout').read_text())
 for i,(_,asset) in enumerate((pair for pair in document['assets'].items() if pair[0].endswith('-sampled'))):
  movie=root/'two-holds.deadpan/Media/Generated'/asset['content_hash'].replace(':','-')
  name=f'compare-{size}-{i}'
  subprocess.run(['python3',str(r/'run.py'),name,str(binary),str(movie),str(r/(name+'-report.json'))],check=True)
