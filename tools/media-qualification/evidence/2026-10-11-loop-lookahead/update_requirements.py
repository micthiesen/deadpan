from pathlib import Path
path = Path('/Users/michael/Code/deadpan/docs/REQUIREMENTS.md')
text = path.read_text()
old = ' | Full format/color matrix, mastered playback, effects, decode-ahead at selection-loop seams and across Generated Holds,'
new = (' [Selection-loop preparation](qualification/loop-lookahead-2026-10-11.md) now follows the captured audible window in Original and Edit, including measured VFR, partial-frame context and long final-picture holds. Internal cuts precede the seam; revision/domain/generation changes reject old scheduling. Thirty-second real-device Edit loops drop zero pictures at 1080p60 and 4K30 (17 and 12 without look-ahead), with zero underruns; loop-return maxima fall from 55.4/85.1 ms to 6.7/15.8 ms. '
       '| Full format/color matrix, mastered playback, effects, decode-ahead across Generated Holds,')
assert text.count(old) == 1
path.write_text(text.replace(old, new))
