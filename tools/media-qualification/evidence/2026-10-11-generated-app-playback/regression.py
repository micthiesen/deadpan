from pathlib import Path
import subprocess
root=Path(__file__).parent
repo=Path('/Users/michael/Code/deadpan')
current=(repo/'crates/deadpan-app/src/preview/harness/telemetry.rs').read_text()
old=(root/'telemetry-before.rs').read_text()
start='    #[test]\n    fn next_decode_request_preserves_the_submitted_picture_until_composition()'
end='    #[test]\n    fn failures_are_retained_without_success_samples_and_recovery_has_its_own_clock()'
tests=current[current.index(start):current.index(end)]
assert old.count(end)==1
worker=(repo/'crates/deadpan-app/src/worker.rs').read_text()
start_at=worker.index('#[derive(Clone, Copy, Debug)]\npub struct WorkerTiming')
worker=worker[start_at:worker.index('\n}',start_at)+2]
for label,source in [('old',old.replace(end,tests+end)),('fixed',current)]:
 directory=root/('telemetry-'+label);directory.mkdir()
 (directory/'telemetry.rs').write_text(source)
 wrapper='mod worker { use std::time::Instant;\n'+worker+'\n}\nmod preview { mod repeat_queue { pub(super) const MAX_WAITING: usize = 16; }\n#[path = '+repr(str(directory/'telemetry.rs')).replace("'",'"')+'] mod telemetry; }\n'
 (directory/'test.rs').write_text(wrapper)
 subprocess.run(['python3',str(root/'run.py'),'telemetry-'+label+'-build','rustc','--test','--edition=2024',str(directory/'test.rs'),'-o',str(directory/'tests')],check=True)
 result=subprocess.run(['python3',str(root/'run.py'),'telemetry-'+label,str(directory/'tests'),'--test-threads=1'])
 assert result.returncode == (101 if label=='old' else 0),(label,result.returncode)
