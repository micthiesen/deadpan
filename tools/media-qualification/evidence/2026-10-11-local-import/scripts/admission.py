from accept import *
if __name__=='__main__':
    # Continue on the portable copy; the original scenario remains unchanged.
    P=R/'Portable.deadpan'
    before=run('admission-before','project','dump',P,'--json')
    content=json.loads((R/'retain.log').read_text())['retained_original']['record']['object']['content']
    reg={'protocol':1,'registration':{'expected_revision':before['revision_id'],'new_revision':'duplicate-sound-registration','original':content,'new_asset_id':'different-suggested-alias','label':'Another name','insertion':None},'streams':{'type':'audio_only','stream':0,'interpretation':'stereo_left_right'}}
    dedup=run('dedup','project','register-source',P,'--request-json',write('dedup',reg))
    assert dedup['committed'] is False,dedup
    assert run('dedup-dump','project','dump',P,'--json')==before
    # Audio stream 1 of a video container must register only sound, even though
    # the same complete container also owns this project's Original picture.
    original=json.loads((R/'create.log').read_text())['created']['content'].split(':')[1]
    reg['registration'].update(new_revision='container-sound-registration',original={'algorithm':'blake3','digest':original},new_asset_id='container-sound',label='Container audio only')
    reg['streams']={'type':'audio_only','stream':0}
    run('wrong-video-stream','project','register-source',P,'--request-json',write('wrong-video-stream',reg),exit=1)
    assert run('wrong-stream-dump','project','dump',P,'--json')==before
    reg['streams']['stream']=1
    run('container-sound','project','register-source',P,'--request-json',write('container-sound',reg))
    after=run('container-dump','project','dump',P,'--json')
    for key in ['nodes','root','presentation_basis','basis_state']:assert after[key]==before[key],key
    assert after['assets']['container-sound']['video'] is None
    assert after['assets']['container-sound']['audio'] is not None
    assert len(after['assets'])==len(before['assets'])+1
    # Sound catalog changes must not alter the rendered picture or mix.
    run('catalog-export-verify','verify-export',P,'--movie',R/'output/portable.mp4','--every','1','--samples','16016:24213,48000:56192,180000:188192','--report',R/'catalog-verification.json')
    run('admission-validate','project','validate',P)
    (R/'admission-result.json').write_text(json.dumps({'status':'pass','checks':['same content/stream reuses catalog asset despite different proposed alias and label','video stream selected as audio fails without authored changes','explicit container audio stream adds sound only','import preserves all nodes, timing, Original, basis and placed mix','unchanged movie verifies against the new catalog revision']},indent=2)+'\n')
