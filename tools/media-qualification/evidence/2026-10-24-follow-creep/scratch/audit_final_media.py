import hashlib,json,re,struct
from collections import Counter
from pathlib import Path
o=Path('/tmp/deadpan-follow-creep-2026-10-24'); r=Path('/private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmpzVeReG')
def j(p): return json.loads(Path(p).read_text())
def sha(p):
 h=hashlib.sha256()
 with open(p,'rb') as f:
  for b in iter(lambda:f.read(1048576),b''):h.update(b)
 return h.hexdigest()
def norm(x):return {k:v for k,v in x.items() if k!='revision_id'}
inv=j(o/'media-final-invocation.json');term=j(o/'media-final-terminal.json');before_src=j(o/'media-final-source-before.json');after_src=j(o/'media-final-source-after.json');freeze=j(o/'runtime-source-freeze.json')
summary=j(r/'follow-creep-summary.json');result=j(o/'media-final-result.json');v=j(r/'follow-creep-verification.json')
ev=j(r/'fixture/follow-creep-authored-evidence.json');pb=j(r/'fixture/follow-creep-before-pcm.json');pa=j(r/'fixture/follow-creep-after-pcm.json')
copy=j(r/'fixture/follow-creep-portable-copy.json')['portable_copy'];rr=j(r/'exports/deadpan-render-e10aaedd-4d66-4b6f-828b-4a37a5c0aa69.json')
test=(o/'media-final.stdout').read_text();err=(o/'media-final.stderr').read_text();line=next((x.strip() for x in test.splitlines() if x.startswith('test result:')),None)
movie=Path(summary['movie']);audio=v['audio'][0];poses=ev['poses'];branches=Counter(x['branch'] for x in poses)
retained={name:Path(info['retained']) for name,info in inv['binaries'].items()}
testret=Path(inv['test']['retained'])
binary_matches={name:p.is_file() and sha(p)==inv['binaries'][name]['sha256'] for name,p in retained.items()}
checks={
'test_1_passed_66_filtered_not_skipped':bool(line and re.search(r'1 passed; 0 failed; 0 ignored; 0 measured; 66 filtered out',line)),
'terminal_exit_zero_binaries_and_sources_unchanged':term['exit_code']==0 and term['binaries_unchanged'] and term['runtime_sources_unchanged'],
'final_cli_hash_is_e9ad':inv['binaries']['deadpan-cli']['sha256']=='e9ad52a23d3f91ac198007f18e69811d0ec09797e24c53230255f4e083150701',
'all_four_retained_binaries_hash_match':len(binary_matches)==4 and all(binary_matches.values()),
'retained_test_hash_match':testret.is_file() and sha(testret)==inv['test']['sha256'],
'current_sources_exactly_match_freeze':before_src==after_src==freeze and hashlib.sha256((o/'runtime-source-freeze.json').read_bytes()).hexdigest()==inv['runtime_source_freeze_sha256'],
'result_equals_raw_summary':result==summary and summary['passed'] and not summary['failures'],
'120_sequential_pictures':len(v['pictures'])==120 and [x['project_frame'] for x in v['pictures']]==list(range(120)),
'picture_failures_zero':v['summary']['pictures_failed']==0,
'picture_offsets_zero':v['summary']['nonzero_offsets']==0,
'full_stereo_audio_192192':pb['channels']==2 and pb['samples']==192192 and audio['project_samples']==[0,192192] and audio['uncovered_samples']==0,
'audio_offset_verified_zero':audio['offset_status']=='verified_zero' and audio['measured_offset_samples']==0,
'authored_pcm_bits_equal':pb['canonical']['authored']==pa['authored'],
'limited_pcm_bits_equal':pb['canonical']['limited']==pa['limited'],
'limiter_gain_bits_equal':pb['canonical']['limiter_gain']==pa['limiter_gain'],
'click_window_nonzero':all(any(vv!=0 for vv in pa['authored'][i]) for i in range(47808,48192)),
'pose_120_rows_and_exact_oracle':len(poses)==120 and [x['frame'] for x in poses]==list(range(120)) and all(x['actual']==x['expected'] for x in poses),
'portable_copy_seven_receipt_verified_revisions':copy['verified_history']=={'replayed':0,'revisions':7,'verified_by_receipt':7},
'portable_original_cloned':len(copy['originals'])==1 and copy['originals'][0]['method']=='cloned',
'undo_matches_before_ignoring_revision_id':norm(ev['undone'])==norm(ev['before']),
'portable_restore_matches_after_ignoring_revision_id':norm(ev['restored'])==norm(ev['after']),
'movie_hash_matches_render_receipt':sha(movie)==rr['destination_readback']['sha256'],
}
click=tuple(struct.unpack('<f',struct.pack('<I',b))[0] for b in pa['authored'][48000])
files=[o/'media-final-invocation.json',o/'media-final-terminal.json',o/'media-final-result.json',o/'media-final.stdout',o/'media-final.stderr',o/'media-final-source-before.json',o/'media-final-source-after.json',o/'runtime-source-freeze.json',r/'follow-creep-summary.json',r/'follow-creep-verification.json',r/'follow-creep-verification.stdout.json',r/'follow-creep-verification.stderr.txt',r/'follow-creep-render.stdout.jsonl',r/'follow-creep-render.stderr.txt',r/'exports/deadpan-render-e10aaedd-4d66-4b6f-828b-4a37a5c0aa69.json',r/'fixture/follow-creep-authored-evidence.json',r/'fixture/follow-creep-before-pcm.json',r/'fixture/follow-creep-after-pcm.json',r/'fixture/follow-creep-portable-copy.json',r/'fixture/follow-creep-apply.stdout.json',r/'fixture/follow-creep-apply.stderr.txt',r/'fixture/request-follow-creep-r01.json',r/'fixture/request-follow-creep-r02.json',r/'fixture/macro-request.json',r/'fixture/original.mp4',movie,testret,*retained.values()]
hashes={str(p):{'sha256':sha(p),'bytes':p.stat().st_size} for p in files if p.is_file()}
target=next(iter(ev['after']['targets'].values()))
data={
'scope':'Independent read-only final-run audit. No product command, build, test, or repository edit. No independent DSP implementation, acoustic listening, or tracking-accuracy claim.',
'checks':checks,'mismatches':[k for k,x in checks.items() if not x],
'test':{'name':inv['test_name'],'result_line':line,'exit_code':term['exit_code'],'elapsed_seconds':term['elapsed_seconds'],'test_binary':inv['test'],'retained_test_sha256':sha(testret),'binaries_unchanged':term['binaries_unchanged'],'runtime_sources_unchanged':term['runtime_sources_unchanged']},
'retained_binaries':{k:{'path':str(p),'expected_sha256':inv['binaries'][k]['sha256'],'actual_sha256':sha(p) if p.is_file() else None,'matches':binary_matches[k]} for k,p in retained.items()},
'source_freeze':{'invocation_sha256':inv['runtime_source_freeze_sha256'],'freeze_file_sha256':sha(o/'runtime-source-freeze.json'),'files':len(freeze),'before_equals_after':before_src==after_src,'after_equals_freeze':after_src==freeze},
'video':{'movie':str(movie),'sha256':sha(movie),'bytes':movie.stat().st_size,'render_receipt_sha256':rr['destination_readback']['sha256'],'summary':v['summary']},
'audio':{'samples':len(pa['authored']),'channels':pb['channels'],'range':audio['project_samples'],'uncovered':audio['uncovered_samples'],'offset_status':audio['offset_status'],'offset_samples':audio['measured_offset_samples'],'authored_equals_before':checks['authored_pcm_bits_equal'],'limited_equals_before':checks['limited_pcm_bits_equal'],'limiter_gain_equals_before':checks['limiter_gain_bits_equal'],'nonzero_authored_pairs':sum(1 for x in pa['authored'] if any(q!=0 for q in x)),'nonzero_limited_pairs':sum(1 for x in pa['limited'] if any(q!=0 for q in x)),'nonzero_click_window':[47808,48192],'sample_48000':click,'DSP_independent':False,'listening':False},
'poses':{'rows':len(poses),'branches':dict(branches),'all_actual_equals_expected':checks['pose_120_rows_and_exact_oracle'],'target_samples':[(x['at'],x['state'],x['confidence'],x['region']['center']) for x in target['samples']],'corrections':target['corrections'],'owner_position_examples':{str(f):next(x['owner_position'] for x in poses if x['frame']==f) for f in [0,10,80,119]}},
'portable_copy':{'report':copy,'destination_exists':Path(copy['destination']).is_dir(),'undo_matches_before_ignoring_revision_id':checks['undo_matches_before_ignoring_revision_id'],'restore_matches_after_ignoring_revision_id':checks['portable_restore_matches_after_ignoring_revision_id'],'authored_history':ev['history']},
'sha256_inventory':hashes,
'limitations':['Existing burned-in counter video; target samples and correction are deterministic and manually authored. This verifies resolver/pose behavior, not tracker accuracy.','Exact PCM float-bit equality is a before/after consistency check, not an independent DSP oracle. Nonzero PCM confirms sample presence only; no listening claim.','Undo/copy state equality excludes revision_id, which necessarily changes across undo and portable copy. Hashes establish audited file identity, not provenance beyond the supplied invocation and terminal records.']}
(o/'media-final-independent-audit.json').write_text(json.dumps(data,indent=2,sort_keys=True)+'\n')
L=['# Final fresh-run media audit','','Parsed retained final-run records, media reports, PCM evidence, source files, and emitted movie only. Did not rerun product code.','',
f"- Test: {inv['test_name']}; {line}. Exit {term['exit_code']}; terminal records binaries and runtime sources unchanged.",
f"- Final retained binary hashes: four required helper/CLI binaries and the test executable match invocation digests. CLI SHA-256 {inv['binaries']['deadpan-cli']['sha256']}.",
f"- Runtime source freeze: {len(freeze)} paths; before, after, and freeze maps equal; invocation freeze SHA-256 matches file.",
f"- Video: {len(v['pictures'])} sequential pictures; {v['summary']['pictures_failed']} picture failures; {v['summary']['nonzero_offsets']} nonzero offsets. Movie SHA-256 {sha(movie)} ({movie.stat().st_size} bytes), matching render receipt.",
f"- Audio: stereo [0,192192), full coverage, verified_zero offset=0. Authored, limited, and limiter-gain arrays match bitwise to baseline. Nonzero click window [47808,48192); sample 48000 decodes as {click}. This does not claim listening or independent DSP correctness.",
f"- Pose: {len(poses)} exact per-frame rows; branches {dict(branches)}; actual equals expected for all frames. Counter footage and manually authored regions do not test tracker accuracy.",
f"- Portable copy: {copy['verified_history']}; Original cloned. Undo and copy restore compare to baseline/final state when revision_id is excluded.",
'','## Checks','']+[f"- {'PASS' if x else 'FAIL'} {k}" for k,x in checks.items()]+['','## Limitations']+[f"- {x}" for x in data['limitations']]+['','## SHA-256 inventory','']+[f"- {x['sha256']} ({x['bytes']} bytes): {p}" for p,x in hashes.items()]
(o/'media-final-independent-audit.md').write_text('\n'.join(L)+'\n')
print(json.dumps({'checks':checks,'mismatches':data['mismatches'],'branches':dict(branches),'click_sample_48000':click,'hashed_files':len(hashes),'movie_sha256':sha(movie)},indent=2))
