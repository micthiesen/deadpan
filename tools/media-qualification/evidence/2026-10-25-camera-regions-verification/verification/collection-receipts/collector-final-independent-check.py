import json, hashlib, pathlib, os
r=pathlib.Path('/tmp/deadpan-camera-regions-2026-10-25')
repo=pathlib.Path('/Users/michael/Code/deadpan')
def pin(p):
 b=p.read_bytes();return {'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
def load(n):return json.loads((r/n).read_text())
ids=load('verified-identities.json')
report={'scope':'Read-only independent hashing and selection audit. No collector, product, Cargo or publication invocation.','findings':[],'collector':pin(r/'collect-regions.py'),'helper':pin(r/'bundle-helpers.py'),'plans':[]}
source_total=0; rs=bytearray()
for n,row in sorted(ids['sources'].items()):
 p=repo/n; assert pin(p)==row, n; source_total+=row['bytes']
 if n.endswith('.rs'):
  b=p.read_bytes();rs.extend(b'\n// ===== '+n.encode()+b' =====\n');rs.extend(b)
  if not b.endswith(b'\n'):rs.extend(b'\n')
for n,row in ids['binaries'].items(): assert pin(pathlib.Path(row['executed']))=={k:row[k] for k in ['bytes','sha256']}, n
for plan_name in ['collection-primary.json','collection-regressions.json','collection-verification.json']:
 p=r/plan_name; d=load(plan_name)
 assert d['collector_sha256']==report['collector']['sha256']
 total=sum((r/n).stat().st_size for n in ['collect-regions.py','bundle-helpers.py','verified-identities.json',plan_name])+source_total+len(rs)
 paths=set(); arcs=[]; forbidden=[]
 for item in d['files']:
  root=repo if item.get('root')=='repository' else r
  src=root/item['source']
  got=pin(src); assert got['sha256']==item['sha256'], str(src)
  assert item['path'] not in paths,item['path'];paths.add(item['path']);total+=got['bytes']
  if item.get('png'):assert src.read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
  if any(s in item['source'] for s in ['VNObservation.h.txt','formatter.rs','.app/','/target/']):forbidden.append(item['source'])
 for a in d['archives']:
  root=r/a['root']; actual={}
  for base,dirs,files in os.walk(root):
   for name in dirs:assert not (pathlib.Path(base)/name).is_symlink()
   for name in files:
    f=pathlib.Path(base)/name;assert f.is_file() and not f.is_symlink()
    actual[f.relative_to(root).as_posix()]=pin(f)
  expected={x['path']:{k:x[k] for k in ['bytes','sha256']} for x in a['expected_members']}
  assert actual==expected,a['name']
  size=sum(x['bytes'] for x in actual.values());total+=size
  arcs.append({'name':a['name'],'members':len(actual),'bytes':size})
 assert not forbidden,forbidden
 assert total+1048576<268435456,total
 report['plans'].append({'name':plan_name,**pin(p),'files':len(d['files']),'archives':arcs,'raw_bytes_recomputed':total,'with_1MiB_reserve':total+1048576,'forbidden_payloads':forbidden})
fc=load('qualification-failure-chain.json');plan=load('collection-verification.json')
by_dest={x['path']:x for x in plan['files']}
for item in fc['selected_inputs']:
 got=pin(r/item['source']);assert got=={k:item[k] for k in ['bytes','sha256']},item['source']
 dest=by_dest[item['package_path']]
 assert dest['source']==item['source'] and dest['sha256']==item['sha256'],item['source']
actual_nonzero={}
for p in sorted(r.glob('*.json')):
 try:d=json.loads(p.read_text())
 except (ValueError,UnicodeError):continue
 if not isinstance(d,dict):continue
 cmd=d.get('command',d.get('argv'));code=d.get('exit_code')
 if cmd and isinstance(code,int) and code!=0:actual_nonzero[p.name]=(cmd,code)
recorded={x['receipt']:(x['command'],x['exit_code']) for x in fc['nonzero_invocations']}
assert actual_nonzero==recorded,(set(actual_nonzero)-set(recorded),set(recorded)-set(actual_nonzero))
for x in fc['nonzero_invocations']:
 assert pin(r/x['receipt'])['sha256']==x['receipt_sha256']
 for suffix,dest in x['streams'].items():
  f=r/(pathlib.Path(x['receipt']).stem+'.'+suffix)
  assert f.exists() and by_dest[dest]['sha256']==pin(f)['sha256']
report['failure_chain']={'manifest':pin(r/'qualification-failure-chain.json'),'selected_entries':len(fc['selected_inputs']),'actual_top_level_nonzero_command_or_argv_receipts':len(actual_nonzero),'all_match':True,'correction':'25 is the verified count. Earlier prose count26 was an enumeration error; no26th command/argv receipt was found.','historical_collector_bytes':'13425 and14c unavailable and unexecuted; manifest explicitly says hashes only.'}
report['sources_and_binaries']={'sources':len(ids['sources']),'binaries':len(ids['binaries']),'all_match':True}
(r/'collector-final-independent-check.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
