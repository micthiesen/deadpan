#!/usr/bin/env python3
"""Offline evidence audit. Launches no product process and preserves the failed run."""
from datetime import datetime, timezone
from fractions import Fraction as F
import copy
import hashlib
import json
from pathlib import Path
import stat
import sys

S = Path(__file__).resolve().parent
D = S / 'clipped-media-final'
OUT = S / 'clipped-media-reconciliation.json'
EXPECTED_SCRIPT = '00aaf769d321c12cb7067c0de2c2c8f94ae6253e319c0f82de695883a75e8466'
EMPTY = {'bytes':0, 'sha256':hashlib.sha256(b'').hexdigest()}
PINS = {}
Q = 1 << 32


def need(condition, reason):
    if not condition:
        raise AssertionError(reason)


def pin(path):
    path = Path(path).resolve(strict=True)
    need(stat.S_ISREG(path.lstat().st_mode), f'not regular:{path}')
    digest = hashlib.sha256()
    count = 0
    with path.open('rb') as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b''):
            digest.update(chunk)
            count += len(chunk)
    value = {'bytes':count, 'sha256':digest.hexdigest()}
    need(str(path) not in PINS or PINS[str(path)] == value, f'changed during audit:{path}')
    PINS[str(path)] = value
    return value


def read(path):
    path = Path(path)
    need(pin(path)['bytes'] <= 268435456, f'oversized input:{path}')
    return path.read_bytes()


def obj(path):
    return json.loads(read(path))


def response(name):
    raw = read(D/(name+'.stdout')).decode()
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return [json.loads(line) for line in raw.splitlines() if line.strip()]


def ratio(value):
    return F(int(value['numerator']),int(value['denominator']))


def wire(value):
    value = F(value)
    return {'numerator':str(value.numerator),'denominator':str(value.denominator)}


def grid(value):
    return F(round(F(value)*Q),Q)


def scale(frame,base):
    a,b = grid(base),grid(2*base)
    progress = grid(F(2*frame+1,160))
    return grid(a+(b-a)*progress)


def sans_revision(value):
    value = copy.deepcopy(value)
    value.pop('revision_id',None)
    return value


def native_tree(path):
    return {str(p.relative_to(path)):pin(p) for p in sorted(path.rglob('*'))
            if stat.S_ISREG(p.lstat().st_mode) and p.name != 'project.sqlite-shm'}


need(not OUT.exists(), 'refusing to overwrite reconciliation')
audit_script = pin(Path(__file__))
inputs = obj(D/'inputs.json')
terminal = obj(D/'terminal.json')
failure_raw = read(D/'failure.json')
failure = json.loads(failure_raw)
wrapper = obj(S/'clipped-media-final.json')
original_script = pin(S/'clipped-media.py')
need(original_script == inputs['script'] == terminal['script_after']
     and original_script['sha256'] == EXPECTED_SCRIPT, 'original script pin')
need(pin(S/'run.py')['sha256'] == wrapper['runner_sha256'], 'original wrapper pin')
need(wrapper['exit_code'] == 1 and terminal['status'] == 'failed'
     and terminal['native_authoritative_equal'] is False, 'original failure must remain a failure')
need(failure['error'] == "AssertionError('persistent native package bytes changed')"
     and 'persistent native package bytes changed' in failure['traceback'], 'unexpected original failure')
need(not (D/'result.json').exists(), 'historical missing result was replaced')
need(wrapper['sources_unchanged'] and wrapper['binaries_unchanged']
     and wrapper['source_before'] == wrapper['source_after']
     and wrapper['binary_before'] == wrapper['binary_after']
     and len(wrapper['source_before']) == 21, 'original frozen21 receipt')
for lane in ['source_before','binary_before','streams']:
    for path,expected in wrapper[lane].items():
        need(pin(path) == expected, f'wrapper input/stream pin:{path}')
need(inputs['binaries'] == terminal['binaries_after'], 'run binary before/after pins')
for path,expected in inputs['binaries'].items():
    need(pin(path) == expected, f'current binary pin:{path}')

