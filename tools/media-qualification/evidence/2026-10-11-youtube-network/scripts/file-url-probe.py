from pathlib import Path
import hashlib
import json
import subprocess

root = Path(__file__).parent
folder = root / 'file-url-probe'
folder.mkdir()
marker = folder / 'owned-private-marker.mp4'
marker.write_bytes(b'deadpan-owned-local-file-access-witness')
metadata = json.loads((root / 'metadata.json').read_text())
metadata['formats'][0]['url'] = marker.as_uri()
metadata['formats'][0]['filesize'] = marker.stat().st_size
(folder / 'metadata.json').write_text(json.dumps(metadata))
yt = Path('/tmp/deadpan-youtube-limits-bundle-20261011/Deadpan.app/Contents/Resources/helpers/yt-dlp/2026.08.19/yt-dlp_macos')
command = [str(yt), '--ignore-config', '--no-plugin-dirs', '--no-cache-dir', '--retries', '0',
           '--load-info-json', str(folder / 'metadata.json'), '--format', '137',
           '--output', str(folder / 'download.mp4')]
with (folder / 'helper.log').open('x') as log:
    result = subprocess.run(command, env={'HOME': str(root / 'home'), 'TMPDIR': str(root / 'tmp'), 'PATH': '/usr/bin:/bin'}, stdout=log, stderr=subprocess.STDOUT, timeout=20)
error = (folder / 'helper.log').read_text()
assert result.returncode != 0 and not (folder / 'download.mp4').exists(), error
assert 'file:// URLs are disabled by default' in error, error
(folder / 'result.json').write_text(json.dumps(dict(command=command, exit=result.returncode,
    helper_sha256=hashlib.sha256(yt.read_bytes()).hexdigest(), downloaded=False), indent=2) + '\n')
print('Pinned helper refused the owned file URL; no output was created')
