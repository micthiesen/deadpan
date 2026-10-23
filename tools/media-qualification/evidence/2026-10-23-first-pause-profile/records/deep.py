from pathlib import Path
import os,json,hashlib,subprocess,time,shutil
s=Path('/tmp/deadpan-first-pause-profile-2026-10-22')
old=s/'instrumented-corrected';root=s/'deep-instrumented-corrected';target=s/'deep-target'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(p):
 names=subprocess.check_output(['rg','--files','-g','*.rs'],cwd=p,text=True).splitlines()
 return {n:sha(p/n)for n in names}
assert snapshot(old)==json.loads((s/'instrumented-identities.json').read_text())
shutil.copytree(old,root)
path=root/'crates/deadpan-store/src/lib.rs'
def patch(before,after):
 text=path.read_text();assert text.count(before)==1,(before[:80],text.count(before));path.write_text(text.replace(before,after,1))
def patch_write(before,after):
 text=path.read_text();start=text.index('fn write_command_plan(');end=text.index('/// Core edits describe authored intent.',start)
 body=text[start:end];assert body.count(before)==1,(before[:80],body.count(before))
 path.write_text(text[:start]+body.replace(before,after,1)+text[end:])
patch('    ensure_unused_revision(connection, documents, &request.new_revision)?;\n    let captured = slice_capture_revision(connection, request)?;',
 '    let _dp_admission = std::time::Instant::now();\n    ensure_unused_revision(connection, documents, &request.new_revision)?;\n    let captured = slice_capture_revision(connection, request)?;')
patch('    command_plan(Arc::clone(current.document()), next, edit, request, None)\n',
 '    eprintln!("DP24 store_transition_admission_ms={:.3}", _dp_admission.elapsed().as_secs_f64()*1000.0);\n    command_plan(Arc::clone(current.document()), next, edit, request, None)\n')
patch('    let request_json = serde_json::to_string(request)?;\n',
 '    let _dp_request_wire = std::time::Instant::now();\n    let request_json = serde_json::to_string(request)?;\n')
patch('    let edit_json = serde_json::to_string(&edit)?;\n',
 '    eprintln!("DP24 store_request_wire_ms={:.3}", _dp_request_wire.elapsed().as_secs_f64()*1000.0);\n    let _dp_edit_wire = std::time::Instant::now();\n    let edit_json = serde_json::to_string(&edit)?;\n')
patch('    check_document_size(&edit_json)?;\n',
 '    check_document_size(&edit_json)?;\n    eprintln!("DP24 store_edit_serialize_ms={:.3} edit_bytes={}", _dp_edit_wire.elapsed().as_secs_f64()*1000.0, edit_json.len());\n    let _dp_edit_decode = std::time::Instant::now();\n')
patch('    if !edit.inverse.restores(&next, &current)? {\n',
 '    eprintln!("DP24 store_edit_roundtrip_ms={:.3}", _dp_edit_decode.elapsed().as_secs_f64()*1000.0);\n    let _dp_inverse = std::time::Instant::now();\n    if !edit.inverse.restores(&next, &current)? {\n')
patch('    // The adopted result must be exactly what history replay reconstructs\n',
 '    eprintln!("DP24 store_inverse_restore_ms={:.3}", _dp_inverse.elapsed().as_secs_f64()*1000.0);\n    // The adopted result must be exactly what history replay reconstructs\n')
patch_write('    compound::require_authored(&plan)?;\n',
 '    let _dp_writer_checks = std::time::Instant::now();\n    compound::require_authored(&plan)?;\n')
patch_write('    source_registration::check_revision_assets(connection, &plan.current, &plan.next)?;\n    insert_revision(\n',
 '    source_registration::check_revision_assets(connection, &plan.current, &plan.next)?;\n    eprintln!("DP24 store_writer_checks_ms={:.3}", _dp_writer_checks.elapsed().as_secs_f64()*1000.0);\n    let _dp_revision = std::time::Instant::now();\n    insert_revision(\n')
patch_write('    let register_bank = match &plan.compound {\n',
 '    eprintln!("DP24 store_insert_revision_ms={:.3}", _dp_revision.elapsed().as_secs_f64()*1000.0);\n    let _dp_history = std::time::Instant::now();\n    let register_bank = match &plan.compound {\n')
patch_write('    let history_id = connection.last_insert_rowid();\n',
 '    eprintln!("DP24 store_history_write_ms={:.3}", _dp_history.elapsed().as_secs_f64()*1000.0);\n    let _dp_post_history = std::time::Instant::now();\n    let history_id = connection.last_insert_rowid();\n')
patch_write('    documents.set_receipt_link(audit::extend(\n',
 '    eprintln!("DP24 store_post_history_ms={:.3}", _dp_post_history.elapsed().as_secs_f64()*1000.0);\n    let _dp_receipt = std::time::Instant::now();\n    documents.set_receipt_link(audit::extend(\n')
patch_write('    Ok((\n        CommitOutcome {\n',
 '    eprintln!("DP24 store_receipt_extend_ms={:.3}", _dp_receipt.elapsed().as_secs_f64()*1000.0);\n    Ok((\n        CommitOutcome {\n')
identities=snapshot(root)
assert set(identities)==set(json.loads((s/'instrumented-identities.json').read_text()))
(s/'deep-source-identities.json').write_text(json.dumps(identities,indent=2)+'\n')
overlay=s/'deep-store-overlay.rs.txt';shutil.copy2(path,overlay)
env=dict(os.environ,CARGO_TARGET_DIR=str(target),DEADPAN_FFMPEG_PREFIX='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix')
cmd=['cargo','build','--locked','--release','-p','deadpan-cli','--example','perf'];started=time.monotonic()
with(s/'deep-build.stdout').open('w')as out,(s/'deep-build.stderr').open('w')as err:r=subprocess.run(cmd,cwd=root,env=env,stdout=out,stderr=err)
receipt={'command':cmd,'cwd':str(root),'target_dir':str(target),'exit':r.returncode,'seconds':time.monotonic()-started,'script_sha256':sha(s/'deep.py'),'overlay_sha256':sha(overlay)}
(s/'deep-build.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
if r.returncode:raise SystemExit(r.returncode)
assert snapshot(root)==identities
binary=s/'deep-perf';shutil.copy2(target/'release/examples/perf',binary)
binaries=json.loads((s/'binaries.json').read_text());binaries['instrumented']={'source_path':str(target/'release/examples/perf'),'executed_path':str(binary),'sha256':sha(binary)}
(s/'deep-binaries.json').write_text(json.dumps(binaries,indent=2)+'\n')
code=(s/'measure.py').read_text().replace("out=s/'controlled'","out=s/'deep-controlled'").replace("s/'binaries.json'","s/'deep-binaries.json'")
extra=['store_transition_admission_ms','store_request_wire_ms','store_edit_serialize_ms','store_edit_roundtrip_ms','store_inverse_restore_ms','store_writer_checks_ms','store_insert_revision_ms','store_history_write_ms','store_post_history_ms','store_receipt_extend_ms']
code=code.replace("for i in range(5):",'keys += '+repr(extra)+'\nfor i in range(5):')
(s/'deep-measure.py').write_text(code)
with(s/'deep-driver.log').open('w')as log:r=subprocess.run(['python',str(s/'deep-measure.py')],stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0,r.returncode
assert sha(binary)==binaries['instrumented']['sha256'] and snapshot(root)==identities
print('Deep controlled batch passed source/binary guards',flush=True)
