from pathlib import Path,PurePosixPath
import hashlib,json,gzip,os,re,collections
BASE=Path('/tmp/deadpan-N-publication-finalization-2026-11-01')
OUT=Path('/tmp/deadpan-N-final-archive-audit-2026-11-01')
HOME=Path('/tmp/deadpan-read-signal-policy-admission-2026-11-01')
AR=BASE/'archive';CAND=Path('/tmp/deadpan-read-signal-policy-archive-candidate-2026-11-01')
ROOTS={'main':Path('/Users/michael/Code/deadpan'),'scoped':Path('/tmp/deadpan-query-groundwork-2026-10-26/scoped')}
CHUNK=1024*1024
def check(x,msg):
 if not x:raise AssertionError(msg)
def fp(p,unpack=False):
 h=hashlib.sha256();size=0;head=b''
 with (gzip.open(p,'rb') if unpack else p.open('rb')) as f:
  while b:=f.read(CHUNK):
   if len(head)<4:head=(head+b)[:4]
   h.update(b);size+=len(b)
 return {'bytes':size,'sha256':h.hexdigest()},head
def pin(p):
 v,_=fp(p);return {'path':str(p),**v}
def verify_pin(row):
 p=Path(row['path']);actual,_=fp(p)
 for k in ['bytes','sha256']:
  if k in row:check(row[k]==actual[k],f'pin {k} mismatch: {p}')
 return actual
def jload(p):return json.loads(p.read_text())
def json_payload(row):
 with gzip.open(AR/row['stored_path'],'rb') as f:return json.load(f)
def safe(p):
 q=PurePosixPath(p);check(q.parts and not q.is_absolute() and '..' not in q.parts,'safe path '+p);return q.as_posix()
proof=jload(BASE/'archive-build-proof.json')
check(proof['all_stored_and_expanded_hashes_verified'] and proof['input_signatures_and_final_digests_verified'],'actual build success proof')
inv=jload(AR/'inventory.json');rows=inv['records'];byname={r['path']:r for r in rows}
check(len(rows)==len(byname)==proof['records']==1080,'unique logical records')
check(pin(AR/'inventory.json')['sha256']==proof['inventory_sha256']=='2043e651f7852450472e9a2576e4961bab3a53dd0443a1d6a655243461402a7b','actual inventory SHA')
cfg=jload(CAND/'config-v2.json');freeze=jload(BASE/'directory-freeze.json')
check(pin(CAND/'config-v2.json')['sha256']==inv['configuration']['sha256']==freeze['configuration']['sha256'],'final config binding')
check(inv['configuration']['bytes']==pin(CAND/'config-v2.json')['bytes'],'config length')
check(inv['scope']==cfg['scope'] and inv['base_revision']==cfg['base_revision']=='1423c8edd378902c4233a64209f4ec6c765e9026','scope/base binding')
check(len(cfg['directories'])==33 and freeze['all_input_writers_finished'],'finished input membership')
# Reconstruct final eligible file membership independently from fixed config, without executing builder.
skip={'main','before-main','target','__pycache__','.git','payloads'};suffix={'.json','.py','.stdout','.stderr','.txt','.md','.rs','.patch'}
members={};allinputs={};skipped=[]
for d in cfg['directories']:
 root=Path(d['path']);check(root.is_dir() and not root.is_symlink(),'input root')
 for parent,dirs,files in os.walk(root,followlinks=False):
  for n in dirs:
   if n in skip:skipped.append(str(Path(parent)/n))
  dirs[:]=sorted(n for n in dirs if n not in skip)
  for n in sorted(files):
   p=Path(parent)/n
   if p.suffix not in suffix:continue
   check(p.is_file() and not p.is_symlink(),'input regular file')
   name=safe(d['prefix'].strip('/')+'/'+p.relative_to(root).as_posix())
   check(name not in allinputs,'duplicate actual candidate')
   members[name]=p;allinputs[name]=p
