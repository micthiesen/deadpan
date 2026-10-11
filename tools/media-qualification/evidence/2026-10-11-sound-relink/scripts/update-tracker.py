from pathlib import Path

path = Path('/Users/michael/Code/deadpan/docs/REQUIREMENTS.md')
old = 'Native register/insert has [workspace evidence](qualification/native-workspace-2026-09-21.md); single-original initialization retry is implemented; relink and basis-preview UI, bookmark resolution, legacy asset requalification, full sound ownership and full format/failure matrix remain open. [Native root sound placement](qualification/native-sound-placement-2026-09-27.md) is a qualified-catalog subset.'
new = 'Managed and [linked Original creation](qualification/linked-original-2026-10-09.md), recovery/retry, bookmark resolution and native relinking are implemented. [Catalog sound recovery](qualification/sound-recovery-2026-10-11.md) verifies managed repair, linked sound location changes and history preservation. [Sound placement](qualification/native-sound-placement-2026-09-27.md) has native evidence. The consolidated local/sound acceptance audit remains; broader source-format and creative-operation coverage is tracked with DP-02 and DP-16.'
text = path.read_text()
assert text.count(old) == 1
path.write_text(text.replace(old, new))
