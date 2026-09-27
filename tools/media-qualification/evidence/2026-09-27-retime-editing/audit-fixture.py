from pathlib import Path
import hashlib,json
r=Path('/Users/michael/Code/deadpan')
s=Path('/tmp/deadpan-retime-20260927')
base=r/'crates/deadpan-store/tests/fixtures'
record=json.loads((base/'v33-retime-history.provenance.json').read_text())
for field,filename in [('producer_script_sha256', record['producer_script']), ('producer_log_sha256',record['producer_log']), ('sql_sha256','v33-retime-history.sql'), ('input_fixture_sha256',record['input_fixture'])]:
    assert hashlib.sha256((base/filename).read_bytes()).hexdigest()==record[field], (field, filename)
assert hashlib.sha256((s/'old-deadpan-cli').read_bytes()).hexdigest()==record['binary_sha256']
assert record['reconstructed_sql_validated_by_old_binary']
(s/'fixture-audit.json').write_text(json.dumps(dict(verified=True, provenance=record),indent=2)+'\n')
print(json.dumps({'verified':True,'counts':record['counts'],'database_schema':record['database_schema'],'core_schema':record['core_schema']}))
