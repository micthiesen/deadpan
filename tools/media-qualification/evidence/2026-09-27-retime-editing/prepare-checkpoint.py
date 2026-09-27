from pathlib import Path
s=Path('/tmp/deadpan-retime-20260927'); previous=Path('/tmp/deadpan-original-audition-20260927')
p=(previous/'summarize-tree.py').read_text().replace(str(previous),str(s))
(s/'summarize-tree.py').write_text(p)
p=(previous/'checkpoint.py').read_text()
p=p.replace(str(previous),str(s)).replace('gate-2/source-after.json','gate-1/source-after.json').replace('evidence/2026-09-27-original-audition','evidence/2026-09-27-retime-editing').replace('/tmp/deadpan-moment-paste-20260926/checkpoint',str(previous/'checkpoint'))
(s/'checkpoint.py').write_text(p)
