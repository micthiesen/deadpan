from pathlib import Path
import os, subprocess, json, hashlib
repo = Path('/Users/michael/Code/deadpan')
root = Path(__file__).parent
prefix = '/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
env = dict(os.environ, PATH=prefix+'/bin:'+os.environ['PATH'])
source = repo/'native/deadpan-source/tests/fixtures/limited709.mkv'
out = root/'missing-range.mkv'
cmd = ['ffmpeg', '-nostdin', '-y', '-i', str(source), '-map', '0:v:0', '-c:v', 'copy', '-color_range', 'unknown', '-fflags', '+bitexact', str(out)]
result = subprocess.run(cmd, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
(root/'missing-range-generate.log').write_text(result.stdout)
result.check_returncode()
probe = subprocess.run(['ffprobe', '-v', 'error', '-show_streams', '-show_frames', '-of', 'json', str(out)], env=env, text=True, capture_output=True, check=True)
(root/'missing-range-probe.json').write_text(probe.stdout)
print(json.dumps(dict(command=cmd, sha256=hashlib.sha256(out.read_bytes()).hexdigest(), bytes=out.stat().st_size, probe=json.loads(probe.stdout)), indent=2))
