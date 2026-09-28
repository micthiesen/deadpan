# Current FFmpeg build admission investigation

Read-only inspection on 2026-09-28. No network, rebuild, codec execution, signature-verifier execution, or tests. `otool` read Mach-O metadata only. These are current file comparisons against retained build evidence, not an independently reproduced build.

## Exact build and source records

- Current receipt: `/tmp/deadpan-ui-ffmpeg-build.json`, 121710 bytes, SHA-256 `baf7437c0cf4a61c94d75efd92db50b5ed3a96b2fa8f1e891284bdcf7c3d78c7`.
- Receipt schema 1, result `passed`; started `2026-09-26T20:19:41.806718+00:00`, finished `2026-09-26T20:25:51.495061+00:00`.
- Work directory `/private/tmp/deadpan-ui-ffmpeg`, installed prefix `/private/tmp/deadpan-ui-ffmpeg/prefix`, source `/private/tmp/deadpan-ui-ffmpeg/ffmpeg-8.0.3`. `/tmp` resolves to `/private/tmp` here.
- Builder: `tools/media-qualification/compatible/build.py`, SHA-256 `2d3a5559da517ae67962b4adfa74123b92402d8b86dee82af214daeee57bac44`.
- Pins: `tools/media-qualification/compatible/pins.json`, SHA-256 `538a1aaba16fe36d9d41a5ba0052be6d19e74c383800349c4a1dfa12ea05edfa`.
- Both current repository files match the receipt's source hashes; the full current pins object matches the receipt.
- Archive `/tmp/deadpan-ui-ffmpeg/downloads/ffmpeg-8.0.3.tar.xz`, SHA-256 `6136812ea6d4e68bdba27e33c2a94382711cdf4f8602ffef056ff792bd6f9818`, matches the pin.
- Detached signature `.tar.xz.asc`, SHA-256 `975f9512458fc39cacf35a9496f51c9d82e3a3b684a9db111bedcfa523f2b2b8`, matches the pin.
- Release key `ffmpeg-devel.asc`, SHA-256 `397b3becedcd5a98769967ff1ff8501ddc89f8368b8f766e4701377d7dbaabe5`, matches the pin.
- Retained command-01 log records `VALIDSIG FCF986EA15E6E293A5644F10B4322F04D67658D8`, the pinned signer. This investigation read and hash-checked that log; it did not freshly run GPG.
- All 26 command logs match their receipt hashes; commands record zero exit status. Commands 8, 9 and 10 are configure, `make -j8`, and `make install`; command 11 is installed `ffprobe -version`.
- All 9893 regular archive entries match the corresponding retained source file bytes, with zero changed or missing files. This comparison does not assert that the source directory contains no generated/additional files.
- All 143 installed header files match corresponding files in the retained source directory.
- `ffbuild/config.mak` exactly matches the receipt's complete configuration text; command-11 version output matches the receipt's retained output.
- Source `config.h` declares `LGPL version 2.1 or later`, `CONFIG_GPL`, `CONFIG_NONFREE`, `CONFIG_VERSION3`, and `CONFIG_NETWORK` all 0. VideoToolbox and AudioToolbox are enabled. `config_components.h` enables `CONFIG_H264_VIDEOTOOLBOX_ENCODER` and `CONFIG_AAC_ENCODER`.
- Receipt and current source license hashes match: `COPYING.LGPLv2.1` = `246041b6ecf9bc32d718a62c57877c78b5eb397b6467e74ed7ae2626ab189c30`; `LICENSE.md` = `2e1d16c72fd74e12063776371da757322f8b77589386532f4fd8634bde7de1af`.

## Current artifacts

All seven real installed dylibs match the receipt:

| File under prefix/lib | SHA-256 |
| --- | --- |
| libavcodec.62.11.103.dylib | 58aac15dab11ce599ae595eb8e5bf54ed68327d9929451650fa3b4bcf6307f1f |
| libavdevice.62.1.103.dylib | 7b59be114d6dfd4a16184e688c9974337b2b3bac2188d96330c7d9aab6151184 |
| libavfilter.11.4.103.dylib | be1fde7995745f1e6b98fc657a101e340b584f1a0fd072cff72149ca44d6b4c5 |
| libavformat.62.3.103.dylib | 1e95ffdb6a28931038273c3c30a1807b1254613efb806e429caddf703fa5972d |
| libavutil.60.8.103.dylib | a5245d17b143832d7b57351d0bb97d0e31d65ae6a53bea195627b64e9b44edb8 |
| libswresample.6.1.103.dylib | ba3b62c128fbe9f50ef74a40b624efc1c1239e8bc4ddb822d56637bd2e9c10ce |
| libswscale.9.1.103.dylib | 02a571c85bbd5df607f7aea1021d93764b4d53cb1c1432d03ed7d840cb15d104 |