native = Path(inputs['native_project']).resolve(strict=True)
old_native,new_native = inputs['native_authoritative_before'],terminal['native_authoritative_after']
delta = {key:{'before':old_native.get(key),'after':new_native.get(key)}
         for key in sorted(set(old_native)|set(new_native))
         if old_native.get(key) != new_native.get(key)}
need(delta == {'project.sqlite-wal':{'before':None,'after':EMPTY}},
     'only absent-to-empty WAL is allowed')
need(native_tree(native) == new_native, 'current persistent source differs from recorded terminal')
need(inputs['volatile_file_excluded'] == 'project.sqlite-shm', 'SHM exclusion changed')

before = response('copy-document-before')
after = response('copy-document-after')
native_before,native_after = response('native-document-before'),response('native-document-after')
need(native_before == native_after == before, 'native authored document changed')
request = obj(D/'set-target-request.json')
revision = request['new_revision']
project = (D/'cli-derived-clipped.deadpan').resolve(strict=True)
portable = (D/'cli-derived-history-portable.deadpan').resolve(strict=True)
cli = Path(next(path for path in inputs['binaries'] if Path(path).name == 'deadpan-cli'))
owners = [(key,node) for key,node in before['nodes'].items()
          if node['kind']['type'] == 'repeat' and (node.get('framing') or {}).get('value',{}).get('type') == 'follow']
need(len(owners) == 1, 'one outer followed Repeat')
owner,repeated = owners[0]
follow = repeated['framing']['value']
fallback = follow['fallback']
need(ratio(fallback['center_x']) == F(311385129,536870912)
     and ratio(fallback['center_y']) == F(1,2)
     and ratio(fallback['scale']) == F(11,10)
     and ratio(follow['scale']) == F(11,10)
     and follow['scale_transition'] == {'factor':wire(2),'curve':'linear'}, 'saved framing')
need(before['nodes'][before['root']]['kind']['children'] == [owner]
     and not before.get('overrides') and not before.get('gap_overrides'), 'owner scope')
target_id = follow['target']
target = before['targets'][target_id]
need(target_id == 'target-1' and len(before['targets']) == 1
     and target['region'] == {'center':[580000,500000]} and not target.get('samples')
     and target['corrections'] == [{'at':12012,'region':{'center':[680000,500000]}}]
     and ratio(target['display_aspect']) == F(16,9), 'initial retained point/correction')
changed = copy.deepcopy(target)
changed['corrections'][0]['region']['center'][0] = 980000
need(request == {'protocol':1,'project_id':before['project_id'],
                 'expected_revision':before['revision_id'],'new_revision':revision,
                 'command':{'command':'set_target','id':target_id,'target':changed}},
     'only the intended typed SetTarget request')
expected = copy.deepcopy(before)
expected['revision_id'] = revision
expected['targets'][target_id] = changed
need(after == expected, 'document changes beyond correction/revision')
gap = repeated['kind']['gap']
context = {'canvases':[{'width':320,'height':180,'fit':'fit','layers':[]}]}
need(gap['duration'] == 4 and gap['audio'] == {'type':'silence'}
     and gap['picture_context'] == context
     and gap['video'] == {'type':'freeze','asset':target['asset'],
                         'timestamp':{'ticks':23023,'time_base':{'numerator':1,'denominator':30000}}},
     'identity captured gap')
need(ratio(repeated['kind']['escalation']['zoom']['step']) == F(1,8)
     and repeated['kind']['escalation']['zoom'].get('target') is None, 'centered escalation')
a,b = response('plan-before'),response('plan-after')
a['plan']['metadata'].pop('revision_id')
b['plan']['metadata'].pop('revision_id')
need(a == b and a['plan']['metadata']['duration'] == 80, 'full plan preservation')
audio_a,audio_b = response('audio-plan-before')['audio'],response('audio-plan-after')['audio']
need(sans_revision(audio_a) == sans_revision(audio_b)
     and audio_a['samples'] == {'start':0,'end':128128}, 'complete audio-plan preservation')

