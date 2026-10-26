#!/usr/bin/env python3
"""Compare source snapshots from settled media and final release receipts."""
from pathlib import Path
import hashlib, json

ROOT=Path('/tmp/deadpan-targeted-explode-2026-10-25')
media=json.loads((ROOT/'media-second.json').read_text())
release=json.loads((ROOT/'release-build-seventh.json').read_text())
maps={
 'media-second-to-release-seventh':(media['source_after'],release['source_after']),
 'media-second-to-native-final':(media['source_after'],json.loads((ROOT/'native-final.json').read_text())['source_before']),
 'media-second-to-check-final-fifth':(media['source_after'],json.loads((ROOT/'check-final-fifth.json').read_text())['source_before']),
 'native-final-to-release-seventh':(json.loads((ROOT/'native-final.json').read_text())['source_before'],release['source_after']),
}
comparisons={}
for name,(old,new) in maps.items():
    if set(old)!=set(new): raise RuntimeError(f'{name}: source file key sets differ')
    changed=[{'path':path,'left':old[path],'right':new[path]} for path in sorted(old) if old[path]!=new[path]]
    comparisons[name]={'source_file_count':len(old),'same_path_set':True,
                       'unchanged_file_count':len(old)-len(changed),'changed_file_count':len(changed),
                       'changed_files':changed}
    expected={'media-second-to-release-seventh':3,'media-second-to-native-final':2,
              'media-second-to-check-final-fifth':3,'native-final-to-release-seventh':1}[name]
    if len(changed)!=expected: raise RuntimeError(f'{name}: unexpected changed count {len(changed)}')
result={
 'basis':{'media_receipt':str(ROOT/'media-second.json'),'media_exit_code':media['exit_code'],
          'media_sources_unchanged':media['sources_unchanged'],
          'release_receipt':str(ROOT/'release-build-seventh.json'),'release_exit_code':release['exit_code'],
          'release_sources_unchanged':release['sources_unchanged']},
 'comparisons':comparisons,
 'scope_note':'This comparison identifies the exact source changes between the passing media-second run and passing release-build-seventh; it is not a dependency analysis.'}
out=ROOT/'source-map-delta-media-second-to-release-seventh.json'
out.write_text(json.dumps(result,sort_keys=True,indent=2)+'\n')
print(json.dumps({'path':str(out),'sha256':hashlib.sha256(out.read_bytes()).hexdigest(),'bytes':out.stat().st_size,
                  'comparisons':{n:{'unchanged':v['unchanged_file_count'],'changed':v['changed_file_count'],
                                    'paths':[x['path'] for x in v['changed_files']]} for n,v in comparisons.items()}}))