Every unversioned/major-version dylib symlink resolves inside this prefix to its matching real versioned dylib.

Installed `prefix/bin/ffprobe` and source-tree `ffprobe` are byte-identical, 203416 bytes, SHA-256 `ee5ef69a0a778327e6dff3e1d0550bc01a57ac83f61f88ca08c201111eeef1e8`. Source-tree `ffprobe_g` is 211176 bytes, SHA-256 `68989a1a1b3a378af966a3cc161083e976e494732914097514bc440bc88772c3`. Command-09 records `LD ffprobe_g` and `STRIP ffprobe`; command-10 records `INSTALL ffprobe`.

Current `otool -L` inspection of installed ffprobe names all seven FFmpeg libraries exclusively under `/private/tmp/deadpan-ui-ffmpeg/prefix/lib`; all other dependencies are `/System/Library/Frameworks/...` or `/usr/lib/libSystem.B.dylib`. Its Mach-O minimum is macOS 15.0, SDK 26.5. The receipt records those same deployment values for every dylib, whose bytes still match.

The receipt does not retain an ffprobe binary hash or installed-header hashes. The current comparisons above supply fresh observations, not retroactive receipt fields. Static load commands do not prove runtime loaded paths in the presence of loader overrides. Runtime admission must reject/clear those overrides and inspect the actual probe linkage.

Do not use `tools/media-qualification/compatible/results/build-2026-09-20.json` as this prefix's receipt. That older report names another build, despite the same source pin. `ffv1/qualify.py` currently hardcodes that older receipt and is not a sufficient exemplar for current receipt binding.

## Smallest harness admission

1. Take the actual receipt explicitly. Resolve and compare its prefix to the selected prefix; require schema/result, pinned archive/signature/key identities, matching builder/pins hashes, complete expected library inventory, and successful recorded build commands.
2. Rehash current real libraries against the receipt. Retain symlink resolutions and fresh ffprobe/header/configuration hashes. Hash the receipt and all harness/oracle sources. Preserve the distinction between receipt evidence and newly observed hashes.
3. Reject inherited loader overrides before running compiler/probe/ffprobe, and retain the controlled environment. Validate the probe's exact FFmpeg library paths, runtime versions/configuration/license, and ffprobe's exact version/configuration during parent-owned execution.
4. Rehash admitted receipt, sources, binary, ffprobe and libraries after the run, including failure paths. Retain failed command output and a failure report without claiming encoding/export acceptance.

## Exact substantive read/hash commands executed

Executed from `/Users/michael/Code/deadpan`:

