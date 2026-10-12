import json,re,subprocess
from pathlib import Path
repo=Path('/Users/michael/Code/deadpan')
paths=subprocess.check_output(['git','diff','--name-only','--','crates'],cwd=repo,text=True).splitlines()
mechanical=[];substantive=[]
for n in paths:
 before=subprocess.check_output(['git','show','HEAD:'+n],cwd=repo,text=True)
 after=(repo/n).read_text()
 cleaned=re.sub(r'(?m)^\s*wet_sends: Default::default\(\),\n','',after)
 cleaned=cleaned.replace(' wet_sends: Default::default(),','')
 (mechanical if cleaned==before else substantive).append(n)
report=dict(mechanical_default_only=mechanical,substantive=substantive)
Path('/tmp/deadpan-event-returns-20261012/diff-review.json').write_text(json.dumps(report,indent=2)+'\n')
print('Default-only:',len(mechanical),'Substantive:',len(substantive))
print('\n'.join(substantive))
