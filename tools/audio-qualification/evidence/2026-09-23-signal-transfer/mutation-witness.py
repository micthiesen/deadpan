from pathlib import Path
import gzip, json, os, shutil, subprocess, time

repo = Path('/Users/michael/Code/deadpan')
scratch = Path('/tmp/deadpan-transfer-20260923')
copy = scratch / 'mutant'
copy.mkdir(exist_ok=True)
for name in ['crates', 'native']:
    shutil.copytree(repo / name, copy / name, dirs_exist_ok=True)
for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'rustfmt.toml', '.rustfmt.toml']:
    if (repo / name).exists():
        shutil.copy2(repo / name, copy / name)
env = os.environ.copy()
env['DEADPAN_FFMPEG_PREFIX'] = '/tmp/deadpan-media-compatible-xyhilms4/prefix'
env['CARGO_TARGET_DIR'] = str(scratch / 'mutation-target')
cases = [
    ('input-mask', 'crates/deadpan-audio/src/signal_transfer.rs',
     'block.samples[left..right].fill([0.0; 2]);',
     'let _ = (left, right); // Deliberate defect: omit prefilter suppression.',
     ['cargo', 'test', '-p', 'deadpan-audio', '--locked', '--test', 'signal_transfer', 'explicit_silence_masks_input_taps_and_zero_pcm_does_not_create_policy', '--', '--exact']),
    ('halo-work', 'crates/deadpan-audio/src/stages.rs',
     'self.read_controlled(provider, at, count, control, false)?',
     'self.read_inner(provider, at, count, control.check()?, cancelled, false)?',
     ['cargo', 'test', '-p', 'deadpan-audio', '--locked', '--test', 'stages', 'transferred_halo_', '--', '--nocapture']),
]
results = []
for name, relative, old, new, command in cases:
    path = copy / relative
    baseline = (repo / relative).read_text()
    assert baseline.count(old) == 1
    path.write_text(baseline.replace(old, new))
    start = time.monotonic()
    output = subprocess.run(command, cwd=copy, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    path.write_text(baseline)
    with gzip.open(scratch / (name + '-mutant.log.gz'), 'wt') as log:
        log.write(output.stdout)
    row = {'name': name, 'mutation': {'path': relative, 'old': old, 'new': new}, 'command': command, 'exit_code': output.returncode, 'seconds': time.monotonic() - start}
    results.append(row)
    print(json.dumps(row), flush=True)
    print(output.stdout[-3000:], flush=True)
    assert output.returncode == 101 and 'test result: FAILED.' in output.stdout, 'Expected assertion failure, not compilation failure'
(scratch / 'mutation-report.json').write_text(json.dumps(results, indent=2) + '\n')