owned=jload(Path(cfg['owned_code_paths']));check(len(owned)==len(set(owned))==23,'curated23code')
for r in cfg['files']:
 name=safe(r['name']);check(name not in allinputs,'explicit duplicate')
 allinputs[name]=Path(r['path'])
 if name.startswith('shipping-source/'):members[name]=Path(r['path'])
for rel in owned:
 name=safe('shipping-source/'+rel);p=Path(cfg['scope_root'])/safe(rel)
 check(name not in allinputs or allinputs[name].resolve()==p.resolve(),'owned collision')
 members[name]=p;allinputs[name]=p
fr={x['path']:x for x in freeze['records']}
check(set(members)==set(fr) and len(fr)==1075,'exact1075 frozen membership')
check(set(allinputs)==set(byname),'all1080 archive names match final actual inputs')
check(set(allinputs)-set(members)==set(['support/blake3_stream.rs','support/M-archive-builder-v2.py','support/M-metadata-binding-regression.json','support/directory-freeze.json','support/freeze_inputs.py']),'five explicit support inputs outside freeze')
check(not any(p.as_posix().startswith('/Users/michael/Code/deadpan/') for p in allinputs.values()),'no Main whole source inputs')
check(not any(x.endswith('.orig') or '.orig/' in x for x in byname),'no .orig bodies')
check(not any(any(y in {'main','before-main','.git','target'} for y in PurePosixPath(x).parts) for x in byname),'no whole Main checkout/tree')
# Measure every selected physical source alias against the frozen and archive row pins.
for i,(name,p) in enumerate(sorted(allinputs.items())):
 a,_=fp(p);r=byname[name]
 check(a=={k:r[k] for k in ['bytes','sha256']},'actual source alias '+name)
 if name in fr:
  check(a=={k:fr[name][k] for k in ['bytes','sha256']} and p.resolve()==Path(fr[name]['source']).resolve(),'frozen source alias '+name)
 if i and i%250==0:print('source aliases',i,flush=True)
print('all1080 source aliases and1075 membership verified',flush=True)
# Independently verify stored and expanded streams once per physical payload.
stored={}
for r in rows:
 safe(r['path']);safe(r['stored_path'])
 expected='payloads/'+r['sha256']+'.gz';check(r['stored_path']==expected,'content-addressed payload name')
 old=stored.setdefault(r['stored_path'],r)
 check(all(old[k]==r[k] for k in ['bytes','sha256','stored_bytes','stored_sha256']),'all shared aliases equal')
check(len(stored)==proof['unique_payloads']==785,'unique physical payload count')
check(sum(r['bytes'] for r in rows)==proof['expanded_bytes_with_aliases']==5157749876,'expanded alias total')
check(sum(r['stored_bytes'] for r in stored.values())==proof['stored_bytes']==129246220,'stored total')
physical={p.relative_to(AR).as_posix() for p in AR.rglob('*') if p.is_file()}
check(physical==set(stored)|{'inventory.json'},'no extra or missing primary archive files')
native_magic=(b'\x7fELF',b'MZ',b'\xfe\xed\xfa\xce',b'\xce\xfa\xed\xfe',b'\xfe\xed\xfa\xcf',b'\xcf\xfa\xed\xfe',b'\xca\xfe\xba\xbe')
expanded_unique=0
for i,(name,r) in enumerate(sorted(stored.items()),1):
 p=AR/name;check(not p.is_symlink(),'stored file no symlink')
 packed,head=fp(p);check(packed=={'bytes':r['stored_bytes'],'sha256':r['stored_sha256']},'stored SHA '+name)
 expanded,head=fp(p,True);check(expanded=={'bytes':r['bytes'],'sha256':r['sha256']},'expanded SHA '+name)
 check(not any(head.startswith(m) for m in native_magic),'no packaged native executable payload')
 expanded_unique+=expanded['bytes']
 if i%100==0:print('stored+expanded payloads',i,flush=True)
