from pathlib import Path
import os,sys,subprocess,json,hashlib,time
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
mode=sys.argv[1]
assert mode in ('release','bundle','bundle-final')
binroot={
 'release':repo/'target/release',
 'bundle':Path('/tmp/deadpan-source-interpretation-bundle-20261011/Deadpan.app/Contents/MacOS'),
 'bundle-final':Path('/tmp/deadpan-source-interpretation-final-bundle-20261011/Deadpan.app/Contents/MacOS'),
}[mode]
work=root/('production-'+mode)
work.mkdir(exist_ok=False)
if mode == 'release':
 original_binroot = binroot
 binroot = work/'bin'
 binroot.mkdir()
 for name in ['deadpan-cli','deadpan-app','deadpan-media-worker','deadpan-track','deadpan-transcribe']:
  subprocess.run(['cp','-c',str(original_binroot/name),str(binroot/name)],check=True)
# Fresh packaged home, no developer prefix or bridge paths. The release probe
# likewise uses the executable's linked dependencies and immutable resources.
probe_home=work/'home'; probe_home.mkdir()
env={'HOME':str(probe_home),'PATH':str(binroot)+':/usr/bin:/bin:/usr/sbin:/sbin','TMPDIR':'/tmp','LANG':'en_US.UTF-8'}
rows=[]; commands=[]
def run(name,args,ok=True):
 start=time.monotonic()
 p=subprocess.run(['deadpan-cli',*map(str,args)],env=env,cwd=work,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=300)
 (work/(name+'.stdout')).write_bytes(p.stdout); (work/(name+'.stderr')).write_bytes(p.stderr)
 commands.append({'name':name,'args':args,'exit':p.returncode,'seconds':time.monotonic()-start})
 (work/'commands.json').write_text(json.dumps(commands,indent=2,default=str)+'\n')
 if ok and p.returncode: raise RuntimeError(f'{name} exit {p.returncode}: {p.stderr.decode()[-2500:]}')
 return json.loads(p.stdout.splitlines()[-1] if args[0]=='render' else p.stdout),p.returncode
cases=[('cfr-bframes.mp4',{'range':'full','chroma_location':'center'}),
       ('hevc-sdr-10-limited.mp4',{'range':'full','chroma_location':'center'}),
       ('av1-sdr-8-limited.mp4',{'range':'full','chroma_location':'top_left'}),
       ('hevc-pq.mp4',{'chroma_location':'center'})]
for index,(fixture,choice) in enumerate(cases):
 name=Path(fixture).stem; package=work/(name+'.deadpan')
 source=repo/'native/deadpan-source/tests/fixtures'/fixture
 created,_=run(name+'-create',['project','create-original',package,source])
 document,_=run(name+'-before',['project','dump',package,'--json'])
 asset=created['created']['asset_id']
 request={'protocol':1,'project_id':document['project_id'],'expected_revision':document['revision_id'],'new_revision':'interpreted','command':{'command':'set_source_interpretation','asset':asset,'interpretation':choice}}
 path=work/(name+'-command.json'); path.write_text(json.dumps(request))
 run(name+'-preview',['command',package,'--json',path,'--dry-run'])
 same,_=run(name+'-after-preview',['project','dump',package,'--json']); assert same==document
 run(name+'-commit',['command',package,'--json',path])
 after,_=run(name+'-after',['project','dump',package,'--json'])
 assert after['assets']==document['assets'] and after['nodes']==document['nodes']
 assert after['source_interpretations'][asset]==choice
 output=work/(name+'-exports'); output.mkdir()
 rendered,_=run(name+'-render',['render',package,'--output',output,'--expected','interpreted'])
 assert rendered['status']['outcome']=='published',rendered
 movie=Path(rendered['status']['receipt']['movie'])
 verified,_=run(name+'-verify',['verify-export',package,'--movie',movie,'--revision','interpreted'])
 assert verified['passed'] and verified['summary']['nonzero_offsets']==0,verified
 run(name+'-undo',['project','undo',package,'--expected','interpreted'])
 undone,_=run(name+'-undone',['project','dump',package,'--json'])
 assert not undone.get('source_interpretations')
 historical,_=run(name+'-historical',['verify-export',package,'--movie',movie,'--revision','interpreted'])
 assert historical['passed']
 row={'fixture':fixture,'choice':choice,'summary':verified['summary'],'movie':str(movie),'movie_sha256':hashlib.sha256(movie.read_bytes()).hexdigest(),'historical_after_undo':historical['passed']}
 rows.append(row); print(mode,name,'passed',row['summary'],flush=True)
 (work/'report.json').write_text(json.dumps({'mode':mode,'cases':rows,'binary':str(binroot/'deadpan-cli'),'binary_sha256':hashlib.sha256((binroot/'deadpan-cli').read_bytes()).hexdigest()},indent=2)+'\n')
