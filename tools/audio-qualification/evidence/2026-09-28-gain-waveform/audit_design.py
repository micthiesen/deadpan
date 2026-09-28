"""Verify retained imagegen asset and exact-prompt identities."""
import hashlib
import json
from pathlib import Path
import struct

root = Path('docs/design')
manifest = json.loads((root / 'manifest.json').read_text())
for entry in manifest['images']:
    data = (root / entry['image']).read_bytes()
    assert hashlib.sha256(data).hexdigest() == entry['sha256'], entry['image']
    assert data[:8] == b'\x89PNG\r\n\x1a\n'
    assert struct.unpack('>II', data[16:24]) == (entry['width'], entry['height'])
    prompt = (root / entry['prompt']).read_bytes()
    assert hashlib.sha256(prompt).hexdigest() == entry['prompt_sha256'], entry['prompt']
    for reference in entry.get('referenced_images', []):
        assert (root / reference).is_file(), reference
print(json.dumps({'images_and_prompts': len(manifest['images']), 'passed': True}))