print('all785 stored+expanded streams verified',flush=True)
# Acceptance/config/guard/receipt/audit authority must use these exact archived bytes.
accept_name=cfg['acceptance_record'];accept=json_payload(byname[accept_name])
check(accept['acceptance_verdict']=='GO' and inv['acceptance']['path']==accept_name and inv['acceptance']['sha256']==byname[accept_name]['sha256']==proof['acceptance']['sha256'],'archived acceptance exact')
check(byname[accept_name]['bytes']==proof['acceptance']['bytes']==10987,'acceptance length')
check(json_payload(byname['evidence/deadpan-read-signal-policy-archive-candidate-2026-11-01/config-v2.json'])==cfg,'archived finalconfig exact')
check(json_payload(byname['support/directory-freeze.json'])==freeze,'archived membership freeze exact')
def named_for_local(p):
 real=Path(p).resolve()
 names=[n for n,x in allinputs.items() if x.resolve()==real]
 check(names,'referenced authority not archived: '+p)
 return names
def archived_pin_join(row):
 actual=verify_pin(row);names=named_for_local(row['path'])
 check(all(byname[n]['bytes']==actual['bytes'] and byname[n]['sha256']==actual['sha256'] for n in names),'authority archived join '+row['path'])
 return names
authorities=[]
for r in accept['independent_bounded_audits']:
 ns=archived_pin_join(r);obj=json_payload(byname[ns[0]]);check(obj[r['field']]==r['bounded_verdict'],'independent authority actualGO')
 authorities.append({'names':ns,'sha256':r['sha256'],'verdict':obj[r['field']]})
for r in accept['commands']:archived_pin_join(r)
for key in ['matrix','actual_crypto_replay']:
 row=accept[key]['proof'] if key=='matrix' else accept[key]
 archived_pin_join(row)
for row in accept['documentation'].values():
 if isinstance(row,dict) and 'path' in row:archived_pin_join(row)
# Independently join every actual matrix receipt and its real stdout/stderr and compare totals.
matrix=json_payload(byname[named_for_local(accept['matrix']['proof']['path'])[0]])
totals={r:collections.Counter(targets=0,passed=0,failed=0,ignored=0,filtered=0) for r in ROOTS}
for x in matrix['actual_target_receipts']:
 ns=archived_pin_join({'path':x['receipt'],'bytes':x['bytes'],'sha256':x['sha256']});j=json_payload(byname[ns[0]]);root=x['variant']
 check(j['exit_code']==0 and j['sources_unchanged'] and j['binaries_unchanged'] and j['binary_before']==j['binary_after'],'actual matrix clean exit/freeze')
 check(j['source_before']==j['source_after'],'actual matrix guard unchanged')
 stdoutp=Path(x['receipt']).with_suffix('.stdout');stderrp=Path(x['receipt']).with_suffix('.stderr')
 for stream,p in [('stdout',stdoutp),('stderr',stderrp)]:
  a,_=fp(p);check(a==j['streams'][stream],'matrix stream pin');named_for_local(str(p))
 text=stdoutp.read_text();matches=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',text)
 check(matches,'actual matrix stdout summary')
 totals[root]['targets']+=1
 for status,passed,failed,ignored,measured,filtered in matches:
  check(status=='ok' and int(failed)==0,'actual matrix status')
  for k,v in [('passed',passed),('failed',failed),('ignored',ignored),('filtered',filtered)]:totals[root][k]+=int(v)
for root in ROOTS:
 check(dict(totals[root])==accept['matrix']['summary'][root]==matrix['summary'][root],'actual matrix totals')
