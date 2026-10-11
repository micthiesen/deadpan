from pathlib import Path
import hashlib, json, subprocess, time

root = Path(__file__).parent
repo = Path('/Users/michael/Code/deadpan')
prefix = Path('/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
source = repo/'native/deadpan-media-worker/src/converter.c'
original = source.read_text()
start = original.index('static int proxy_write_packets(void) {')
end = original.index('\nint deadpan_proxy_open(', start)
instrumented = original[:start]+(root/'drain.c').read_text()+original[end:]+(root/'main.c').read_text()
(root/'instrumented.c').write_text(instrumented)
binary = root/'probe'
command = ['clang','-std=c11','-O2','-Wall','-Wextra','-Werror',str(root/'instrumented.c'),'-I'+str(source.parent),'-I'+str(prefix/'include'),'-L'+str(prefix/'lib'),'-Wl,-rpath,'+str(prefix/'lib'),'-lavcodec','-lavformat','-lavutil','-lswscale','-o',str(binary)]
with (root/'compile.log').open('x') as log:
    subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
(root/'identity.json').write_text(json.dumps({'base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),'production_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'instrumented_sha256':hashlib.sha256((root/'instrumented.c').read_bytes()).hexdigest(),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'command':command},indent=2)+'\n')
rows=[]
for trial in range(200):
    folder=root/f'trial-{trial:03d}';folder.mkdir()
    start=time.monotonic()
    try:
        with (folder/'trace.jsonl').open('w') as log,(folder/'stderr.log').open('w') as err:
            result=subprocess.run([str(binary),str(folder/'incomplete.mp4'),str(folder/'packets.h264')],stdout=log,stderr=err,timeout=25)
        events=[json.loads(line) for line in (folder/'trace.jsonl').read_text().splitlines()]
        packets=[e for e in events if e['kind']=='packet'];pictures=[e for e in events if e['kind']=='decoded']
        out_of_order=[e for e in packets if e['pts']!=e['expected_pts']]
        wrong_picture=[e for e in pictures if e['pts']!=e['picture_id']*1001]
        row={'trial':trial,'exit':result.returncode,'seconds':time.monotonic()-start,'packets':len(packets),'pictures':len(pictures),'out_of_order':out_of_order,'wrong_picture':wrong_picture}
    except subprocess.TimeoutExpired:
        row={'trial':trial,'timeout':True,'seconds':time.monotonic()-start}
    rows.append(row)
    (root/'results.json').write_text(json.dumps(rows,indent=2)+'\n')
    if trial%20==0 or row.get('out_of_order') or row.get('wrong_picture') or row.get('exit'):
        print(json.dumps(row),flush=True)
    if row.get('out_of_order') or row.get('wrong_picture'):
        break
