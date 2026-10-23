from pathlib import Path
import hashlib
import json
import re

S = Path('/tmp/deadpan-current-pause-route-profile-2026-10-23')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def derive(raw):
    result = {}
    for name in ['hardware', 'os', 'machine', 'power', 'thermal']:
        row = raw[name]
        result[name] = {'command': row['command'], 'exit': row['exit'],
                        'stdout': re.sub(r'\bid=\d+\b', 'id=redacted', row['stdout'])}
    result['redactions'] = [
        'Hardware stdout already whitelisted to model, chip, cores and memory in the pinned input.',
        'All command stderr omitted from this public derivative; raw observations remain private.',
        'Numeric battery id replaced with the literal id=redacted.',
        'Machine observation uses uname -msr, which omits hostname.',
    ]
    return result


if __name__ == '__main__':
    source = S / 'host-observations.json'
    pins = json.loads((S / 'driver-pins.json').read_text())
    assert sha(source) == pins['host-observations.json']
    raw = json.loads(source.read_text())
    result = derive(raw)
    assert all(row['exit'] == 0 for name, row in result.items() if name != 'redactions')
    output = S / 'host-observations-sanitized.json'
    output.write_text(json.dumps(result, indent=2) + '\n')
    receipt = {'source': source.name, 'source_sha256': sha(source),
               'output': output.name, 'output_sha256': sha(output),
               'producer': 'sanitize_host.py', 'producer_sha256': sha(Path(__file__)),
               'method': 'derive() whitelist/removal/redaction; pinned source unchanged'}
    (S / 'sanitization-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))
