from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib, json, subprocess, time

root=Path(__file__).parent
work=root/'concurrent-320x180'
work.mkdir()
repo=Path('/Users/michael/Code/deadpan')
prefix=Path('/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
source=repo/'native/deadpan-media-worker/src/converter.c'
drain=(root/'drain.c').read_text().replace('256','320').replace('144','180').replace('bit * 32 + 16','bit * 40 + 20').replace('y = 72','y = 90')
main=(root/'main.c').read_text().replace('256','320').replace('144','180').replace('x / 32','x / 40')
original=source.read_text();start=original.index('static int proxy_write_packets(void) {');end=original.index('\nint deadpan_proxy_open(',start)
(work/'instrumented.c').write_text(original[:start]+drain+original[end:]+main)
binary=work/'probe'
command=['clang','-std=c11','-O2','-Wall','-Wextra','-Werror',str(work/'instrumented.c'),'-I'+str(source.parent),'-I'+str(prefix/'include'),'-L'+str(prefix/'lib'),'-Wl,-rpath,'+str(prefix/'lib'),'-lavcodec','-lavformat','-lavutil','-lswscale','-o',str(binary)]
with (work/'compile.log').open('x') as log:subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
(work/'identity.json').write_text(json.dumps({'production_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'instrumented_sha256':hashlib.sha256((work/'instrumented.c').read_bytes()).hexdigest(),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'command':command,'parallelism':8,'trials':400},indent=2)+'\n')

def run(trial):
    folder=work/f'trial-{trial:03d}';folder.mkdir()
    start=time.monotonic()
    try:
        with (folder/'trace.jsonl').open('w') as log,(folder/'stderr.log').open('w') as err:
            result=subprocess.run([str(binary),str(folder/'incomplete.mp4'),str(folder/'packets.h264')],stdout=log,stderr=err,timeout=25)
        events=[json.loads(line) for line in (folder/'trace.jsonl').read_text().splitlines()]
        packets=[e for e in events if e['kind']=='packet'];pictures=[e for e in events if e['kind']=='decoded']
        return {'trial':trial,'exit':result.returncode,'seconds':time.monotonic()-start,'packets':len(packets),'pictures':len(pictures),'out_of_order':[e for e in packets if e['pts']!=e['expected_pts']],'wrong_picture':[e for e in pictures if e['pts']!=e['picture_id']*1001]}
    except subprocess.TimeoutExpired:
        return {'trial':trial,'timeout':True,'seconds':time.monotonic()-start}

rows=[]
with ThreadPoolExecutor(max_workers=8) as pool:
    for future in as_completed([pool.submit(run,n) for n in range(400)]):
        row=future.result();rows.append(row)
        (work/'results.json').write_text(json.dumps(sorted(rows,key=lambda r:r['trial']),indent=2)+'\n')
        if len(rows)%40==0 or row.get('out_of_order') or row.get('wrong_picture') or row.get('exit') or row.get('timeout'):
            print(json.dumps(row),flush=True)
