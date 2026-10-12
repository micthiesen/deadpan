from pathlib import Path
import gzip
import hashlib
import json
import shutil
import subprocess

root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
dest=repo/'tools/audio-qualification/evidence/2026-10-11-persistent-audio'
dest.mkdir(parents=True, exist_ok=True)
owned=[
 'Cargo.lock', 'crates/deadpan-audio/Cargo.toml','crates/deadpan-audio/build.rs',
 'crates/deadpan-audio/src/lib.rs','crates/deadpan-audio/src/limited.rs',
 'crates/deadpan-audio/src/progress.rs','crates/deadpan-audio/src/stages.rs',
 'crates/deadpan-audio/src/persistent_pcm.rs','crates/deadpan-audio/src/persistent_pcm_tests.rs',
 'crates/deadpan-audio/src/persistent_stage_tests.rs','crates/deadpan-audio/src/stage_cache.rs',
 'crates/deadpan-playback/src/controller.rs','crates/deadpan-playback/src/preparation.rs','crates/deadpan-playback/src/warming.rs',
 'crates/deadpan-cli/examples/perf/playback.rs','crates/deadpan-cli/src/audio.rs','crates/deadpan-cli/src/storage.rs',
 'crates/deadpan-app/src/preview/diagnostics.rs','crates/deadpan-app/src/preview/storage.rs','crates/deadpan-app/src/transport.rs',
 'crates/deadpan-app/src/preview/harness/diagnostics.rs','crates/deadpan-app/src/preview/harness/storage.rs',
 'docs/AUDIO_STAGE_PREPARATION.md','docs/PLAYBACK.md','docs/STORAGE.md','docs/REQUIREMENTS.md',
 'docs/SAVED_AUDIO.md','docs/spec/AGENT_HANDOFF.md','docs/qualification/persistent-audio-2026-10-11.md',
]
subprocess.run(['git','add','--',*owned],cwd=repo,check=True)
diff=subprocess.check_output(['git','diff','--cached','--binary','--','Cargo.lock','crates'],cwd=repo)
(dest/'reviewed-source.diff.gz').write_bytes(gzip.compress(diff,mtime=0))
sources={name:hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in owned}
(dest/'source-files.json').write_text(json.dumps({'base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),'sha256':sources},indent=2)+'\n')
for path in root.iterdir():
 if path.is_file() and path.suffix in ['.json','.py']:
  shutil.copyfile(path,dest/path.name)
 elif path.is_file() and path.suffix=='.log':
  (dest/(path.name+'.gz')).write_bytes(gzip.compress(path.read_bytes(),mtime=0))
for group,prefix,names in [('replays','',['original-playback','sound-playback','diagnostics','storage']),('visual-replays','visual-',['diagnostics','storage'])]:
 for name in names:
  data=(root/group/name/'report.json').read_bytes()
  (dest/f'replay-{prefix}{name}.json.gz').write_bytes(gzip.compress(data,mtime=0))
for group,name,png in [('diagnostics','diagnostics','diagnostics-014.png'),('storage','storage','storage-015.png')]:
 shutil.copyfile(root/'visual-replays'/group/png,dest/f'{name}.png')
for name in ['fixture-document.json','fixture-retime.json','fixture-lead.json']:
 shutil.copyfile(Path('/tmp/deadpan-audio-ahead-20261011')/name,dest/('short-'+name))
long=json.loads(Path('/tmp/deadpan-disk-pcm-20261011/export.json').read_text())
(dest/'long-fixture.json').write_text(json.dumps({k:long[k] for k in ['fixture','package','revision','movie']},indent=2)+'\n')
cache_records=[]
for cache in ['final-device-cache','final-long-cache']:
 for path in (root/cache).iterdir():
  if path.suffix!='.pcm':continue
  with path.open('rb') as stream:
   header=stream.read(80)
   stream.seek(0)
   sha=hashlib.file_digest(stream,'sha256').hexdigest()
  cache_records.append(dict(directory=cache,name=path.name,bytes=path.stat().st_size,sha256=sha,
      format=header[:8].decode(),key='blake3:'+header[8:40].hex(),frames=int.from_bytes(header[40:48],'little'),
      payload_checksum='blake3:'+header[48:80].hex()))
(dest/'cache-records.json').write_text(json.dumps(cache_records,indent=2)+'\n')
manifest={path.name:dict(bytes=path.stat().st_size,sha256=hashlib.sha256(path.read_bytes()).hexdigest())
          for path in sorted(dest.iterdir()) if path.name!='manifest.json'}
(dest/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
subprocess.run(['python3',str(dest/'check-evidence.py')],cwd=repo,check=True)
print(json.dumps(dict(files=len(manifest),bytes=sum(record['bytes'] for record in manifest.values()))))
