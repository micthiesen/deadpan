# Scripted confirmation thumbnail

`youtube-thumbnail.png` is a generated 960 × 540 RGB image with four flat
quadrants, clockwise from the upper left: `(206, 94, 80)`, `(66, 171, 169)`,
`(217, 166, 66)`, `(143, 108, 196)`. The dividing coordinates are x=480 and y=270.
It is owned test data, not a frame or thumbnail from the named YouTube video.
The scripted import decodes and resizes it through the production bounded image
decoder to 480 × 270 RGBA, exercising the native 160/96-point display limits.

SHA-256: `ea5bc8e2af57a592fc88d4e2f8c20a2932811c5a54889e3e4a6a0bb497030ad8`.
