from pathlib import Path
import hashlib,json,subprocess,time
R=Path(__file__).parent
P=Path('/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
F=Path('/Users/michael/Code/deadpan/native/deadpan-source/tests/fixtures')
cmd=['cc','-O2','-Wall','-Wextra','-Werror','-I'+str(P/'include'),str(R/'probe.c'),'-L'+str(P/'lib'),'-Wl,-rpath,'+str(P/'lib'),'-lavformat','-lavcodec','-lavutil','-lswscale','-o',str(R/'probe')]
subprocess.run(cmd,check=True)
results=[]
for name in ['cfr-bframes.mp4','hevc-sdr-8-limited.mp4','hevc-sdr-10-full.mp4','vp9-sdr-10-full.mp4','aperture-fractional.mp4']:
    cases={}
    for mode in ['soft','vt']:
        start=time.monotonic();p=subprocess.run([str(R/'probe'),mode,str(F/name)],capture_output=True,timeout=45)
        (R/(name+'.'+mode+'.jsonl')).write_bytes(p.stdout);(R/(name+'.'+mode+'.stderr')).write_bytes(p.stderr)
        cases[mode]={'exit':p.returncode,'seconds':time.monotonic()-start,'frames':[json.loads(line) for line in p.stdout.splitlines()]}
    a,b=cases['soft']['frames'],cases['vt']['frames']
    equal=len(a)==len(b) and all(all(x[k]==y[k] for k in ['pts','duration','size','range','matrix','chroma','normalized','sha256']) for x,y in zip(a,b))
    row={'fixture':name,'source_sha256':hashlib.sha256((F/name).read_bytes()).hexdigest(),'equal':equal,'software_exit':cases['soft']['exit'],'hardware_exit':cases['vt']['exit'],'software_frames':len(a),'hardware_frames':len(b),'first_software':a[:1],'first_hardware':b[:1]}
    results.append(row);print(json.dumps(row),flush=True)
(R/'probe-results.json').write_text(json.dumps(results,indent=2)+'\n')
