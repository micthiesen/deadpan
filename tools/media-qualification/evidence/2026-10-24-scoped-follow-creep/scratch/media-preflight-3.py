"""Read-only audit of media.py fixture guards against already-retained evidence.

No CLI/native/render processes. Parses the runner and executes only its pure
helpers, read-only SQLite history observation, and the dump/report guard block
before run('validate'). The dump invocation is replaced by retained dump bytes.
This is fixture-oracle preflight, not media qualification.
"""
import ast
import json
from pathlib import Path
from types import SimpleNamespace
import sqlite3
import copy
from fractions import Fraction
import hashlib

root = Path('/tmp/deadpan-scoped-follow-creep-2026-10-24')
source = (root / 'media.py').read_text()
tree = ast.parse(source)
defs = {'sha', 'canonical', 'ratio', 'round_even', 'quantized', 'scale_at', 'last_check', 'history'}
functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in defs]
receipt = json.loads((root / 'media-second.json').read_text())
command = receipt['command']
project = Path(command[command.index('--project') + 1])
report = json.loads((root / 'native-scoped-first/report.json').read_text())
scenario = next(s for s in report['scenarios'] if s['name'] == 'scoped-follow-creep')
assert len(scenario['checks']) == 155 and all(c['passed'] for c in scenario['checks'])
output = root / 'media-preflight-3-output'
output.mkdir()
namespace = dict(globals(), Q=1 << 32, args=SimpleNamespace(project=project, output=output),
                 checks=scenario['checks'])
exec(compile(ast.Module(body=functions, type_ignores=[]), '<runner-pure-helpers>', 'exec'), namespace)
block = []
inside = False
for node in tree.body:
    if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'document'
                                            for t in node.targets):
        inside = True
        node = ast.parse("document = json.loads((root / 'media-second/dump.stdout').read_text())").body[0]
    if inside and isinstance(node, ast.Expr) and isinstance(node.value, ast.Call):
        call = node.value
        if isinstance(call.func, ast.Name) and call.func.id == 'run':
            assert call.args[0].value == 'validate'
            break
    if inside:
        block.append(node)
assert block
# The retained dump replacement and subsequent guard block contain no command
# invocations. Nested calls are checked too; a future run() addition fails here.
assert not any(isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == 'run'
               for statement in block for n in ast.walk(statement))
exec(compile(ast.Module(body=block, type_ignores=[]), '<retained-dump-guards>', 'exec'), namespace)
print(json.dumps({
    'scope': 'read-only retained dump/native-report guard preflight; no CLI/native/render execution',
    'passed': True, 'native_checks_passed': 155,
    'oracle_rows_checked': len(namespace['oracle']),
    'clone_aliases_checked': len(namespace['aliases']),
    'revision': namespace['revision'],
    'history_observed': namespace['before_history'],
    'driver_sha256': hashlib.sha256(source.encode()).hexdigest(),
}, indent=2))
