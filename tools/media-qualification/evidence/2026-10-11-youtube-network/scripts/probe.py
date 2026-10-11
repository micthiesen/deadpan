from pathlib import Path
import http.server
import json
import os
import socket
import subprocess
import sys
import threading
import time

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
bundle = Path('/tmp/deadpan-youtube-limits-bundle-20261011/Deadpan.app')
yt = bundle / 'Contents/Resources/helpers/yt-dlp/2026.08.19/yt-dlp_macos'
for name in ['home', 'tmp', 'download']:
    (root / name).mkdir(mode=0o700, exist_ok=True)
requests = []
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        requests.append(self.path)
        body = b'owned-network-probe'
        self.send_response(200)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
port = server.server_address[1]
metadata = dict(id='Z4C82eyhwgU', title='owned probe', extractor='youtube', extractor_key='Youtube',
                webpage_url='https://www.youtube.com/watch?v=Z4C82eyhwgU', duration=1,
                formats=[dict(format_id='137', protocol='https', url=f'http://127.0.0.1:{port}/owned',
                              ext='mp4', vcodec='avc1.640028', acodec='none', width=320, height=180,
                              fps=24, filesize=19)])
(root / 'metadata.json').write_text(json.dumps(metadata))
env = {'HOME': str(root / 'home'), 'TMPDIR': str(root / 'tmp'), 'PATH': '/usr/bin:/bin'}
command = [str(yt), '--ignore-config', '--no-plugin-dirs', '--no-remote-components',
           '--no-cache-dir', '--no-cookies', '--no-cookies-from-browser', '--no-playlist',
           '--user-agent', 'OpenAI File Downloader, XaiImageApiFetch/1.0',
           '--load-info-json', str(root / 'metadata.json'), '--format', '137',
           '--output', str(root / 'download/test.mp4')]
rows = []
try:
    with (root / 'helper-2.log').open('x') as log:
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=20)
    rows.append(dict(name='pinned-helper-local-network', exit=result.returncode, requests=list(requests), command=command))
    for label, rule in [('blocked', '(version 1)(allow default)(deny network*)'),
                        ('allowed-port', f'(version 1)(allow default)(deny network*)(allow network-outbound (remote tcp "localhost:{port}"))')]:
        requests.clear()
        target = f'http://127.0.0.1:{port}/{label}'
        cmd = ['sandbox-exec', '-p', rule, 'curl', '--silent', '--show-error', '--max-time', '5',
               '--user-agent', 'OpenAI File Downloader, XaiImageApiFetch/1.0', target]
        with (root / (label + '.log')).open('x') as log:
            result = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, timeout=10)
        rows.append(dict(name=label, exit=result.returncode, requests=list(requests), command=cmd))
finally:
    server.shutdown()
    server.server_close()
    thread.join()
(root / 'results.json').write_text(json.dumps(rows, indent=2) + '\n')
print(json.dumps(rows, indent=2))
