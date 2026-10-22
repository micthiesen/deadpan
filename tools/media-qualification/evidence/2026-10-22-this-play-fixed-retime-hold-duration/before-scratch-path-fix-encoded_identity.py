from pathlib import Path
import hashlib,json,shutil,sys
s=Path('/tmp/deadpan-this-play-this-play-fixed-duration-2026-10-22')
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
if sys.argv[1]=='pre':
 listing=json.loads((s/'encoded-list.json').read_text())
 paths={k:Path(v['binary-path']) for k,v in listing['rust-suites'].items() if v['status']=='listed'}
 for name in ['deadpan-cli','deadpan-media-worker','deadpan-transcribe','deadpan-track']:
  p=Path('target/release')/name
  if p.exists(): paths[name]=p
 rows={}
 for name,p in paths.items():
  copy=s/'encoded-bin'/p.name
  copy.parent.mkdir(exist_ok=True)
  shutil.copy2(p,copy)
  rows[name]={'path':str(p.resolve()),'retained':str(copy),'sha256':sha(p),'bytes':p.stat().st_size}
  assert sha(copy)==rows[name]['sha256']
 (s/'encoded-binaries.json').write_text(json.dumps(rows,indent=2)+'\n')
else:
 rows=json.loads((s/'encoded-binaries.json').read_text())
 for row in rows.values(): assert sha(Path(row['path']))==row['sha256']
 outputs=[]
 for result in json.loads((s/'encoded-result.json').read_text()):
  p=Path(result['movie']); package=Path(result['package']); report=result['report']
  files=[{'path':str(f),'bytes':f.stat().st_size,'sha256':sha(f)} for f in sorted(package.rglob('*')) if f.is_file()]
  audio=[{k:w[k] for k in ['window','project_samples','measured_offset_samples','offset_status','uncovered_samples','flags']} for w in report['audio']]
  outputs.append({'movie':str(p),'movie_bytes':p.stat().st_size,'movie_sha256':sha(p),'package':str(package),'package_files':files,'summary':report['summary'],'audio_windows':audio})
 (s/'encoded-identities.json').write_text(json.dumps({'binaries':rows,'outputs':outputs},indent=2)+'\n')
 print(json.dumps([{'movie':Path(r['movie']).name,'summary':r['summary'],'audio_windows':r['audio_windows']} for r in outputs],indent=2))
