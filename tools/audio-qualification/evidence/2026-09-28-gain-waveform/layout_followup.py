"""Verify the visual-review alignment fix, preserving the complete backend gate."""
from pathlib import Path
import subprocess

check = Path(__file__).with_name('check.py')
for args in [('app-base', 'waveform09'), ('feature', 'waveform10'), ('visual', 'waveform11', 'gain', 'room-tone')]:
    result = subprocess.run(['python3', str(check), *args])
    if result.returncode:
        raise SystemExit(result.returncode)
