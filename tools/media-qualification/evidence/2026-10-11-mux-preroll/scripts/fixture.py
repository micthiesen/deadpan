from pathlib import Path
import hashlib
import json
import os
import subprocess

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
old = repo / 'tools/media-qualification/evidence/2026-10-08-mux-duration/movie.mp4'
current = root / 'capture-1/pq-640x360-hardware-targettwo.mp4'
combined = root / 'current-aac-reordered.mp4'
prefix = '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
env = dict(os.environ, PATH=prefix + '/bin:' + os.environ['PATH'])
def packets(path, stream):
    result = subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', stream, '-show_packets', '-show_data_hash', 'sha256', '-of', 'json', str(path)], env=env)
    return json.loads(result)['packets']
identities = {}
for name, source, stream in [('video', old, 'v:0'), ('audio', current, 'a:0')]:
    original = packets(source, stream)
    remuxed = packets(combined, stream)
    keep = lambda rows: [{key: row[key] for key in ['pts', 'dts', 'duration', 'size', 'data_hash']} for row in rows]
    assert keep(original) == keep(remuxed), name
    identities[name] = keep(original)
assert len(identities['video']) == 46 and len(identities['audio']) == 74
assert identities['audio'][0]['pts'] == -2048 and identities['audio'][1]['pts'] == -1024
data = bytearray(combined.read_bytes())
assert data[288:300] == bytes([0, 0, 0, 32]) + b'mdhd' + bytes(4)
assert data[312:316] == (46).to_bytes(4)
# FFmpeg 9 repaired the old muxer's bookkeeping while copying the streams.
# Reintroduce only its retained 47-tick header defect; packet bytes and all
# packet clocks stay identical to the two real encoder outputs above.
data[312:316] = (47).to_bytes(4)
evidence = repo / 'tools/media-qualification/evidence/2026-10-11-mux-preroll'
evidence.mkdir(exist_ok=True)
(evidence / 'reordered-current-aac.mp4').write_bytes(data)
record = dict(method='Stream-copy retained reordered HEVC and freshly encoded current AAC, then restore only the original 47-tick video mdhd defect.',
    sources={str(path):dict(bytes=path.stat().st_size, sha256=hashlib.sha256(path.read_bytes()).hexdigest()) for path in [old, current, combined]},
    fixture_sha256=hashlib.sha256(data).hexdigest(), fixture_bytes=len(data),
    ffmpeg=subprocess.check_output(['ffmpeg','-version'], text=True),
    ffprobe=subprocess.check_output(['ffprobe','-version'], text=True, env=env),
    packets=identities)
(root / 'fixture-identity.json').write_text(json.dumps(record, indent=2)+'\n')
print('Fixture retains all 46 video and 74 AAC packet bytes/clocks; only mdhd is changed:', record['fixture_sha256'])
