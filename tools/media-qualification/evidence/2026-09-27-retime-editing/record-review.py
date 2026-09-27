from pathlib import Path
import hashlib,json
s=Path('/tmp/deadpan-retime-20260927'); r=Path('/Users/michael/Code/deadpan')
source=json.loads((s/'review-paths.json').read_text())
assert all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in source.items())
record={
 'git_review_base':'origin/main',
 'increment_base':'preserved before/ matching preceding verified original-audition checkpoint',
 'source_sha256':source,
 'reviews':[
  {'agent':'retime_general_review','lens':'Core/app command correctness, interface integration, marks and bindings','findings':[]},
  {'agent':'retime_timing_review','lens':'Exact timing, pitch policy, retained audio bindings and processing history',
   'finding':'The split-Partition PCM test compared chunk sizes from the same plan, which did not independently establish retained processing history.',
   'action':'Added direct canonical DSP/resampling expectations over the complete inner history, proved a restarted history differs for both pitch policies, and read the suffix first through a fresh StageAudio cache.',
   'followup':'Reviewer confirmed the independent oracle detects lost child processing history for both pitch policies.'},
  {'agent':'retime_history_review','lens':'Strict legacy grammar, schema-33 replay, genuine old-CLI fixture provenance and durable history','findings':[],
   'followup':'Reviewed the two late CLI doctor/test changes. Schemas 28/34 and partial capability labels match the implementation; full device/acoustic qualification remains open. No findings.'}
 ],
 'limits':['Independent reviewers inspected code and fixture evidence without running Cargo. Parent owns verification.','No native visual, acoustic or accessibility qualification is established by static review.'],
 'verification_findings':[{'finding':'The broad workspace run caught the previous schema numbers in the CLI doctor integration test and an outdated migration label in doctor output.','action':'Updated schemas to core 28/database 34, migration-through-33 label, and partial native speed/Original/loop audition capability labels. Historical old-CLI fixture output remains unchanged.','verification':'All 17 CLI project command tests rerun in final-checks; see retained report for actual results.'}],
 'discarded_findings':[], 'known_material_findings_remaining':[]
}
(s/'review.json').write_text(json.dumps(record,indent=2)+'\n')
print('Recorded reviews of',len(source),'source paths')