check(totals['scoped']['passed']==1156 and totals['main']['passed']==1162 and totals['scoped']['ignored']==totals['main']['ignored']==40 and totals['main']['filtered']==7 and totals['scoped']['filtered']==0,'final doc matrix numbers')
# Full current named runtime maps remain exact, including unowned Main WIP; no source content is copied from Main.
source_counts={};owned_pins=[]
for root,repo in ROOTS.items():
 j=jload(HOME/f'read-signal-policy-after-build3-{root}.json');mp=j['source_after']
 check(len(mp)==(3000 if root=='main' else 2684),'named runtime source count')
 for rel,expected in mp.items():
  a,_=fp(repo/rel);check(a==expected,root+' current runtime source '+rel)
 source_counts[root]=len(mp)
 for rel in owned:
  check(not (repo/rel).is_symlink(),root+' curated file no symlink '+rel)
  a,_=fp(repo/rel);check(a=={k:byname['shipping-source/'+rel][k] for k in ['bytes','sha256']},root+' curated source '+rel)
 if root=='scoped':owned_pins=[{'path':rel,**fp(repo/rel)[0]} for rel in owned]
 print(root,'named runtime maps and23 owned source verified',flush=True)
# All42 immutable payloads are unchanged in both roots.
oracles=jload(HOME/'immutable-oracle-pins42.json');check(len(oracles)==42,'42 literal oracle pins')
for root,repo in ROOTS.items():
 for r in oracles:
  a,_=fp(repo/r['path']);check(a['sha256']==r['sha256'] and ('bytes' not in r or a['bytes']==r['bytes']),root+' frozen payload '+r['path'])
# Source.orig salts are present locally as declared but never shipping/archive payload records.
orig=[rel for rel in jload(HOME/'read-signal-policy-after-build3-scoped.json')['source_after'] if rel.endswith('.orig')]
check(len(orig)==5 and not any(str(p).endswith('.orig') for p in allinputs.values()),'five local orig salts excluded')
# Four final documents are exact in both roots, archived and match acceptance documentation pins.
docpins=jload(HOME/'documentation-pins.json');docresults=[];n_text={}
for rel,expected in docpins.items():
 name='shipping-source/'+rel;check(name in byname,'archived shipping doc '+rel)
 for root,repo in ROOTS.items():
  a,_=fp(repo/rel);check(a==expected=={k:byname[name][k] for k in ['bytes','sha256']},root+' doc pin '+rel)
  if root=='scoped':n_text[rel]=(repo/rel).read_text()
 docresults.append({'path':rel,**expected})
drafts=Path('/tmp/deadpan-read-signal-policy-documentation-2026-11-01')
for draft,rel in [('qualification.md','docs/qualification/read-signal-policy-admission-2026-11-01.md'),('handoff-section.md','docs/spec/AGENT_HANDOFF.md'),('design-section.md','docs/design/projected-policy-source-admission.md'),('requirements-section.md','docs/REQUIREMENTS.md')]:
 check((drafts/draft).read_text().strip() in n_text[rel],'final N draft section exact '+draft)
# Local N links: baseline and final evidence inventory are real under both mirror roots.
refs=[]
for draft,rel in [('qualification.md','docs/qualification/read-signal-policy-admission-2026-11-01.md'),('handoff-section.md','docs/spec/AGENT_HANDOFF.md'),('design-section.md','docs/design/projected-policy-source-admission.md'),('requirements-section.md','docs/REQUIREMENTS.md')]:
 text=(drafts/draft).read_text()
 for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',text):
  if '://' in target or target.startswith('#'):continue
  local=target.split('#',1)[0]
  for root,repo in ROOTS.items():
   p=(repo/rel).parent/local;check(p.is_file(),'supported N local doc reference missing '+str(p))
   if p.name=='inventory.json':check(pin(p)['sha256']==proof['inventory_sha256'],'local mirror inventory reference')
   refs.append({'root':root,'document':rel,'link':target,'target':str(p.resolve()),**fp(p)[0]})