```sh
python3 - <<'PY'
from pathlib import Path
import json, hashlib, tarfile
repo=Path('/Users/michael/Code/deadpan')
report_path=Path('/tmp/deadpan-ui-ffmpeg-build.json')
report=json.loads(report_path.read_text())
def sha(p):
    with Path(p).open('rb') as f: return hashlib.file_digest(f,'sha256').hexdigest()
print('REPORT',sha(report_path),report_path.stat().st_size)
print('META',json.dumps({k:report.get(k) for k in ('schema_version','result','started_utc','finished_utc','work_directory','prefix','source_sha256')},indent=2))
print('SCRIPT_MATCH', {n:sha(repo/'tools/media-qualification/compatible'/n)==h for n,h in report['source_sha256'].items()})
print('PIN_MATCH',report['pins']==json.loads((repo/'tools/media-qualification/compatible/pins.json').read_text()))
print('COMMANDS',len(report['commands']),[(i,c['argv'],c['exit_code']) for i,c in enumerate(report['commands']) if i<=11])
print('LOG_MATCH',all(Path(c['log']).is_file() and sha(c['log'])==c['log_sha256'] for c in report['commands']))
for name,item in report['libraries'].items():
    print('LIB',name,sha(item['path']),sha(item['path'])==item['sha256'])
for name,h in report['license_file_sha256'].items(): print('LICENSE',name,h,sha(Path(report['work_directory'])/'ffmpeg-8.0.3'/name)==h)
source=Path(report['work_directory'])/'ffmpeg-8.0.3'
print('CONFIG_MATCH',report['configuration']==(source/'ffbuild/config.mak').read_text())
print('VERSION_LOG_MATCH',report['ffprobe_version']==Path(report['commands'][11]['log']).read_text())
print('FFPROBE',sha(Path(report['prefix'])/'bin/ffprobe'),(Path(report['prefix'])/'bin/ffprobe').stat().st_size)
archive=Path('/tmp/deadpan-ui-ffmpeg/downloads/ffmpeg-8.0.3.tar.xz')
compared=0; changed=[]; missing=[]
with tarfile.open(archive,'r:xz') as tar:
    for m in tar:
        if not m.isfile(): continue
        relative=Path(m.name).relative_to('ffmpeg-8.0.3')
        p=source/relative
        if not p.is_file(): missing.append(str(relative)); continue
        af=tar.extractfile(m)
        original=hashlib.file_digest(af,'sha256').hexdigest()
        if sha(p)!=original: changed.append(str(relative))
        compared+=1
print('ARCHIVE_SOURCE_COMPARE',{'regular_files_compared':compared,'changed':changed,'missing':missing})
PY
```

```sh
python3 - <<'PY'
from pathlib import Path
import hashlib, json
root=Path('/tmp/deadpan-ui-ffmpeg'); source=root/'ffmpeg-8.0.3'; prefix=root/'prefix'
def sha(p):
    with p.open('rb') as f: return hashlib.file_digest(f,'sha256').hexdigest()
for name in ('ffprobe','ffprobe_g'):
    p=source/name
    if p.is_file(): print('BUILD_PROGRAM',str(p),p.stat().st_size,sha(p))
compared=0; changed=[]; missing=[]
for p in sorted((prefix/'include').rglob('*')):
    if not p.is_file(): continue
    relative=p.relative_to(prefix/'include'); upstream=source/relative
    if not upstream.is_file(): missing.append(str(relative)); continue
    if sha(p)!=sha(upstream): changed.append(str(relative))
    compared+=1
print('INSTALLED_HEADER_SOURCE_COMPARE',{'files':compared,'changed':changed,'missing_source':missing})
print('LIB_SYMLINKS',json.dumps({p.name:str(p.resolve()) for p in sorted((prefix/'lib').glob('*.dylib')) if p.is_symlink()},indent=2))
report=json.loads(Path('/tmp/deadpan-ui-ffmpeg-build.json').read_text())
print('LINK_AND_DEPLOYMENT_RECEIPT')
for name,item in report['libraries'].items():
    print(name,item['build_version'].strip())
print('FFPROBE_BYTES_MATCH_BUILT',sha(prefix/'bin/ffprobe')==sha(source/'ffprobe'))
PY
```

```sh
otool -L /tmp/deadpan-ui-ffmpeg/prefix/bin/ffprobe
otool -l /tmp/deadpan-ui-ffmpeg/prefix/bin/ffprobe | rg -A 7 'LC_BUILD_VERSION|LC_RPATH'
rg -n 'STRIP.*ffprobe|LD.*ffprobe|INSTALL.*ffprobe' /tmp/deadpan-ui-ffmpeg/command-09.log /tmp/deadpan-ui-ffmpeg/command-10.log
rg -n '^#define (CONFIG_(GPL|NONFREE|VERSION3|NETWORK|VIDEOTOOLBOX|AUDIOTOOLBOX|H264_VIDEOTOOLBOX_ENCODER|AAC_ENCODER)|FFMPEG_LICENSE)' /tmp/deadpan-ui-ffmpeg/ffmpeg-8.0.3/config.h /tmp/deadpan-ui-ffmpeg/ffmpeg-8.0.3/config_components.h
```

The download hashes were obtained with `hashlib.sha256(p.read_bytes()).hexdigest()` for every direct file in `/tmp/deadpan-ui-ffmpeg/downloads` and compared with the pinned values printed from `tools/media-qualification/compatible/pins.json`.
