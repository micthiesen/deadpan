import hashlib,json,re,struct
from collections import Counter
from pathlib import Path
out=Path('/tmp/deadpan-follow-creep-2026-10-24'); root=Path('/private/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmptRnJ7C')
def j(p): return json.loads(Path(p).read_text())
def h(p):
 x=hashlib.sha256()
 with open(p,'rb') as f:
  for b in iter(lambda:f.read(1048576),b''): x.update(b)
 return x.hexdigest()
def normalized(d):
 return {k:v for k,v in d.items() if k!='revision_id'}
inv=j(out/'media-first-invocation.json'); term=j(out/'media-first-terminal.json'); bins=j(out/'first-release-binaries.json')
test=(out/'media-first.stdout').read_text(); err=(out/'media-first.stderr').read_text()
res=j(out/'media-first-result.json'); sm=j(root/'follow-creep-summary.json'); ver=j(root/'follow-creep-verification.json')
ev=j(root/'fixture/follow-creep-authored-evidence.json'); bef=j(root/'fixture/follow-creep-before-pcm.json'); aft=j(root/'fixture/follow-creep-after-pcm.json')
cp=j(root/'fixture/follow-creep-portable-copy.json')['portable_copy']; rr=j(root/'exports/deadpan-render-dcd88887-cbed-4221-8fe2-371b0b0ca6a9.json')
movie=Path(sm['movie']); poses=ev['poses']; branches=Counter(p['branch'] for p in poses); audio=ver['audio'][0]
test_line=next((x.strip() for x in test.splitlines() if x.startswith('test result:')),None)
checks={
'test_1_passed_66_filtered_not_skipped':bool(test_line and re.search(r'1 passed; 0 failed; 0 ignored; 0 measured; 66 filtered out',test_line)),
'terminal_exit_zero_and_binaries_unchanged':term['exit_code']==0 and term['binaries_unchanged'],
'raw_result_equals_summary':res==sm and not sm['failures'] and sm['passed'],
'120_sequential_pictures':len(ver['pictures'])==120 and [p['project_frame'] for p in ver['pictures']]==list(range(120)),
'picture_failures_zero':ver['summary']['pictures_failed']==0,
'picture_offsets_zero':ver['summary']['nonzero_offsets']==0,
'audio_full_192192_stereo':bef['channels']==2 and bef['samples']==192192 and audio['project_samples']==[0,192192] and audio['uncovered_samples']==0,
'audio_offset_verified_zero':audio['offset_status']=='verified_zero' and audio['measured_offset_samples']==0,
'pcm_authored_bits_equal':bef['canonical']['authored']==aft['authored'],
'pcm_limited_bits_equal':bef['canonical']['limited']==aft['limited'],
'pcm_limiter_gain_bits_equal':bef['canonical']['limiter_gain']==aft['limiter_gain'],
'pcm_signal_nonzero':any(any(v!=0 for v in row) for row in aft['authored']),
'pose_120_rows_and_oracle_matches':len(poses)==120 and [p['frame'] for p in poses]==list(range(120)) and all(p['actual']==p['expected'] for p in poses),
'portable_history_7_verified':cp['verified_history']=={'replayed':0,'revisions':7,'verified_by_receipt':7},
'portable_original_cloned':len(cp['originals'])==1 and cp['originals'][0]['method']=='cloned',
'undo_state_matches_before_except_new_revision':normalized(ev['undone'])==normalized(ev['before']),
'copy_state_matches_after_except_new_revision':normalized(ev['restored'])==normalized(ev['after']),
'source_binaries_match_initial_and_terminal':all(inv['binaries'][k]['sha256']==bins[k]['sha256'] for k in bins) and term['binaries_unchanged'],
'movie_hash_matches_render_receipt':h(movie)==rr['destination_readback']['sha256'],
}
files=[out/'media-first-invocation.json',out/'media-first-terminal.json',out/'first-release-binaries.json',out/'media-first.stdout',out/'media-first.stderr',out/'media-first-result.json',root/'follow-creep-summary.json',root/'follow-creep-verification.json',root/'follow-creep-verification.stdout.json',root/'follow-creep-verification.stderr.txt',root/'follow-creep-render.stdout.jsonl',root/'follow-creep-render.stderr.txt',root/'exports/deadpan-render-dcd88887-cbed-4221-8fe2-371b0b0ca6a9.json',root/'fixture/follow-creep-authored-evidence.json',root/'fixture/follow-creep-before-pcm.json',root/'fixture/follow-creep-after-pcm.json',root/'fixture/follow-creep-portable-copy.json',root/'fixture/follow-creep-apply.stdout.json',root/'fixture/follow-creep-apply.stderr.txt',root/'fixture/request-follow-creep-r01.json',root/'fixture/request-follow-creep-r02.json',root/'fixture/macro-request.json',root/'fixture/original.mp4',movie]
hashes={str(p):{'sha256':h(p),'bytes':p.stat().st_size} for p in files if p.is_file()}
target=next(iter(ev['after']['targets'].values()))
click_pair=tuple(struct.unpack('<f',struct.pack('<I',b))[0] for b in aft['authored'][48000])
click_window_nonzero=all(any(v!=0 for v in aft['authored'][i]) for i in range(47808,48192))
data={
'scope':'Read-only independent parsing of retained reports/media. No product command rerun. No independent DSP implementation, acoustic listening, or tracking-accuracy claim.',
'passed_checks':checks,'mismatches':[k for k,v in checks.items() if not v],
'test':{'name':inv['test_name'],'test_binary':inv['test'],'stdout_result_line':test_line,'terminal':term,'stderr_excerpt':err[:500]},
'video':{'movie':str(movie),'sha256':h(movie),'bytes':movie.stat().st_size,'pictures':len(ver['pictures']),'picture_summary':ver['summary'],'render_receipt_sha256':rr['destination_readback']['sha256'],'movie_sha256_matches_receipt':h(movie)==rr['destination_readback']['sha256']},
'audio':{'samples':192192,'channels':2,'full_range':audio['project_samples'],'uncovered':audio['uncovered_samples'],'offset_status':audio['offset_status'],'offset_samples':audio['measured_offset_samples'],'PCM_authored_equal':checks['pcm_authored_bits_equal'],'PCM_limited_equal':checks['pcm_limited_bits_equal'],'PCM_gain_equal':checks['pcm_limiter_gain_bits_equal'],'nonzero_authored_pairs':sum(1 for r in aft['authored'] if any(v!=0 for v in r)),'nonzero_limited_pairs':sum(1 for r in aft['limited'] if any(v!=0 for v in r)),'source_click_sample_48000':click_pair,'click_window_47808_48192_nonzero':click_window_nonzero,'DSP_independent':False,'listening':False},
'poses':{'rows':len(poses),'branches':dict(branches),'all_actual_equals_expected':checks['pose_120_rows_and_oracle_matches'],'target_samples':[(x['at'],x['state'],x['confidence'],x['region']['center']) for x in target['samples']],'manual_correction':target['corrections'],'follow_value':next(n['framing'] for n in ev['after']['nodes'].values() if 'framing' in n),'owner_position_examples':{str(f):next(p['owner_position'] for p in poses if p['frame']==f) for f in [0,10,80,119]}},
'portable_copy':{'report':cp,'source':str(root/'fixture/follow-creep.deadpan'),'destination_exists':Path(cp['destination']).is_dir(),'state_restored_equals_after_ignoring_revision_id':checks['copy_state_matches_after_except_new_revision'],'authored_history':ev['history'],'note':'Copy report verifies 7 revisions by receipts. Separate authored-evidence history row reports 7 revisions, 6 replayed, 0 verified-by-receipt; these counters describe separate checks.'},
'identities':{'source_sha256':rr['provenance']['catalog'][0]['qualification']['source_sha256'],'original_blake3':rr['provenance']['catalog'][0]['qualification']['original']['content']['digest'],'test_binary_sha256':inv['test']['sha256'],'release_binaries':inv['binaries'],'sha256_files':hashes},
'limitations':['Existing counter video, not a naturally moving subject. Target samples and correction are deterministic and manually authored.','Evidence checks tracking-state consumption and independent Q32 pose arithmetic, not tracking accuracy.','PCM arrays compare retained IEEE float bit patterns (authored/limited/gain) against before baseline. This is not an independent DSP oracle and makes no listening or audibility claim.','Undo and copy state compare equal after removing the intentionally different revision_id. Hashes identify audited files; they do not independently attest how invocation records were produced.']}
(out/'media-independent-audit.json').write_text(json.dumps(data,indent=2,sort_keys=True)+'\n')
lines=['# Independent retained-media audit','','Parsed only supplied invocation/terminal records and retained raw reports, PCM arrays, copy report, source file, and emitted movie. No product command, build, or test was rerun.','',
f"- Test: {inv['test_name']}; {test_line}; terminal exit {term['exit_code']}, binaries unchanged.",
f"- Video: 120 sequential frames checked; {ver['summary']['pictures_failed']} failures; {ver['summary']['nonzero_offsets']} nonzero picture offsets. Movie SHA-256 {h(movie)} ({movie.stat().st_size} bytes), matching render receipt.",
f"- Audio: stereo [0, 192192), 192192 samples, no uncovered samples, verified_zero offset at 0; PCM bit arrays match before for authored, limited, and limiter gain. Nonzero authored pairs {data['audio']['nonzero_authored_pairs']}; limited {data['audio']['nonzero_limited_pairs']}. The retained source-click window [47808, 48192) is nonzero; pair 48000 decodes to {data['audio']['source_click_sample_48000']}. This confirms PCM presence only, not listening. No independent DSP claim.",
f"- Pose evidence: 120 rows; branch counts {dict(branches)}; every actual pose equals expected. Target samples include motion, Lost, later tracked sample and manual correction; source pixels are the counter fixture, not a real moving subject.",
f"- Portable copy: {cp['verified_history']}; Original cloned; undo state equals before and restored copy state equals after after excluding revision_id. Authored history separately reports {ev['history']}.",
'','## Checks','']
lines += [f"- {'PASS' if v else 'FAIL'} {k}" for k,v in checks.items()]
lines += ['','## Limitations'] + [f'- {x}' for x in data['limitations']]
lines += ['','## SHA-256 inventory',''] + [f"- {x['sha256']} ({x['bytes']} bytes): {p}" for p,x in hashes.items()]
(out/'media-independent-audit.md').write_text('\n'.join(lines)+'\n')
print(json.dumps({'checks':checks,'branch_counts':dict(branches),'pcm_nonzero_pairs':[data['audio']['nonzero_authored_pairs'],data['audio']['nonzero_limited_pairs']],'hashed_files':len(hashes),'mismatches':data['mismatches'],'movie_sha256':h(movie)},indent=2))
