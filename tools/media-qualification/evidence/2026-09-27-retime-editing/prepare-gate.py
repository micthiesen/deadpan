from pathlib import Path
s=Path('/tmp/deadpan-retime-20260927')
previous=Path('/tmp/deadpan-original-audition-20260927')
(s/'gate.py').write_text((previous/'gate.py').read_text())