# Reconstruct every expected command, including scope, order and revision args.
expected_commands = [
 ('native-document-before',['project','dump',str(native),'--json']),
 ('copy-native',['project','copy-portable',str(native),str(project)]),
 ('copy-document-before',['project','dump',str(project),'--json']),
 ('plan-before',['inspect-plan',str(project)]),
 ('audio-plan-before',['inspect-plan',str(project),'--audio-samples','0','128128']),
]
expected_commands += [(f'frame-before-{j:03}',['inspect-plan',str(project),'--frame',str(j)]) for j in range(80)]
expected_commands += [
 ('set-target',['command',str(project),'--json',str((D/'set-target-request.json').resolve())]),
 ('copy-document-after',['project','dump',str(project),'--json']),
 ('validate-derived',['project','validate',str(project)]),
 ('plan-after',['inspect-plan',str(project)]),
 ('audio-plan-after',['inspect-plan',str(project),'--audio-samples','0','128128']),
]
expected_commands += [(f'frame-after-{j:03}',['inspect-plan',str(project),'--frame',str(j)]) for j in range(80)]
undone,redone = response('portable-undone-dump'),response('portable-redone-dump')
expected_commands += [
 ('render',['render',str(project),'--output',str(D),'--name','cli-derived-clipped-repeat.mp4','--expected',revision]),
 ('verify',['verify-export',str(project),'--movie',str(D/'cli-derived-clipped-repeat.mp4'),
            '--revision',revision,'--every','1','--samples','0:128128','--report',str(D/'verification.json')]),
 ('copy-derived',['project','copy-portable',str(project),str(portable)]),
 ('portable-dump',['project','dump',str(portable),'--json']),
 ('portable-undo',['project','undo',str(portable),'--expected',revision]),
 ('portable-undone-dump',['project','dump',str(portable),'--json']),
 ('portable-redo',['project','redo',str(portable),'--expected',undone['revision_id']]),
 ('portable-redone-dump',['project','dump',str(portable),'--json']),
 ('portable-validate',['project','validate',str(portable)]),
 ('portable-checkpoint',['project','checkpoint',str(portable)]),
 ('native-document-after',['project','dump',str(native),'--json']),
]
commands = obj(D/'commands.json')
need(len(commands) == terminal['commands'] == len(expected_commands) == 181, 'complete181command trail')
for row,(name,args) in zip(commands,expected_commands):
    need(row['name'] == name and row['argv'] == [str(cli),*args]
         and row['exit_code'] == 0 and row['seconds'] >= 0 and row['started_at'], f'command terminal/scope:{name}')
    for stream in ['stdout','stderr']:
        need(pin(D/(name+'.'+stream)) == row[stream], f'command stream pin:{name}.{stream}')

