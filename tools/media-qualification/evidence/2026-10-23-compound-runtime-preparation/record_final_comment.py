from pathlib import Path
import json,hashlib,subprocess,time
s=Path('/tmp/deadpan-compound-runtime-preparation-2026-10-23')
name='crates/deadpan-app/src/preview/harness/semantic.rs'
before=(s/'release-native-corrected-source'/name).read_bytes();after=Path(name).read_bytes()
old=b'    // Split has semantic dot support. Whole-Original insertion still uses its\n    // direct source-registration path and must invalidate the older cut.\n'
new=b'    // Split has semantic dot support. Whole-Original insertion has no dot\n    // recipe; its Edit-domain commit must invalidate the older cut.\n'
assert before.count(old)==1 and before.replace(old,new)==after
(s/'harness-built-before-comment.rs.txt').write_bytes(before);(s/'harness-final-after-comment.rs.txt').write_bytes(after)
sha=lambda x:hashlib.sha256(x).hexdigest()
start=time.monotonic()
with(s/'final-comment-fmt.stdout').open('w')as out,(s/'final-comment-fmt.stderr').open('w')as err:r=subprocess.run(['cargo','fmt','--all','--','--check'],stdout=out,stderr=err)
assert r.returncode==0
prod=['crates/deadpan-store/src/compound.rs','crates/deadpan-store/src/lib.rs','crates/deadpan-store/tests/compound_preparation.rs','crates/deadpan-cli/src/macros.rs','crates/deadpan-app/src/project/service/macros.rs','crates/deadpan-app/src/project/service/macros/tests.rs']
ident=json.loads((s/'release-native-corrected-source-identities.json').read_text())
assert all(sha(Path(n).read_bytes())==ident[n]for n in prod)
row={'path':name,'before_sha256':sha(before),'after_sha256':sha(after),'exactly_two_comment_lines_changed':True,'all_six_callback_files_identical_to_qualified_source':True,'after_all_25_post_steps_passed':True,'format_exit':r.returncode,'format_seconds':time.monotonic()-start,'no_behavioral_change_or_timing_claim_for_comment':True}
(s/'harness-final-comment-only.json').write_text(json.dumps(row,indent=2)+'\n')
print(json.dumps(row))
