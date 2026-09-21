"""Development-only deterministic FFV1 fixtures; uses ffmpeg 9.0.1 CLI producer.
The application/tests decode only through the pinned FFmpeg 8.0.3 libraries.
"""
from pathlib import Path
import os
import subprocess
import tempfile

root = Path(__file__).parent
env = os.environ.copy()
with tempfile.TemporaryDirectory(prefix='deadpan-source-color-') as scratch:
    raw = Path(scratch) / 'vectors.yuv'
    # YUV444 permits direct per-pixel matrix tests without chroma resampling.
    raw.write_bytes(bytes([16,235,81,145]*2 + [128,128,90,54]*2 + [128,128,240,34]*2))
    for name,range_,transfer,format_,extra in [
        ('limited709','tv','bt709','yuv444p',''),
        ('full709','pc','bt709','yuv444p',''),
        ('hdr-pq','tv','smpte2084','yuv444p',''),
        ('ten-bit','tv','bt709','yuv444p10le',''),
        ('interlaced','tv','bt709','yuv444p',':field_mode=tff'),
        ('anamorphic','tv','bt709','yuv444p',',setsar=2/1')]:
        subprocess.run(['ffmpeg','-nostdin','-v','error','-f','rawvideo','-pix_fmt','yuv444p','-video_size','4x2','-framerate','24','-i',str(raw),'-vf',f'setparams=range={range_}:color_primaries=bt709:color_trc={transfer}:colorspace=bt709{extra}','-frames:v','1','-an','-c:v','ffv1','-level','3','-pix_fmt',format_,'-color_range',range_,'-colorspace','bt709','-color_trc',transfer,'-color_primaries','bt709','-y',str(root/'fixtures'/f'{name}.mkv')],check=True,env=env)
    rgb=root/'../../deadpan-media-worker/tests/fixtures/rgb1_24.mp4'
    subprocess.run(['ffmpeg','-nostdin','-v','error','-display_rotation','90','-i',str(rgb),'-c','copy','-y',str(root/'fixtures/rotated90.mp4')],check=True,env=env)