retained_observations = obj(D/'independent-plan-observations.json')
need(len(retained_observations) == 80, 'retained oracle coverage')
clipped = []
for j in range(80):
    prior,sample = response(f'frame-before-{j:03}')['sample'],response(f'frame-after-{j:03}')['sample']
    play,within = divmod(j,28)
    in_gap = within >= 24
    ordinal = 23 if in_gap else within
    zoom = F(8+play,8)
    projected = F(1,2)+(F(58 if ordinal < 12 else 98,100)-F(1,2))*zoom
    visible = 0 <= projected <= 1
    if not visible:
        clipped.append(j)
    for version,late_percent in [(prior,68),(sample,98)]:
        p = 58 if ordinal < 12 else late_percent
        x = F(1,2)+(F(p,100)-F(1,2))*zoom
        seen = 0 <= x <= 1
        layers = [v for v in version['framing'] if v['instance']['node'] == owner and not v['escalation']]
        need(len(layers) == 1, f'outer layer:{j}')
        layer = layers[0]
        pose = layer['pose']
        need(ratio(layer['local_position']) == F(2*j+1,2)
             and ratio(layer['effective_duration']) == 80, f'owner clock:{j}')
        expected_x = grid(x) if seen else grid(ratio(fallback['center_x']))
        expected_y = F(1,2) if seen else grid(ratio(fallback['center_y']))
        base = ratio(follow['scale']) if seen else ratio(fallback['scale'])
        need(pose == {'center_x':wire(expected_x),'center_y':wire(expected_y),
                      'scale':wire(scale(j,base))}, f'independent Q32pose:{j}')
        esc = [v for v in version['framing'] if v['escalation']]
        need(len(esc) == (0 if play == 0 else 1), f'escalation count:{j}')
        if esc:
            need(esc[0]['instance'] == layer['instance']
                 and esc[0]['pose'] == {'center_x':wire(F(1,2)),'center_y':wire(F(1,2)),
                                       'scale':wire(zoom)}, f'inner centered escalation:{j}')
        need(all(v['pose'] is None for v in version['framing']
                 if v is not layer and not v['escalation']), f'other pose:{j}')
        picture = version['picture']
        need(picture['asset'] == target['asset']
             and picture['point']['time_base'] == {'numerator':1,'denominator':30000}, f'asset/clock:{j}')
        if in_gap:
            need(picture['type'] == 'freeze' and ratio(picture['point']['ticks']) == 23023
                 and version['picture_context'] == context
                 and version['instance'] == {'node':owner,'repeats':[]}
                 and version['gap_after']['ordinal'] == play, f'captured gap:{j}')
        else:
            scopes = version['instance']['repeats']
            need(picture['type'] == 'source'
                 and ratio(picture['point']['ticks']) == F((2*ordinal+1)*1001,2)
                 and version.get('picture_context') is None and version.get('gap_after') is None
                 and len(scopes) == 1 and scopes[0]['node'] == owner
                 and scopes[0]['iteration']['ordinal'] == play, f'provider reset:{j}')
    a,b = sans_revision(prior),sans_revision(sample)
    for value in [a,b]:
        for layer in value['framing']:
            if layer['instance']['node'] == owner and not layer['escalation']:
                layer.pop('pose')
    need(a == b, f'all non-outer sample fields:{j}')
    saved = retained_observations[j]
    actual_pose = next(v['pose'] for v in sample['framing']
                       if v['instance']['node'] == owner and not v['escalation'])
    need(saved == {'Edit':j,'Original':ordinal,'play':play,'gap':in_gap,
                   'source_percent':58 if ordinal < 12 else 98,'escalation':wire(zoom),
                   'projected_before_clip':wire(projected),'visible':visible,
                   'expected_center':actual_pose['center_x'],'actual_pose':actual_pose},
         f'retained independent oracle record:{j}')
need(clipped == list(range(40,56))+list(range(68,80)) and len(clipped) == 28, 'exact28clipped frames')

report = obj(D/'verification.json')
need(report == response('verify'), 'verification stdout/file mismatch')
need(report['passed'] and not report['failures'] and report['revision_id'] == revision
     and report['range'] == [0,80] and report['canvas'] == [320,180]
     and report['frame_rate'] == [30000,1001]
     and report['movie']['video_frames'] == report['movie']['expected_video_frames'] == 80
     and report['movie']['expected_audio_samples'] == 128128, 'export verification terminal')
need(len(report['pictures']) == 80 and report['summary']['pictures_checked'] == 80
     and report['summary']['pictures_failed'] == 0, 'all80pictures')
for j,picture in enumerate(report['pictures']):
    within = j % 28
    ordinal = 23 if within >= 24 else within
    need(picture['project_frame'] == picture['output_frame'] == j and picture['passed']
         and picture['provenance'] == {'kind':'original','asset':target['asset'],
                                      'source_frame':ordinal,'source_pts':ordinal*1001}, f'encoded picture:{j}')
need(len(report['audio']) == 1, 'one complete audio window')
audio = report['audio'][0]
need(audio['window'] == audio['project_samples'] == [0,128128] and audio['passed']
     and audio['uncovered_samples'] == 0 and audio['offset_status'] == 'verified_zero'
     and audio['measured_offset_samples'] == 0, 'full128128 zero-offset audio')
events = response('render')
need(isinstance(events,list) and events[-1]['event'] == 'finished', 'render terminal event')
render_status = events[-1]['status']
receipt = render_status['receipt']
need(render_status['outcome'] == 'published' and render_status['attempt_state'] == 'verified'
     and render_status['captured_revision'] == revision and render_status['cleanup_confirmed'], 'render publication')
movie = D/'cli-derived-clipped-repeat.mp4'
need(pin(movie) == {'bytes':receipt['movie_bytes'],'sha256':receipt['movie_sha256']}, 'published movie pin')
need(pin(receipt['report']) == {'bytes':receipt['report_bytes'],'sha256':receipt['report_sha256']},
     'published render report pin')
