"""Synthetic field-motion originals, encoded with development FFmpeg/libx264.

Every progressive input field has a known moving rectangle. tinterlace combines
successive fields; the runtime must recover their distinct temporal positions.
--check reproduces the committed fixtures without writing them.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import tempfile
import wave

ROOT = Path(__file__).parent / 'fixtures'
WIDTH, HEIGHT = 96, 64
CASES = [('fields-tff.mp4', '50', 'top', 24, 0),
         ('fields-bff.mp4', '60000/1001', 'bottom', 24, 0),
         ('fields-single.mp4', '50', 'top', 2, 0),
         ('fields-single-bff.mp4', '50', 'bottom', 2, 0),
         ('fields-tff-bframes.mp4', '50', 'top', 24, 2),
         ('fields-100.mp4', '100', 'top', 24, 0)]

def frames(count):
    for field in range(count):
        y = bytearray([32] * (WIDTH * HEIGHT))
        for row in range(12, 52):
            x = 8 + field * 2
            y[row*WIDTH+x:row*WIDTH+x+12] = bytes([208] * 12)
        yield bytes(y) + bytes([128] * (WIDTH * HEIGHT // 2))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='deadpan-fields-') as scratch:
        scratch = Path(scratch)
        for name, rate, parity, count, bframes in CASES:
            raw = scratch/'fields.yuv'
            raw.write_bytes(b''.join(frames(count)))
            wav = scratch/'audio.wav'
            parts = [int(part) for part in rate.split('/')]
            numerator, denominator = parts if len(parts) == 2 else (parts[0], 1)
            samples = count * 48000 * denominator // numerator
            with wave.open(str(wav), 'wb') as audio:
                audio.setnchannels(1)
                audio.setsampwidth(2)
                audio.setframerate(48000)
                audio.writeframes(b''.join(struct.pack('<h', round(6000*math.sin(2*math.pi*440*n/48000)))
                                           for n in range(samples)))
            target = scratch/name
            subprocess.run(['ffmpeg','-nostdin','-v','error','-f','rawvideo','-pixel_format','yuv420p',
                '-video_size',f'{WIDTH}x{HEIGHT}','-framerate',rate,'-i',str(raw),'-i',str(wav),
                '-vf',f'tinterlace=interleave_{parity}', '-c:v','libx264','-qp','12',
                '-flags:v','+ilme+ildct+bitexact', '-fflags','+bitexact',
                '-x264-params',f'{"tff" if parity == "top" else "bff"}=1:keyint=3:min-keyint=3:scenecut=0:bframes={bframes}:colorprim=bt709:transfer=bt709:colormatrix=bt709',
                '-color_range','tv','-colorspace','bt709','-color_trc','bt709','-color_primaries','bt709',
                '-chroma_sample_location','left','-video_track_timescale','60000',
                '-c:a','aac','-b:a','128k','-flags:a','+bitexact','-movflags','+faststart','-y',str(target)],check=True)
            data = target.read_bytes()
            destination = ROOT/name
            if args.check:
                assert destination.read_bytes() == data, name
            else:
                destination.write_bytes(data)
            print(json.dumps({'name':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),
                              'fields':count,'field_rate':[numerator,denominator],
                              'parity':parity,'audio_samples':samples}))

if __name__ == '__main__':
    main()
