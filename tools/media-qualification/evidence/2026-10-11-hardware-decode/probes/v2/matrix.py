from pathlib import Path
import hashlib,json,subprocess,time
R=Path(__file__).parent
F=Path('/Users/michael/Code/deadpan/native/deadpan-source/tests/fixtures')
names=['vfr.mp4','vfr-long-terminal.mp4','rotated90.mp4','aperture.mp4','h264-sdr-10-limited.mp4','h264-sdr-10-full.mp4','hevc-sdr-8-full.mp4','hevc-sdr-10-limited.mp4','hevc-pq-open-gop.mp4','hdr-pq-av.mp4','hdr-hlg-av.mp4','fields-tff.mp4','fields-bff.mp4','telecine-tff.mp4','vp9-sdr-8-limited.mp4','vp9-sdr-8-full.mp4','vp9-sdr-10-limited.mp4','vp9-altref.mp4','vp9-existing-8.mp4','vp9-existing-10.mp4','vp9-sdr-8-limited.webm','vp9-vfr.webm','vp9-pq-static.mp4','vp9-pq-static.webm']
results=[]
for name in names:
    if not (F/name).is_file():
        results.append({'fixture':name,'missing':True});continue
    cases={}
    for mode in ['soft','vt']:
        start=time.monotonic()
        try:p=subprocess.run([str(R/'probe'),mode,str(F/name)],capture_output=True,timeout=45)
        except subprocess.TimeoutExpired as e:
            (R/(name+'.'+mode+'.timeout.json')).write_text(json.dumps({'timeout_seconds':45})+'\n');raise
        (R/(name+'.'+mode+'.jsonl')).write_bytes(p.stdout);(R/(name+'.'+mode+'.stderr')).write_bytes(p.stderr)
        cases[mode]={'exit':p.returncode,'seconds':time.monotonic()-start,'frames':[json.loads(line) for line in p.stdout.splitlines()]}
    a,b=cases['soft']['frames'],cases['vt']['frames'];keys=['pts','duration','size','range','matrix','chroma','normalized','sha256']
    mismatches=[{'ordinal':i,'fields':[k for k in keys if x[k]!=y[k]]} for i,(x,y) in enumerate(zip(a,b)) if any(x[k]!=y[k] for k in keys)]
    row={'fixture':name,'sha256':hashlib.sha256((F/name).read_bytes()).hexdigest(),'equal':len(a)==len(b) and not mismatches and cases['soft']['exit']==cases['vt']['exit']==0,'software_exit':cases['soft']['exit'],'vt_exit':cases['vt']['exit'],'software_frames':len(a),'vt_frames':len(b),'mismatches':mismatches}
    results.append(row);print(json.dumps(row),flush=True)
(R/'matrix-results.json').write_text(json.dumps(results,indent=2)+'\n')
