from pathlib import Path

r = Path('/Users/michael/Code/deadpan')
changes = {
    'AGENTS.md': [
        ('Core schema 27 retains', 'Core schema 28 retains'),
        ('Database schemas 1 through 32 replay', 'Database schemas 1 through 33 replay'),
        ('Database schema 33 stores core schema 27', 'Database schema 34 stores core schema 28'),
        ('Database schema 33 retains content-keyed', 'Database schema 34 retains content-keyed'),
        ('Current schema 33 stores core 27.', 'Database 33 uses frozen core 27, retaining SpliceSource while rejecting new Retime edits. Current schema 34 stores core 28.'),
    ],
    'docs/ARCHITECTURE.md': [
        ('database schemas 1 through 32 directly to schema 33 and core document schema 27.', 'database schemas 1 through 33 directly to schema 34 and core document schema 28.'),
        ('SQLite schema 33, schema-1-through-32 migration', 'SQLite schema 34, schema-1-through-33 migration'),
    ],
    'README.md': [
        ('schema-1-through-32 migration to schema 33', 'schema-1-through-33 migration to schema 34'),
    ],
    'docs/HEADLESS.md': [
        ('Database schemas 1 through 32 return', 'Database schemas 1 through 33 return'),
        ('schema 33 and core document schema 27. Database-32 replays frozen core 26,', 'schema 34 and core document schema 28. Database-33 replays frozen core 27,\nwhich admits SpliceSource but refuses the new Retime edit commands.\nDatabase-32 replays frozen core 26,'),
    ],
}
for name, replacements in changes.items():
    path = r / name
    text = path.read_text()
    for before, after in replacements:
        assert text.count(before) == 1, (name, before, text.count(before))
        text = text.replace(before, after)
    path.write_text(text)
