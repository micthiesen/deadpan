#!/bin/sh
# Regenerate fixtures/speech.mp4: about seven seconds of synthesized English
# speech (macOS `say`, voice Samantha) under a 160x90 gray H.264 picture with
# BT.709-tagged picture and mono AAC, the input of the sanitized whisper.cpp run in
# `cargo xtask sanitizers`. Developer-only; needs macOS `say` and an ffmpeg
# with libx264. The committed file is the evidence input; regeneration may
# differ byte-for-byte across OS voice versions.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
say -v Samantha -o "$scratch/speech.aiff" \
  "The quick brown fox jumps over the lazy dog. Deadpan checks every native adapter under address and thread sanitizers before release."
ffmpeg -y -hide_banner -loglevel error -f lavfi -i "color=c=gray:s=160x90:r=30" \
  -i "$scratch/speech.aiff" -map 0:v -map 1:a -shortest \
  -c:v libx264 -preset veryslow -crf 40 -pix_fmt yuv420p -g 30 -bf 0 \
  -color_range tv -colorspace bt709 -color_primaries bt709 -color_trc bt709 \
  -bsf:v h264_metadata=colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1:video_full_range_flag=0 \
  -c:a aac -ac 1 -ar 48000 -b:a 48k -movflags +faststart+write_colr \
  "$here/fixtures/speech.mp4"
