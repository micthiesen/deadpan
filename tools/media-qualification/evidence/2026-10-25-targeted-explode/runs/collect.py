#!/usr/bin/env python3
"""Append hash-verified, compact targeted-explode qualification evidence.

Run roots are explicit --tree NAME=PATH arguments supplied only after the
owner confirms a run is terminal. Active receipts are never copied. This script
does not build, invoke tests, or modify product sources.
"""
from pathlib import Path
import argparse, gzip, hashlib, json, platform, sys, time

SCRATCH = Path('/tmp/deadpan-targeted-explode-2026-10-25')
DEST = Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-25-targeted-explode')

def sha(b): return hashlib.sha256(b).hexdigest()

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--tree', action='append', default=[], help='NAME=PATH; only terminal, owner-confirmed run roots')
    ap.add_argument('--file', action='append', default=[], help='NAME=PATH for one settled report/log to retain')
    ap.add_argument('--include-sixth', action='store_true', help='explicitly include check-sixth receipt after owner confirms it is terminal')
    ap.add_argument('--screenshots', action='append', default=[], help='individual final PNG paths to retain')
    ap.add_argument('--exclude-png', action='store_true', help='hash/size-ledger PNGs instead of retaining an unselected screenshot set')
    ap.add_argument('--freeze', action='store_true', help='freeze only after owner confirms all runs are complete')
    ap.add_argument('--status', default='mutable collection; final proof runs pending')
    args = ap.parse_args()
    DEST.mkdir(parents=True, exist_ok=True)
    index = DEST / 'inventory.json'
    inv = json.loads(index.read_text()) if index.exists() else {
        'format':'deadpan-targeted-explode-evidence-v1','files':[],'omissions':[],'metadata':{},'counts':{}}
    entries = {e['path']:e for e in inv['files']}
    omissions = {e['source']:e for e in inv['omissions']}
    selected_shots = {str(Path(s)) for s in args.screenshots}
    for s in selected_shots:
        omissions.pop(s, None)

    def add(src, rel, compress=None):
        src=Path(src)
        if not src.is_file(): return
        raw=src.read_bytes()
        if compress is None: compress=src.suffix.lower() in {'.json','.jsonl','.txt','.log','.html','.stdout','.stderr'}
        outrel=rel+('.gz' if compress and not rel.endswith('.gz') else '')
        stored=gzip.compress(raw,compresslevel=9,mtime=0) if compress else raw
        out=DEST/outrel;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(stored)
        copied=out.read_bytes();decoded=gzip.decompress(copied) if compress else copied
        if decoded!=raw: raise RuntimeError('copy mismatch: '+outrel)
        entries[outrel]={'source':str(src),'path':outrel,'source_bytes':len(raw),'source_sha256':sha(raw),
                         'stored_bytes':len(stored),'stored_sha256':sha(stored),
                         'compression':'gzip' if compress else 'none','validated':True}

    def omit(src, reason):
        src=Path(src); b=src.read_bytes()
        omissions[str(src)]={'source':str(src),'reason':reason,'bytes':len(b),'sha256':sha(b)}

    # Settled qualification, recovery, and infrastructure records. The sixth
    # receipt is excluded until the caller supplies explicit owner confirmation.
    stable=['cache-prune.json','check-fifth-recovery.json','check-fifth.json',
            'check-first.json','check-first.stdout','check-first.stderr',
            'check-second.json','check-second.stdout','check-second.stderr',
            'check-third.json','check-third.stdout','check-third.stderr',
            'check-fourth.json','check-fourth.stdout','check-fourth.stderr',
            'check-fifth.stdout','check-fifth.stderr',
            'check-ime-continuation.json','check-ime-continuation.stdout','check-ime-continuation.stderr',
            'check-ui.json','check-ui.stdout','check-ui.stderr',
            'check-final.json','check-final.stdout','check-final.stderr',
            'check-final-third.json','check-final-third.stdout','check-final-third.stderr',
            'check-final-fourth.json','check-final-fourth.stdout','check-final-fourth.stderr',
            'check-final-fifth.json','check-final-fifth.stdout','check-final-fifth.stderr',
            'media-first.json','media-first.stdout','media-first.stderr',
            'media-second.json','media-second.stdout','media-second.stderr',
            'native-first.json','native-first.stdout','native-first.stderr',
            'release-build.json','release-build.stdout','release-build.stderr','release-binaries.json',
            'native-second.json','native-second.stdout','native-second.stderr',
            'native-second-retained.json','native-second-retained.stdout','native-second-retained.stderr',
            'native-third.json','native-third.stdout','native-third.stderr',
            'native-fourth.json','native-fourth.stdout','native-fourth.stderr',
            'native-fifth.json','native-fifth.stdout','native-fifth.stderr',
            'native-final.json','native-final.stdout','native-final.stderr',
            'release-build-second.json','release-build-second.stdout','release-build-second.stderr',
            'release-build-third.json','release-build-third.stdout','release-build-third.stderr','release-third-binaries.json',
            'release-build-fourth.json','release-build-fourth.stdout','release-build-fourth.stderr','release-fourth-binaries.json',
            'release-build-fifth.json','release-build-fifth.stdout','release-build-fifth.stderr','release-fifth-binaries.json',
            'release-build-sixth.json','release-build-sixth.stdout','release-build-sixth.stderr','qualified-binaries.json',
            'release-build-seventh.json','release-build-seventh.stdout','release-build-seventh.stderr','final-qualified-binaries.json',
            'native-final-v2.json','native-final-v2.stdout','native-final-v2.stderr',
            'native-final-quiet.json','native-final-quiet.stdout','native-final-quiet.stderr',
            'check-final-second.json','check-final-second.stdout','check-final-second.stderr',
            'release-final-binaries.json',
            'fixture-patch.json','removed-files.json','run-before-incremental-control.py',
            'run.py','source-files.json','targeted_explode_export.rs',
            'targeted_explode_native.rs','targeted_explode_recipe.rs','refine_native.py']
    if args.include_sixth:
        stable += ['check-sixth.json','check-sixth.stdout','check-sixth.stderr']
    for name in stable:
        p=SCRATCH/name
        if p.exists(): add(p,'runs/'+name)

    for spec in args.tree:
        if '=' not in spec: raise ValueError('--tree must be NAME=PATH')
        name,path=spec.split('=',1); root=Path(path)
        if not root.is_dir(): raise FileNotFoundError(root)
        for p in sorted(root.rglob('*')):
            if not p.is_file(): continue
            n=p.name.lower()
            if n.endswith('.lock') or n.endswith('-shm') or n.endswith('.shm') or (n.endswith('-wal') and p.stat().st_size==0):
                omit(p,'lock_or_sqlite_sidecar_omitted'); continue
            parts=set(p.parts)
            if 'target' in parts or 'incremental' in parts or p.suffix.lower() in {'.dylib','.so','.a','.o','.rlib','.d'}:
                omit(p,'binary_or_build_cache_omitted'); continue
            if args.exclude_png and p.suffix.lower()=='.png' and str(p) not in selected_shots:
                omit(p,'unselected_screenshot_bytes_omitted_pending_root_review'); continue
            if p.suffix.lower() in {'.app','.framework'}: continue
            add(p,f'{name}/{p.relative_to(root).as_posix()}')

    for spec in args.file:
        if '=' not in spec: raise ValueError('--file must be NAME=PATH')
        name,path=spec.split('=',1)
        add(Path(path),name)

    add(Path(__file__), 'runs/collect.py')

    for shot in args.screenshots:
        p=Path(shot)
        if p.is_file(): add(p,'ui/screenshots/'+p.name,False)

    metadata={
      'collector_script':str(Path(__file__)),
      'collector_sha256':sha(Path(__file__).read_bytes()),
      'collector_bytes':Path(__file__).stat().st_size,
      'updated_at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),
      'python':sys.version,'platform':platform.platform(),
      'scratch_root':str(SCRATCH),'destination':str(DEST),
      'status':args.status,'terminal_trees':args.tree,'terminal_files':args.file,
      'sixth_receipt_included':args.include_sixth,
      'unselected_png_bytes_omitted':args.exclude_png,
      'freeze_requested':args.freeze}
    readme='''# Targeted Explode qualification evidence (2026-10-25)\n\nFinal evidence is complete; collection remains mutable until the owner freezes it. This bundle preserves initial fixture-compilation failures, Cargo disk-full failure and recovery, native-check failures, successful IME-continuation/UI checks, both media runs, and all four generated export roots. Check-fifth's start receipt and raw stdout/stderr are retained with recovery and cache-prune records; recovery confirms the source map still matched the fifth-run start. The sixth run exited 1: 323 of 324 filtered tests passed, while `follow_center_sync_preserves_unfinished_scale_and_arrival_composition` failed. A later check-ime-continuation run exited 0; its 2203-file source map was unchanged, strict normal/UI target lint and format passed, and the normal filtered test lane ran one passing test with 4390 skipped. Check-ui exited 0 with 44 passed and 1339 skipped, including UI unit execution. Check-final through check-final-fifth strict no-test checks exited 0.\n\nThe first native visual replay failed because the harness advanced 80 frames after the scope had already retained Edit frame 243 (Original 119), past the targeted play's Original span `[0, 110)` and Edit terminal 234. The corrected child Camera draft/reprojection case passed in native-second, but the full-document comparison initially failed while all 368 picture/pose/audio observations matched the typed oracle. The retained-history rerun failed the same canonical comparison. In the retained package, read-only SQLite opens with WAL, returns `PRAGMA quick_check=ok`, and reads history row 67: outer revision `c678df92-b263-4503-9221-a8b48f6f492a` contains a compound Explode step with leaf revision/timing allocation `66d93d0c-9ea9-4bad-87db-fa9026a42346`. The raw database, nonempty WAL, manifest, generated Original and compact parsed request witness are retained. The semantic canonical comparison fix was independently reviewed and native-third passed that full-document check. Its 200.5% decimal witness failed because the scale control lost focus when the harness Tabbed through Apply then Cancel; the observed scale stayed 200%. Native-fourth passed canonical, child Camera, 210%, 200.5% and five-ignored-edit checks, but its policy hint was clipped. Native-fifth passed all 578 zoom checks plus Kestrel, but its help copy was superseded by a compact update adding arrival/curve guidance. Native-final exposed a 1.4375 px Cancel clipping at 960x640; the caption was shortened. Final native-final-v2 visual run exited 0 with 580 zoom checks and one Kestrel check, and final quiet-120 exited 0 with the same checks. Both report unchanged 2203-file sources and unchanged executable. The five retained final screenshots are zoom-141 through zoom-145; every other screenshot is hash/size ledgered as omitted. Terminal release-build-fourth through release-build-seventh exited 0 with unchanged sources and binaries; check-final-third through check-final-fifth exited 0. Compared with media-second, the final release source map has the same 2203 paths: 2200 identical and exactly three app-only changes in camera.rs, camera_fields.rs and the native fixture. Compared with native-final, only camera_fields.rs differs. Exact byte counts and hashes are in the source-map delta file. This compares file hashes, not dependency closure.\n\nThe first media run exited 101: Add passed, while the Multiply static-center control expected to mismatch did not because outer Follow canceled the inner translation at zoom-out. The second media run exited 0 with both tests passing. Add's static-center negative fails as intended. Multiply's static-center control matches as expected for compensated translation, while omitting the second Follow layer produces the visible negative mismatch. The earlier failure is retained as historical evidence, not relabeled as a pass.\n\nThe export witnesses use generated counter footage and authored target geometry. Passing positive cases compare 105 emitted pictures and 168,168 canonical stereo sample frames, verify zero audio offset, preserve before/after media identity, and compare authored, limited and limiter-gain PCM bits. They do not establish general tracking accuracy, independent DSP accuracy, or listening quality. The center-preserved case reports a zero minimum neighbor margin, retained as-is.\n\nThe collector validates source and stored SHA-256 hashes after copying. Repetitive JSON/log/text is gzip-compressed losslessly. Lock files, SQLite SHM and empty WAL sidecars are omitted with size/hash entries; nonempty WAL is retained. Binaries, build caches, downloaded weights and full source archives are excluded.\n'''
    (DEST/'README.md').write_text(readme)
    (DEST/'collector.json').write_text(json.dumps(metadata,sort_keys=True,indent=2)+'\n')
    inv['files']=sorted(entries.values(),key=lambda e:e['path'])
    inv['omissions']=sorted(omissions.values(),key=lambda e:e['source'])
    inv['metadata']={n:{'bytes':(DEST/n).stat().st_size,'sha256':sha((DEST/n).read_bytes())} for n in ['README.md','collector.json']}
    inv['counts']={'files':len(inv['files']),'omissions':len(inv['omissions']),
                   'source_bytes':sum(e['source_bytes'] for e in inv['files']),
                   'stored_bytes':sum(e['stored_bytes'] for e in inv['files'])}
    index.write_text(json.dumps(inv,sort_keys=True,indent=2)+'\n')
    for e in inv['files']:
        b=(DEST/e['path']).read_bytes()
        if len(b)!=e['stored_bytes'] or sha(b)!=e['stored_sha256']: raise RuntimeError('stored integrity: '+e['path'])
        raw=gzip.decompress(b) if e['compression']=='gzip' else b
        if len(raw)!=e['source_bytes'] or sha(raw)!=e['source_sha256']: raise RuntimeError('source integrity: '+e['path'])
    for n,v in inv['metadata'].items():
        b=(DEST/n).read_bytes()
        if len(b)!=v['bytes'] or sha(b)!=v['sha256']: raise RuntimeError('metadata integrity: '+n)
    for o in inv['omissions']:
        p=Path(o['source'])
        if p.exists():
            b=p.read_bytes()
            if len(b)!=o['bytes'] or sha(b)!=o['sha256']: raise RuntimeError('omission ledger integrity: '+o['source'])
    if args.freeze:
        for p in sorted(DEST.rglob('*'),reverse=True):
            if p.is_file(): p.chmod(0o444)
            elif p.is_dir(): p.chmod(0o555)
        DEST.chmod(0o555)
    print(json.dumps({'destination':str(DEST),'counts':inv['counts'],'status':args.status,'freeze_requested':args.freeze},sort_keys=True))

if __name__=='__main__': main()
