from pathlib import Path
repo=Path('/Users/michael/Code/deadpan')
record='qualification/generated-app-playback-2026-10-11.md'
changes={}
def edit(name,old,new):
 s=changes.get(name,(repo/name).read_text())
 assert s.count(old)==1,(name,s.count(old),old)
 changes[name]=s.replace(old,new)
paragraph='''[Real Generated playback in the app]('''+record+''')
now qualifies the actual preview worker with real accepted LTX pictures,
full-resolution Originals and device audio. All 1,830 frames at 1080p60 and
615 frames at 4K30 reach offscreen Metal, with no skips, proxies, underruns or
faults. A replay telemetry fix retains completion timing when the next decode
is requested in the same UI pass. Full format/effects and broader transition
stress remain open; DP-16 stays partial.

'''
edit('docs/REQUIREMENTS.md','## Resumed implementation, 2026-10-06\n\n','## Resumed implementation, 2026-10-06\n\n'+paragraph)
edit('docs/REQUIREMENTS.md','unit tests pass. Full-resolution device measurements remain open under DP-16.','unit tests pass. Full-resolution app/device measurements are recorded above.')
edit('docs/REQUIREMENTS.md','above; full-resolution device measurements remain open and DP-16 stays partial.','above; the subsequent app/device qualification is also recorded above. DP-16 stays partial.')
s=changes['docs/REQUIREMENTS.md'];line=next(line for line in s.splitlines() if line.startswith('| DP-16 |'))
new=line.replace(' | Full format/color matrix, mastered playback, effects, full-resolution Generated decoder-handoff qualification,', ' [Real app Generated playback]('+record+') presents all 1,830/615 frames of the 1080p60/4K30 fixtures with accepted real LTX pictures, zero skips or audio faults, and complete ticket-bound GPU timings. | Full format/color matrix, mastered playback, effects, broader Generated-provider/loop stress,')
assert new!=line
changes['docs/REQUIREMENTS.md']=s.replace(line,new)
edit('docs/PERFORMANCE.md','prepared entry takes 0.17–0.22 ms. Full-resolution device measurements remain open.\nA target\nwithout a measured workload is open, never passed.','prepared entry takes 0.17–0.22 ms. The [real app playback qualification]('+record+')\nthen exercises real accepted LTX pictures and full-resolution 1080p60/4K30\nOriginals through the app worker, real device audio and complete offscreen Metal\nUI. Every picture is presented, with zero skips, proxies, underruns or faults.\nThe record separates worker time from complete UI composition and documents\nthe timing bookkeeping correction. A target without a measured workload is\nopen, never passed.')
edit('docs/PLAYBACK.md','and [entry preparation](qualification/generated-prefetch-2026-10-11.md) records.','and [entry preparation](qualification/generated-prefetch-2026-10-11.md) records.\nThe [actual app playback measurement]('+record+') uses real accepted LTX\npictures, 1080p60/4K30 Originals, device audio and offscreen Metal: all\n1,830/615 frames are presented, without skips, proxy pictures or audio faults.')
edit('docs/spec/AGENT_HANDOFF.md','## Resumed grammar and release audit, 2026-10-06\n\n','## Resumed grammar and release audit, 2026-10-06\n\n'+paragraph.replace(']('+record,'](../'+record))
edit('docs/spec/AGENT_HANDOFF.md','Prepared entry is 0.17–0.22 ms on the small fixture. Full-resolution measurements\nmust use the actual preview worker, since the CLI perf reader does not exercise\nthis app-owned cache. DP-16 remains partial.','Prepared entry is 0.17–0.22 ms on the small fixture. The full-resolution app\nmeasurement above uses the actual preview worker; the CLI perf reader does not\nexercise this app-owned cache. DP-16 remains partial.')
edit('docs/spec/AGENT_HANDOFF.md','full-resolution device measurements remain open under DP-16.','the full-resolution app/device measurement is also recorded above.')
for name,s in changes.items():(repo/name).write_text(s)
