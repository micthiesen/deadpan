from pathlib import Path
import json
import socket
import subprocess
import threading

root = Path(__file__).parent
listener = socket.socket()
listener.bind(('127.0.0.1', 0))
listener.listen()
listener.settimeout(15)
received = []
def capture():
    connection, _ = listener.accept()
    with connection:
        connection.settimeout(5)
        data = b''
        while not data.endswith(b'\r\n\r\n') and len(data) < 8192:
            data += connection.recv(1024)
        received.append(data)
        connection.sendall(b'HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n')
thread = threading.Thread(target=capture)
thread.start()
metadata = json.loads((root / 'metadata.json').read_text())
metadata['formats'][0]['url'] = 'https://www.youtube.com/owned-fixture'
(root / 'connect-fixture/metadata.json').write_text(json.dumps(metadata))
yt = '/tmp/deadpan-youtube-limits-bundle-20261011/Deadpan.app/Contents/Resources/helpers/yt-dlp/2026.08.19/yt-dlp_macos'
command = [yt, '--ignore-config', '--no-plugin-dirs', '--no-cache-dir', '--retries', '0',
           '--proxy', f'http://deadpan:fixture@127.0.0.1:{listener.getsockname()[1]}',
           '--user-agent', 'OpenAI File Downloader, XaiImageApiFetch/1.0',
           '--load-info-json', str(root / 'connect-fixture/metadata.json'), '--format', '137',
           '--output', str(root / 'connect-fixture/video.mp4')]
with (root / 'connect-fixture/helper.log').open('x') as log:
    result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=20)
thread.join()
listener.close()
assert result.returncode != 0 and len(received) == 1
(root / 'connect-fixture/request.bin').write_bytes(received[0])
(root / 'connect-fixture/result.json').write_text(json.dumps(dict(command=command, exit=result.returncode), indent=2) + '\n')
print(repr(received[0]))
