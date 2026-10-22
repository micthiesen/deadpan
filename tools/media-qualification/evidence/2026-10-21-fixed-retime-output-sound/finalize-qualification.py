from pathlib import Path
import hashlib,json,re
s=Path('/tmp/deadpan-fixed-retime-owned-sound-2026-10-21')
t=(s/'qualification-draft.md').read_text()
runtime=json.loads((s/'runtime-results.json').read_text())
e=json.loads((s/'encoded-result.json').read_text())
inventory=json.loads(Path('tools/media-qualification/evidence/2026-10-21-fixed-retime-output-sound/inventory.json').read_text())
tick=chr(96)
def tagged(text): return tick+text+tick
def sub(pattern,replacement):
 global t
 t,n=re.subn(pattern,lambda _:replacement,t,flags=re.S)
 assert n==1,(pattern,n)
def result(name):
 log=(s/runtime['runs'][name]['log']).read_text()
 line=next(line.strip() for line in reversed(log.splitlines()) if 'Summary [' in line)
 return line
sub(r'Formatting and strict changed-package Clippy passed.*?these counts are selection intent, not a recorded final pass\.',
'Final formatting and strict changed-package Clippy pass. The focused core\nselection passes all thirteen tests in 0.144 seconds, all six PCM cases pass\nin 3.107 seconds, and both real-package store cases pass in 0.575 seconds.\nThe complete core/plan selection passes all 1,726 tests in 3.742 seconds.\nThe complete audio/store selection records '+result('broad')+'.\nExecuted focused test SHA-256 identities:\n\n'+ '\n'.join('- '+name+': '+tagged(runtime['binaries'][name]['sha256'])+'.' for name in ['final-core','final-pcm','final-store'])+
'\n\nThe final test-only nonlinear refusal witness uses valid requests of at most\n256 samples. Root caught an oversized request before execution and replaced\nit with the bounded walk plus a cold single-sample refusal at8236. No\nproduction guard or meaningful-support policy was weakened.')
sub(r'ROOT: Replace this paragraph.*?source inspection\.',
'All six final PCM tests pass. The retained nonlinear limitation is explicit:\nboth pre-edit and post-edit full-bus walks fail at chunk8192 because sample8236\nis nonzero while its owner query declares [8236,8237) inactive. The exact\nUnsupported error is checked again with a cold one-sample request. Numerical\nshared-owner saturation and exact masks pass for all other literal samples,\nincluding regular, irregular and cold reads. This is not complete nonlinear\nmaster qualification; repairing that pre-existing fractional owner-support\nhole remains owed under the full development goal.')
sub(r'ROOT: Insert actual final store results.*?successfully\.',
'Both final store tests pass against real managed media and SQLite packages.\nThe success case verifies fresh and journaled growth/shrink, semantic physical\nclocks, preview/commit parity, inverse, Undo/Redo, reopen and full validation.\nThe late-failure case verifies both preview and commit remain atomic.')
sub(r'ROOT: Insert the actual new release CLI/test.*?corrected release run passes\.',
'The corrected release test passes in 7.055 seconds. All eleven pictures and\nall 17,618 presented samples pass in the two contiguous signal windows at\nmeasured offset zero, with no flags or low-SNR blocks. Thirteen audio blocks\nare compared; minimum block SNR is 30.970 dB and minimum luma PSNR is55.579 dB.\nThe independent event oracle has exactly zero maximum error before growth,\nafter growth and after shrink. The deliberately wrong phase differs by\n1.1442310810089111. Release CLI SHA-256: '+tagged(runtime['binaries']['encoded-cli']['sha256'])+
'; test binary: '+tagged(runtime['binaries']['encoded-test']['sha256'])+
'. Emitted MP4: '+tagged(hashlib.sha256(Path(e['movie']).read_bytes()).hexdigest())+
', '+str(Path(e['movie']).stat().st_size)+' bytes.\n\nThe first strict-Reject oracle compilation also exited101 because\nExactRatio.floor returns an infallible i128; removing its erroneous trailing\nquestion mark preserved every endpoint and containment assertion. The failed\ncompiler output and source are retained.')
t=t.replace('## Complete encoded output: final verification pending','## Complete encoded output')
sub(r'The intended retained evidence destination is.*?Cargo.lock remains',
'Retained evidence is in\n'+tagged('tools/media-qualification/evidence/2026-10-21-fixed-retime-output-sound')+
'.\nIts '+str(len(inventory))+' files total '+format(sum(x['bytes'] for x in inventory.values()),',')+
' bytes, plus the SHA-256 inventory. Every entry was rehashed after collection.\nReports, exact old witnesses, final verified sources, commands, reviews,\nphase oracles and media/executable identities are retained; raw media,\nprojects and executables are omitted. Checks ran on Apple M5 Max, Mac17,7,\n128 GiB, macOS26.5.2 (25F84), Rust/Cargo1.97.1. Cargo.lock remains')
assert 'ROOT:' not in t and 'pending' not in t
target=Path('docs/qualification/fixed-retime-output-sound-2026-10-21.md')
target.read_text()
target.write_text(t)
print('Final qualification written with actual verified results')
