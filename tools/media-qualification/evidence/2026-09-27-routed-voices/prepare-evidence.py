from pathlib import Path

previous = Path('/tmp/deadpan-source-voices-20260927')
current = Path('/tmp/deadpan-routed-voices-20260927')
for name in ['checkpoint.py', 'collect.py', 'summarize-tree.py']:
    text = (previous / name).read_text()
    text = text.replace(str(previous), str(current))
    text = text.replace('2026-09-27-source-voices', '2026-09-27-routed-voices')
    text = text.replace('/tmp/deadpan-sound-placement-20260927/checkpoint', str(previous / 'checkpoint'))
    text = text.replace(
        "'checkpoint.py', 'records.py', 'followups.py'",
        "'checkpoint.py', 'records.py', 'prepare-evidence.py'",
    )
    (current / name).write_text(text)
print('Prepared evidence/checkpoint scripts without changing Git state')
