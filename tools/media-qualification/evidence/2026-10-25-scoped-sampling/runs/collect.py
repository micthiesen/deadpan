from pathlib import Path
import argparse, gzip, hashlib, json, platform, sys, time

SCRATCH=Path('/tmp/deadpan-scoped-sampling-2026-10-25')
DEST=Path('/Users/michael/Code/deadpan/tools/media-qualification/evidence/2026-10-25-scoped-sampling')
sha=lambda b:hashlib.sha256(b).hexdigest()

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--file',action='append',default=[],help='DEST_REL=SOURCE for settled individual evidence')
    ap.add_argument('--tree',action='append',default=[],help='DEST_PREFIX=SOURCE terminal tree, owner-confirmed only')
    ap.add_argument('--screenshot',action='append',default=[],help='retain one selected screenshot path')
    ap.add_argument('--status',default='mutable; final runs pending')
    ap.add_argument('--freeze',action='store_true')
    a=ap.parse_args()
    DEST.mkdir(parents=True,exist_ok=True)
    invp=DEST/'inventory.json'
    inv=json.loads(invp.read_text()) if invp.exists() else {'files':[],'omissions':[],'counts':{}}
    entries={x['path']:x for x in inv['files']}
    omissions={x['source']:x for x in inv['omissions']}
    selected_shots={str(Path(s)) for s in a.screenshot}
    for s in selected_shots: omissions.pop(s,None)
    def add(src,rel):
        src=Path(src)
        if not src.is_file(): return
        raw=src.read_bytes()
        comp=src.suffix.lower() in {'.json','.jsonl','.txt','.log','.stdout','.stderr','.html','.yuv','.pcm','.bin','.raw'}
        outrel=rel+('.gz' if comp and not rel.endswith('.gz') else '')
        stored=gzip.compress(raw,compresslevel=9,mtime=0) if comp else raw
        for oldrel,e in list(entries.items()):
            if e['source']==str(src) and oldrel!=outrel:
                oldpath=DEST/oldrel
                if oldpath.exists(): oldpath.unlink()
                del entries[oldrel]
        out=DEST/outrel;out.parent.mkdir(parents=True,exist_ok=True);out.write_bytes(stored)
        actual=out.read_bytes();decoded=gzip.decompress(actual) if comp else actual
        if decoded!=raw: raise RuntimeError('copy mismatch '+outrel)
        entries[outrel]={'source':str(src),'path':outrel,'source_bytes':len(raw),'source_sha256':sha(raw),'stored_bytes':len(stored),'stored_sha256':sha(stored),'compression':'gzip' if comp else 'none','validated':True}
    def omit(src,reason):
        src=Path(src);b=src.read_bytes()
        omissions[str(src)]={'source':str(src),'reason':reason,'bytes':len(b),'sha256':sha(b)}
    for spec in a.file:
        rel,src=spec.split('=',1);add(src,rel)
    for spec in a.tree:
        rel,src=spec.split('=',1);root=Path(src)
        if not root.is_dir(): raise FileNotFoundError(root)
        for p in sorted(root.rglob('*')):
            if not p.is_file(): continue
            n=p.name.lower()
            if n.endswith('.lock') or n.endswith('-shm') or n.endswith('.shm') or (n.endswith('-wal') and p.stat().st_size==0):
                omit(p,'lock_or_sqlite_sidecar_omitted');continue
            if 'target' in p.parts or 'incremental' in p.parts or p.suffix.lower() in {'.dylib','.so','.a','.o','.rlib','.d'} or p.name == 'media-helper' or (p.stat().st_mode & 0o111):
                omit(p,'build_cache_or_binary_omitted');continue
            if p.suffix.lower()=='.png':
                if str(p) in selected_shots:
                    add(p,'ui/screenshots/'+p.name)
                else: omit(p,'screenshot_not_selected_yet')
                continue
            add(p,rel+'/'+p.relative_to(root).as_posix())
    for s in selected_shots: add(s,'ui/screenshots/'+Path(s).name)
    meta={'collector':str(Path(__file__)),'collector_sha256':sha(Path(__file__).read_bytes()),'collector_bytes':Path(__file__).stat().st_size,'updated_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'python':sys.version,'platform':platform.platform(),'status':a.status,'files':a.file,'trees':a.tree,'screenshots':a.screenshot,'freeze_requested':a.freeze}
    readme='''# Scoped Follow target sampling qualification (2026-10-25)

Mutable evidence collection. The initial UI check failed to compile because the fixture returned Arc<Workspace> while declaring Workspace; both mismatched annotations were fixed before check-second. Check-second is terminal and exited 1 after strict normal/UI target lint passed. Of six new unit tests, four passed and two failed because the fixture oracle used root start rather than the actual picture center: expected ordinary center 0.25 but used 0.2; expected retimed provider point 16/11 and target units 372727 but used 8/11 and 336364. The runtime center is 0.25. Preserve this failure; corrected expected math and explicit provider-point assertions are pending.

Check-third exited 0: strict normal/UI lint and format passed, all six targeted unit tests passed, and the filtered normal lane reported 1333 skipped. Its source map contains 2206 files. Debug build-first exited 0 and built all App UI targets; its manifest records the App binary (112,621,440 bytes, SHA-256 5e316bacc2f34c00e7dce208869897a8d25e76b5b87c91a16da42f609846103f). The following selected UI test and retained native replay were active when this note was last updated and are not yet treated as terminal evidence.

The selected UI suite then passed in two terminal runs: 226 tests in unit-ui-first and 7 in unit-ui-second. Native-debug-first failed its exact 11/5 suffix oracle and was corrected. Native-debug-second still failed because full-owner Time selection clamped the endpoint to 115 instead of the expected exclusive 116. Both failed native output trees and receipts are retained as counterevidence; native endpoint navigation is still being investigated. Debug-build-second exited 0. No final media or release proof is claimed yet.

The first commands-generation run intentionally exercised the generator rewrite panic and failed with its documented request to rerun without the environment override; its raw streams and receipt are retained. Check-fourth exited 0 with 111 normal tests, format, and strict normal/UI lint. Replacing a literal h in help with the configured frame.previous binding exposed a commands-reference test failure; commands-generation-second intentionally rewrote the reference. Unit-ui-third reported 240 passing tests with one LEAK and that reference failure. Unit-ui-continuation passed both the commands-reference test and the isolated formerly leaky audio-setters test without a leak. Debug-build-third passed; native-debug-third passed all 530 scoped checks plus Kestrel, including endpoint selection and detected-target composition. Root inspected and selected scoped-follow-creep screenshots 128 and 129 at 960x640. The full-unit leak report and prior command-reference failure remain preserved.

Check-fifth exited 0 and release-build-first exited 0 with unchanged sources. Release visual-first passed 530 scoped checks plus Kestrel; release quiet-first passed 529 plus Kestrel on the same App binary (SHA-256 d17c384dabadcae98e78d0296923e0a15394fa5172e963cf3c7ea29339ff94ba). The final selected screenshots 128 and 129 were inspected at 960x640; their bytes match the debug captures. These visual runs do not finish the media proof.

Media-first exited 1 after original-node comparison, clone isomorphism and complete helper PCM comparisons passed. Its driver's unchanged-document assertion rejected the clone's legitimate audio_lineage/audio_bindings additions. The full failed media tree is retained, including the retained project and nonempty WAL. The helper was compiled from the successful Cargo artifact stream (build receipt retained); the executable itself is excluded and hash/size ledgered. The metadata-oracle fix is now frozen and has a positive fixture plus 14 tamper rejections in retained test/control sources; independent review and the terminal media-second run remain pending. No passing media claim is made yet. The old media script source pin used by media-first and the corrected script are retained separately when both copies are available.

This increment adds sampled target ranges on top of the earlier Follow creep behavior. The media helper sources and contract are retained but have not yet been executed. Final qualification still requires actual retained native project/history evidence, all 240 pictures and 384,384 canonical PCM frames, before/after and sampled exports, preserved authored state, plus final release visual-60 and quiet-120 runs on the same binary and source pin with selected inspected screenshots. The native witness is also being extended with a scoped Camera face proposal, explicit Save/Escape behavior, range recreation and composed named-Creep sampling to resolve the strict DP-08 scope audit; those outcomes are pending.

Generated fixtures only. This evidence does not establish physical interaction, independent DSP correctness, tracking accuracy, or listening quality. No binaries, build caches, model weights, or full source archives are retained. JSON/log/text and generated raw YUV/PCM/reference blobs are gzip-compressed losslessly; MP4 and PNG outputs remain in their native already-compressed form. Lock/SHM and empty WAL sidecars are hash/size ledgered; a nonempty WAL is retained.
'''
    (DEST/'README.md').write_text(readme)
    (DEST/'collector.json').write_text(json.dumps(meta,sort_keys=True,indent=2)+'\n')
    inv['files']=sorted(entries.values(),key=lambda e:e['path'])
    inv['omissions']=sorted(omissions.values(),key=lambda e:e['source'])
    inv['metadata']={n:{'bytes':(DEST/n).stat().st_size,'sha256':sha((DEST/n).read_bytes())} for n in ('README.md','collector.json')}
    inv['counts']={'files':len(inv['files']),'omissions':len(inv['omissions']),'source_bytes':sum(x['source_bytes'] for x in inv['files']),'stored_bytes':sum(x['stored_bytes'] for x in inv['files'])}
    invp.write_text(json.dumps(inv,sort_keys=True,indent=2)+'\n')
    for e in inv['files']:
        b=(DEST/e['path']).read_bytes()
        if len(b)!=e['stored_bytes'] or sha(b)!=e['stored_sha256']:raise RuntimeError('stored mismatch '+e['path'])
        raw=gzip.decompress(b) if e['compression']=='gzip' else b
        if len(raw)!=e['source_bytes'] or sha(raw)!=e['source_sha256']:raise RuntimeError('source mismatch '+e['path'])
    for e in inv['omissions']:
        p=Path(e['source'])
        if p.exists():
            b=p.read_bytes()
            if len(b)!=e['bytes'] or sha(b)!=e['sha256']:raise RuntimeError('omission mismatch '+str(p))
    if a.freeze:
        for p in sorted(DEST.rglob('*'),reverse=True):
            p.chmod(0o444 if p.is_file() else 0o555)
        DEST.chmod(0o555)
    print(json.dumps({'destination':str(DEST),'counts':inv['counts'],'status':a.status,'freeze_requested':a.freeze},sort_keys=True))
if __name__=='__main__':main()