# Six .bin crypto inputs are exactly recoverable from the archived literal inputhex receipt.
crypto=json_payload(byname[named_for_local(accept['actual_crypto_replay']['path'])[0]])
check(crypto['acceptance_verdict']=='GO_ACTUAL_HASH_REPLAY_ONLY','crypto actualboundedGO')
recovered=[]
for e in crypto['executions']:
 data=bytes.fromhex(e['input_hex']);check(len(data)==e['input_bytes'] and hashlib.sha256(data).hexdigest()==e['input_sha256'],'literal inputhex exact')
 check(e['tool_before']==e['tool_after'] and e['exit_code']==0 and e['actual_stdout'].strip()==e['expected_digest'] and e['actual_stderr']=='','actual crypto guarded stdout')
 if e['source'] is not None:
  check(e['source_before']==e['source_after']=={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()},'crypto source before/after exacthex')
  check(Path(e['source']).read_bytes()==data,'excludedbin literal recovery exact')
  recovered.append({'source':e['source'],'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
check(len(recovered)==6 and len(crypto['executions'])==8,'sixpayload+twovector actualreplay')
check(not any(n.endswith('.bin') for n in byname),'no packaged native/bin raw inputs')
# Rehash both physical repo mirrors against primary; separate buildproof supplement is exact.
mirror_records=[]
for root,repo in ROOTS.items():
 dest=repo/'tools/media-qualification/evidence/2026-11-01-read-signal-policy-admission'
 got={p.relative_to(dest).as_posix() for p in dest.rglob('*') if p.is_file()}
 check(got==physical|{'archive-build-proof.json'},root+' exact787 mirror files')
 for name in sorted(physical):
  p=dest/name
  check(not p.is_symlink(),'mirror file no symlink')
  expected=pin(AR/name);a=pin(p);check(a['bytes']==expected['bytes'] and a['sha256']==expected['sha256'],root+' mirror '+name)
 check(fp(dest/'archive-build-proof.json')[0]==fp(BASE/'archive-build-proof.json')[0],root+' buildproof mirror')
 mirror_records.append({'root':root,'files':len(got),'inventory_sha256':pin(dest/'inventory.json')['sha256'],'all_primary_bytes_equal':True,'build_proof_supplement_exact':True})
 print(root,'actual787 mirrored files verified',flush=True)
result={'verdict':'GO_BOUNDED_ACTUAL_ARCHIVE_REFERENCE_AND_CURATED_SOURCE','findings':[],'actual_build_proof':pin(BASE/'archive-build-proof.json'),'inventory':pin(AR/'inventory.json'),'final_config':pin(CAND/'config-v2.json'),'membership_freeze':pin(BASE/'directory-freeze.json'),'counts':{'records':len(rows),'unique_payloads':len(stored),'frozen_members':len(fr),'stored_bytes':proof['stored_bytes'],'expanded_bytes_with_aliases':proof['expanded_bytes_with_aliases'],'expanded_unique_payload_bytes':expanded_unique,'max_stored_payload_bytes':max(r['stored_bytes'] for r in rows)},'all_stored_and_expanded_streams_independently_verified':True,'all1080_current_source_aliases_match':True,'all1075_membership_and_frozen_source_pins_match':True,'acceptance':{'path':accept_name,'bytes':byname[accept_name]['bytes'],'sha256':byname[accept_name]['sha256'],'verdict':accept['acceptance_verdict']},'authority_joins':authorities,'actual_matrix_totals':{k:dict(v) for k,v in totals.items()},'current_named_runtime_source_counts':source_counts,'curated23Scope_source_proposal':owned_pins,'all42_frozen_payloads_both_roots_fixed':True,'local_orig_salts_excluded':orig,'docs':docresults,'supported_local_N_doc_references':refs,'six_crypto_bin_inputs_recovered_from_archived_hex':recovered,'actual_mirrors':mirror_records,'scope':'N ordinary read_signal Signal/Tape producer only; no product gate,DP,coverage score or wholepreparation acceptance. Publication commit/push/remote tree verification remains Root-owned and is not certified here.','execution':'Read-only Python streams/metadata/source/reference inspection, writes only to new final-archive-audit scratch directory. No builder/compiler/Cargo/native/helper/tests/Git execution.'}
(OUT/'actual-audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'verdict':result['verdict'],'counts':result['counts'],'report':pin(OUT/'actual-audit.json')},indent=2),flush=True)
