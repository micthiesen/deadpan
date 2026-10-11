from pathlib import Path
import hashlib,json,subprocess,time
r=Path(__file__).parent
version=subprocess.check_output(['ffmpeg','-version']).decode()
(r/'fixture-ffmpeg-version.txt').write_text(version)
rows=[]
for name,size,rate,seconds in [('1080p60','1920x1080',60,30),('4k30','3840x2160',30,20)]:
 out=r/(name+'.mp4')
 args=['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i',f'testsrc2=size={size}:rate={rate}:duration={seconds}','-f','lavfi','-i',f'sine=frequency=440:sample_rate=48000:duration={seconds}','-ac','2','-c:v','libx264','-preset','veryfast','-pix_fmt','yuv420p','-bsf:v','h264_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1','-color_range','tv','-colorspace','bt709','-color_primaries','bt709','-color_trc','bt709','-c:a','aac','-b:a','160k','-movflags','+faststart',str(out)]
 start=time.monotonic()
 p=subprocess.run(args,capture_output=True)
 (r/(name+'.generate.stderr')).write_bytes(p.stderr)
 assert p.returncode==0,p.stderr
 rows.append(dict(command=args,exit=p.returncode,seconds=time.monotonic()-start,bytes=out.stat().st_size,sha256=hashlib.sha256(out.read_bytes()).hexdigest()))
 print(name,rows[-1],flush=True)
(r/'generated-fixtures.json').write_text(json.dumps(rows,indent=2)+'\n')
