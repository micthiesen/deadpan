from pathlib import Path
import hashlib,json,os,subprocess,time
r=Path('/Users/michael/Code/deadpan');s=Path('/tmp/deadpan-sound-events-20260927');out=s/'playback-isolated';out.mkdir(exist_ok=False)
base=json.loads((s/'playback-retry/report.json').read_text());binary=base['command'][0]
tests=['tests::canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock','tests::original::original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock','tests::original::original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm','tests::worker_panic_is_reported_and_disables_further_requests']
results=[]
for i,name in enumerate(tests):
    command=[binary,'--exact',name,'--nocapture'];start=time.monotonic()
    with (out/f'{i}.log').open('w') as log:
        p=subprocess.run(command,cwd=r,env=dict(os.environ,DEADPAN_FFMPEG_PREFIX='/tmp/deadpan-ui-ffmpeg/prefix',RUST_BACKTRACE='0'),stdout=log,stderr=subprocess.STDOUT)
    result={'command':command,'exit_code':p.returncode,'seconds':time.monotonic()-start,'log':f'{i}.log'};results.append(result);print(json.dumps(result),flush=True)
(out/'report.json').write_text(json.dumps({'binary_sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'commands':results},indent=2)+'\n')
