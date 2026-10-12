import hashlib,json,pathlib,subprocess
s=pathlib.Path(__file__).resolve().parent
repo=pathlib.Path('/Users/michael/Code/deadpan')
binary=repo/'target/release/deadpan-cli'
fixture=json.loads((s/'real-media.json').read_text())
output=s/'new-release-render'
output.mkdir(exist_ok=True)
command=[str(binary),'render',fixture['package'],'--output',str(output),'--name','nested-eq.mp4','--expected',fixture['revision']]
with (s/'release-render.stdout').open('w') as stdout,(s/'release-render.stderr').open('w') as stderr:
 result=subprocess.run(command,stdout=stdout,stderr=stderr)
receipt=dict(command=command,exit=result.returncode,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
(s/'release-render.json').write_text(json.dumps(receipt,indent=2)+'\n')
assert result.returncode==0,(s/'release-render.stderr').read_text()
finished=json.loads((s/'release-render.stdout').read_text().splitlines()[-1])
assert finished['status']['outcome']=='published'
movie=finished['status']['receipt']['movie']
command=[str(binary),'verify-export',fixture['package'],'--movie',movie,'--revision',fixture['revision'],'--every','1']
with (s/'release-verify-report.json').open('w') as stdout,(s/'release-verify.stderr').open('w') as stderr:
 result=subprocess.run(command,stdout=stdout,stderr=stderr)
(s/'release-verify-command.json').write_text(json.dumps(dict(command=command,exit=result.returncode),indent=2)+'\n')
report=json.loads((s/'release-verify-report.json').read_text())
assert result.returncode==0 and report['passed'],report['summary']
assert report['summary']['pictures_checked']==120 and report['summary']['audio_windows_checked']==4
assert report['summary']['nonzero_offsets']==0 and report['summary']['offset_verified_windows']==1
print(json.dumps(dict(passed=True,summary=report['summary'])))
