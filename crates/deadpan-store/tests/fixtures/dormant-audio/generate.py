"""Mux existing synthetic fixtures with sound only in the middle of the picture."""
import hashlib
import json
from pathlib import Path
import subprocess

destination = Path(__file__).resolve().parent
repository = destination.parents[4]
video = repository / 'native/deadpan-source/tests/fixtures/cfr-bframes.mp4'
audio = repository / 'native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav'
output = destination / 'middle-audio.mp4'
arguments = [
    'ffmpeg', '-hide_banner', '-loglevel', 'error', '-nostdin', '-n',
    '-i', str(video), '-itsoffset', '1', '-i', str(audio),
    '-map', '0:v:0', '-map', '1:a:0', '-c:v', 'copy', '-c:a', 'aac', '-b:a', '128k',
    '-map_metadata', '-1', '-movflags', '+faststart', str(output),
]
subprocess.run(arguments, check=True)
report = {
    'purpose': 'Synthetic 30000/1001 picture with 44100 Hz mono sound confined to an interior interval.',
    'command': [str(Path(value).relative_to(repository)) if value.startswith(str(repository)) else value for value in arguments],
    'ffmpeg_version': subprocess.check_output(['ffmpeg', '-version'], text=True),
    'inputs_sha256': {str(path.relative_to(repository)): hashlib.sha256(path.read_bytes()).hexdigest() for path in (video, audio)},
    'output_sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
    'output_bytes': output.stat().st_size,
    'streams': json.loads(subprocess.check_output([
        'ffprobe', '-v', 'error', '-show_entries',
        'stream=index,codec_name,codec_type,time_base,start_pts,start_time,duration_ts,duration,sample_rate,channels,channel_layout',
        '-of', 'json', str(output),
    ], text=True)),
    'decoded_audio_frames': json.loads(subprocess.check_output([
        'ffprobe', '-v', 'error', '-select_streams', 'a', '-show_frames',
        '-show_entries', 'frame=pts,nb_samples', '-of', 'json', str(output),
    ], text=True)),
}
(destination / 'provenance.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'output_bytes': report['output_bytes'], 'output_sha256': report['output_sha256'], 'streams': report['streams']}, indent=2))
