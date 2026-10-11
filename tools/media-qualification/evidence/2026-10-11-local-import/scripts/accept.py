from pathlib import Path
import hashlib,json,os,subprocess,time
R=Path(__file__).parent
REPO=Path('/Users/michael/Code/deadpan')
BIN=REPO/'target/release/deadpan-cli'
ENV=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
P=R/'Original.deadpan'
W=R/'linked.wav'
FIX=REPO/'native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav'
def run(name,*args,exit=0):
    start=time.monotonic()
    with (R/(name+'.stdout')).open('x') as out,(R/(name+'.stderr')).open('x') as err:
        p=subprocess.run([str(BIN),*map(str,args)],cwd=REPO,env=ENV,stdout=out,stderr=err,timeout=180)
    rec={'argv':[str(BIN),*map(str,args)],'exit':p.returncode,'seconds':time.monotonic()-start,'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest()}
    for suffix in ['stdout','stderr']:rec[suffix+'_sha256']=hashlib.sha256((R/(name+'.'+suffix)).read_bytes()).hexdigest()
    (R/(name+'.json')).write_text(json.dumps(rec,indent=2)+'\n')
    print(name,p.returncode,round(rec['seconds'],3),flush=True)
    assert (p.returncode==0 if exit==0 else p.returncode!=0),rec
    raw=(R/(name+'.stdout')).read_text().strip()
    if not raw:return None
    dec=json.JSONDecoder(); objects=[]
    while raw:
        obj,pos=dec.raw_decode(raw);objects.append(obj);raw=raw[pos:].strip()
    return objects[0] if len(objects)==1 else objects

def write(name,obj):
    f=R/(name+'.request.json');f.write_text(json.dumps(obj,indent=2)+'\n');return f

if __name__=='__main__':
    baseline=json.loads((R/'dump-fixed.log').read_text())
    retained=json.loads((R/'retain.log').read_text())['retained_original']['record']
    reg={'protocol':1,'registration':{'expected_revision':baseline['revision_id'],'new_revision':'linked-sound-registration','original':retained['object']['content'],'new_asset_id':'linked-sound','label':'Linked sound','insertion':None},'streams':{'type':'audio_only','stream':0,'interpretation':'stereo_left_right'}}
    run('register','project','register-source',P,'--request-json',write('register',reg))
    catalog=run('catalog','project','dump',P,'--json')
    assert catalog['nodes']==baseline['nodes'] and catalog['root']==baseline['root'] and catalog['presentation_basis']==baseline['presentation_basis']
    assert catalog['assets'][baseline['basis_state']['primary']['asset']]==baseline['assets'][baseline['basis_state']['primary']['asset']]
    assert catalog['assets']['linked-sound']['video'] is None
    run('place','sound',P,'--json',write('place',{'protocol':1,'expected_revision':catalog['revision_id'],'edit':{'type':'place','asset':'linked-sound','at':{'frame':10}}}))
    placed=run('placed','project','dump',P,'--json')
    (R/'output').mkdir()
    original=W.read_bytes()
    W.unlink()
    run('missing-render','render',P,'--output',R/'output','--name','missing.mp4',exit=1)
    assert not (R/'output/missing.mp4').exists()
    assert run('missing-dump','project','dump',P,'--json')==placed
    W.write_bytes(bytes([original[0]^1])+original[1:])
    assert W.stat().st_size==len(original)
    run('corrupt-render','render',P,'--output',R/'output','--name','corrupt.mp4',exit=1)
    assert not (R/'output/corrupt.mp4').exists()
    assert run('corrupt-dump','project','dump',P,'--json')==placed
    W.write_bytes(original)
    run('verify-restored','project','verify-original',P,retained['object']['content']['digest'])
    run('restored-render','render',P,'--output',R/'output','--name','restored.mp4')
    assert (R/'output/restored.mp4').is_file()
    assert run('restored-dump','project','dump',P,'--json')==placed
    PORT=R/'Portable.deadpan'
    run('portable-copy','project','copy-portable',P,PORT)
    inventory=run('portable-originals','project','originals',PORT)
    print('portable inventory keys',list(inventory),flush=True)
    W.unlink()
    run('portable-validate','project','validate',PORT)
    run('portable-render','render',PORT,'--output',R/'output','--name','portable.mp4')
    assert (R/'output/portable.mp4').is_file()
    assert run('portable-dump','project','dump',PORT,'--json')==placed
    run('source-offline-render','render',P,'--output',R/'output','--name','source-offline.mp4',exit=1)
    assert not (R/'output/source-offline.mp4').exists()
    (R/'acceptance-result.json').write_text(json.dumps({'status':'pass','revision':placed['revision_id'],'checks':['catalog preserves picture, basis and Original','missing placed sound prevents publication without document changes','same-length corrupt placed sound prevents publication without document changes','identical restoration renders without document changes','portable copy renders with external sound absent and unchanged authored state','source package still refuses with external sound absent']},indent=2)+'\n')
