from pathlib import Path
import sqlite3, subprocess
root=Path('/Users/michael/Code/deadpan')
scratch=Path('/tmp/deadpan-binding-storage-20260923')
package=scratch/'schema21.deadpan';package.mkdir(exist_ok=True);(package/'Snapshots').mkdir(exist_ok=True)
connection=sqlite3.connect(package/'project.sqlite')
connection.executescript((root/'crates/deadpan-store/tests/fixtures/v20-history.sql').read_text());connection.close()
cli=str(scratch/'deadpan-cli-schema21')
for args in [['doctor'],['project','migrate',str(package)],['project','validate',str(package)]]:
 p=subprocess.run([cli,*args],text=True,capture_output=True,check=True)
 print(p.stdout)
connection=sqlite3.connect(package/'project.sqlite')
version=connection.execute('pragma user_version').fetchone()[0];assert version==21
schemas=connection.execute("select distinct json_extract(document,'$.schema_version') from revisions").fetchall();assert schemas==[(15,)]
header='''-- Authentic database schema 21 / core schema 15 history.
-- Migrated from the schema-20 fixture using the preserved CLI from 983de6c.
-- CLI SHA-256: 9478d4e076d6442560d049c74b21e5e4342918f2e01b51b73ad0a7ec928b9ade.
-- Captured on 2026-09-23 after old-executable migration and validation.
-- Retains direct Split, occurrence copies, lineage, branches, undo and pending redo.
-- No external media or local paths.
PRAGMA application_id=1146113585;
PRAGMA user_version=21;
'''
(root/'crates/deadpan-store/tests/fixtures/v21-history.sql').write_text(header+'\n'.join(connection.iterdump())+'\n')
connection.close()
