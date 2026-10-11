from pathlib import Path
from datetime import datetime,timezone
import hashlib,json,subprocess
r=Path(__file__).parent
final=r/'final'
prefix=Path('/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
(final/'dependencies.txt').write_bytes(subprocess.check_output(['otool','-L',str(final/'compare_decode')]))
(final/'avcodec-dependencies.txt').write_bytes(subprocess.check_output(['otool','-L',str(prefix/'lib/libavcodec.dylib')]))
pins={str(p.resolve()):hashlib.sha256(p.read_bytes()).hexdigest() for name in ['avcodec','avformat','avutil','swscale','avfilter','swresample','avdevice'] for p in [prefix/'lib'/('lib'+name+'.dylib')]}
(final/'library-pins.json').write_text(json.dumps(pins,indent=2)+'\n')
identity=json.loads((final/'identity.json').read_text())
rows=['| Source | Software seek p95 | VideoToolbox seek p95 |', '| --- | ---: | ---: |']
for name,label in [('1080p60','H.264 1080p60'),('4k30','H.264 4K30'),('hevc-4k','HEVC 4K30'),('vp9-4k','VP9 4K30')]:
 d=json.loads((final/(name+'.json')).read_text());a,b=d['cases'];rows.append(f"| {label} | {a['seek_ms']['p95']:.3f} ms | {b['seek_ms']['p95']:.3f} ms |")
section='\n'.join(rows)+'''

All **5,760 sequential picture comparisons and 800 random seeks** pass exact
pixels and PTS/duration. Each case began below load 4, with no concurrent build
or heavy qualification task. The same-file software comparison runs before
VideoToolbox, after a complete reference scan, so input pages are warm.

Final release comparison binary SHA-256:
`'''+identity['binary_sha256']+'''`.
Source identity is base `9479499f92e654b8ab0cc40e030a4894b84d32b5` plus the
nine final source/test hashes in `final/identity.json`; those files are committed
with this record. This is Apple M5 Max, 128 GiB, macOS 26.5.2 and Rust 1.97.1.

The 339-test source/media regression run passes, followed by the final focused
admission and five native hardware tests. All 182 native sanitizer tests pass;
strict workspace/all-target Clippy and rustfmt checks pass. A late test edit
initially failed rustfmt-only checking; it was formatted and the final check
passes. No behavior or assertion failure was suppressed.
'''
p=Path('docs/qualification/hardware-decode-2026-10-11.md');s=p.read_text();start=s.index('<!-- final-measurements -->');end=s.index('<!-- final-measurements-end -->')+len('<!-- final-measurements-end -->');p.write_text(s[:start]+section+s[end:])
p=Path('docs/progress.json');d=json.loads(p.read_text());d['updated_at']=datetime.now(timezone.utc).isoformat(timespec='seconds').replace('+00:00','Z')
d['focus']='Completing remaining renderer and playback requirements in DP-16; measured VideoToolbox decoding remains opt-in because long-GOP seeks are slower.'
f=next(f for f in d['features'] if f['id']=='render');f['note']='Exact timing is complete. Optional VideoToolbox decoding matches software pixels, but slower long-GOP seeks keep preview on software. Remaining renderer, effects and output qualification stay open.'
d['history'].append({'date':'2026-10-11','percent':89,'complete_sections':8,'revision':identity['head'],'retrospective':False,'summary':'Implemented optional persistent VideoToolbox decoding and exact bounded surface transfer. All 339 source/media and 182 native sanitizer tests pass; 5,760 longer-clip picture comparisons and 800 random seeks match. Hardware seeks are slower, so normal preview remains on software and DP-16/DP-24 stay open.'})
p.write_text(json.dumps(d,indent=2)+'\n')