need(response('portable-dump') == after
     and sans_revision(undone) == sans_revision(before)
     and sans_revision(redone) == sans_revision(after), 'portable document and Undo/Redo')
need(response('portable-validate')['valid']
     and response('portable-validate')['revision_id'] == redone['revision_id'], 'portable final validation')
for command,expected_count in [('copy-native',20),('copy-derived',21)]:
    copied = response(command)['portable_copy']
    history = copied['verified_history']
    need(history['revisions'] == expected_count
         and history['replayed']+history['verified_by_receipt'] == expected_count, f'portable history:{command}')
for name,description in [('set-target','Set target'),('portable-undo','Undo Set target'),
                         ('portable-redo','Redo Set target')]:
    outcome = response(name)
    need(outcome['committed'] and outcome['outcome']['edit']['description'] == description
         and outcome['outcome']['edit']['duration_delta'] == 0, f'history operation:{name}')
checkpoint = Path(response('portable-checkpoint')['database_checkpoint'])
pin(checkpoint)
# Hash the recovered portable files without opening SQLite or invoking a product.
portable_pins = {str(p.relative_to(portable)):pin(p) for p in sorted(portable.rglob('*'))
                 if stat.S_ISREG(p.lstat().st_mode) and p.name != 'project.sqlite-shm'}

# The audit itself may only create the new top-level reconciliation JSON.
# Every retained input, failure stream, executable and source is rechecked.
for path,expected_pin in list(PINS.items()):
    need(pin(path) == expected_pin, f'input drift:{path}')
result = {
 'schema_version':1,
 'label':'CLI-derived correction clipping qualification, NOT native-authored',
 'reconciliation_status':'passed_with_original_guard_failure_preserved',
 'audit_kind':'read-only retained-evidence reconciliation; no new product/Cargo commands',
 'reconciler':audit_script,'argv':sys.argv,'finished_at':datetime.now(timezone.utc).isoformat(),
 'original_run':{'exit_code':1,'terminal_status':'failed','commands':181,
                 'all_product_commands_exit_zero':True,'failure_verbatim':failure_raw.decode(),
                 'outer_stderr_verbatim':read(S/'clipped-media-final.stderr').decode(),
                 'failure_pin':pin(D/'failure.json'),'wrapper_pin':pin(S/'clipped-media-final.json')},
 'only_allowed_guard_delta':delta,
 'guard_explanation':'Read-only SQLite access created an absent-to-empty project.sqlite-wal. All previously existing persistent source files and authored documents remain exact; SHM was already explicitly excluded.',
 'native_authored_document_equal':True,
 'all_preexisting_native_persistent_files_equal':True,
 'source21_and_all_executable_pins_unchanged':True,
 'recovered_result':{
   'target_only_change':{'at':12012,'from':[680000,500000],'to':[980000,500000]},
   'revision':revision,'saved_fallback_pose':fallback,
   'saved_fallback_center_q32':int(ratio(fallback['center_x'])*Q),
   'frames_checked':80,'clipped_count':28,'clipped_frames':clipped,
   'audio_samples_verified':128128,'audio_offset_samples':0,'audio_plan_equal':True,
   'portable_document_equal':True,'portable_target_undo_redo_equal':True,
   'portable_redone_revision':redone['revision_id'],'movie':pin(movie),
   'render_report':pin(receipt['report']),
 },
 'portable_persistent_file_pins':portable_pins,
 'limitations':['Independent exact Plan oracle; emitted pixel/audio comparison uses the production shared reference.',
                'No separate canonical PCM/gain-bit before/after comparison in the clipping script.',
                'Original run remains exit1; reconciliation does not rewrite its receipt or create a historical result.json.',
                'SQLite SHM remains explicitly excluded; only the exact empty-WAL creation is newly reconciled.'],
 'verified_input_pins':PINS,
}
OUT.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'status':result['reconciliation_status'],'path':str(OUT),
                  'bytes':OUT.stat().st_size,'sha256':hashlib.sha256(OUT.read_bytes()).hexdigest(),
                  'commands_verified':181,'frames':80,'clipped':28,'audio_samples':128128}))
