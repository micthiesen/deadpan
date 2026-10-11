from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib,json,subprocess,time

root=Path(__file__).parent;work=root/'actual-cfr';work.mkdir()
repo=Path('/Users/michael/Code/deadpan');prefix=Path('/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
source=repo/'native/deadpan-media-worker/src/converter.c'
main=(root/'main.c').read_text().replace('256','320').replace('144','180')
start=main.index('    for (unsigned frame = 0; frame < 120; frame++) {')
end=main.index('    if (avcodec_send_frame(proxy.encoder, NULL)',start)
main=main[:start]+'''
    FILE *input=fopen("/tmp/deadpan-proxy-order-20261011/original.rgba", "rb");
    if (!input) return 10;
    setvbuf(stdout, NULL, _IOLBF, 0);
    for (unsigned frame = 0; frame < 120; frame++) {
        if (fread(rgba,1,320U*180U*4U,input)!=320U*180U*4U) return 11;
        printf("{\\"kind\\":\\"input\\",\\"ordinal\\":%u}\\n",frame);
        if (deadpan_proxy_push(rgba,320U*180U*4U,320U*4U,frame*1001,1001,&error)) goto failed;
    }
    fclose(input);
'''+main[end:]
# Retain decoder timestamps; picture identification comes from independent raw
# decode and comparison against all 120 Original pictures after an anomaly.
drain=(root/'drain.c').read_text().replace('256','320').replace('144','180')
start=drain.index('        unsigned identity = 0;');end=drain.index('        av_frame_unref(probe_frame);',start)
drain=drain[:start]+'''        printf("{\\"kind\\":\\"decoded\\",\\"ordinal\\":%" PRIu64 ",\\"pts\\":%" PRId64 "}\\n",probe_decoded++,probe_frame->pts);
'''+drain[end:]
original=source.read_text();start=original.index('static int proxy_write_packets(void) {');end=original.index('\nint deadpan_proxy_open(',start)
(work/'instrumented.c').write_text(original[:start]+drain+original[end:]+main)
binary=work/'probe';cmd=['clang','-std=c11','-O2','-Wall','-Wextra','-Werror',str(work/'instrumented.c'),'-I'+str(source.parent),'-I'+str(prefix/'include'),'-L'+str(prefix/'lib'),'-Wl,-rpath,'+str(prefix/'lib'),'-lavcodec','-lavformat','-lavutil','-lswscale','-o',str(binary)]
with (work/'compile.log').open('x') as log:subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,check=True)
(work/'identity.json').write_text(json.dumps({'production_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'instrumented_sha256':hashlib.sha256((work/'instrumented.c').read_bytes()).hexdigest(),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'original_rgba_sha256':hashlib.sha256((root/'original.rgba').read_bytes()).hexdigest(),'command':cmd,'parallelism':8,'trials':400},indent=2)+'\n')

def run(trial):
    folder=work/f'trial-{trial:03d}';folder.mkdir();start=time.monotonic();timed_out=False
    try:
        with (folder/'trace.jsonl').open('w') as log,(folder/'stderr.log').open('w') as err:
            result=subprocess.run([str(binary),str(folder/'incomplete.mp4'),str(folder/'packets.h264')],stdout=log,stderr=err,timeout=25)
        code=result.returncode
    except subprocess.TimeoutExpired:
        code=None;timed_out=True
    events=[]
    for line in (folder/'trace.jsonl').read_text().splitlines():
        try:events.append(json.loads(line))
        except json.JSONDecodeError:pass
    packets=[e for e in events if e['kind']=='packet'];pictures=[e for e in events if e['kind']=='decoded']
    return {'trial':trial,'exit':code,'timeout':timed_out,'seconds':time.monotonic()-start,'packets':len(packets),'pictures':len(pictures),'out_of_order':[e for e in packets if e['pts']!=e['expected_pts']]}

rows=[]
with ThreadPoolExecutor(max_workers=8) as pool:
    for future in as_completed([pool.submit(run,n) for n in range(400)]):
        row=future.result();rows.append(row)
        (work/'results.json').write_text(json.dumps(sorted(rows,key=lambda r:r['trial']),indent=2)+'\n')
        if len(rows)%40==0 or row.get('out_of_order') or row.get('exit') or row.get('timeout'):print(json.dumps(row),flush=True)
