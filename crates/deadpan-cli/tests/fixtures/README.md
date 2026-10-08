# Temporal context fixture

`extension-fade.mp4` contains 120 synthetic 64×36 RGB pictures at
30000/1001 fps: 80 pictures of one scene, a 32-picture linear dissolve and
eight pictures of the second scene. Lossless RGB H.264 keeps the exact pixels;
the MP4 clock is 1/30000 with PTS `i * 1001`. Colour is full-range sRGB with
BT.709 primaries. There is no audio or real-person footage.

SHA-256: `e12e0d32a7f52e290b162e496bbabcc6f648a97d4f46b25b031991ce5909aa7f`.
The checked-in generator used FFmpeg 9.0.1. Tests decode through Deadpan's
pinned native FFmpeg, with no external executable required by the test.

Run `python3 generate_extension_fade.py --check` to compare all decoded RGB
bytes against the source formula. To regenerate, move the old fixture aside
and run the generator without arguments; it refuses to overwrite a file.

The regression splits the context into canonical one-frame partitions and
checks that every picture remains unchanged. Both complete and fragmented
versions must reject the same dissolve; splitting must not disable the guard.
