from pathlib import Path
import hashlib,json,subprocess,time
r=Path(__file__).parent
rows=[]
for name,codec,args in [('hevc-4k','libx265',['-preset','ultrafast','-x265-params','keyint=240:min-keyint=240:scenecut=0:colorprim=bt709:transfer=bt709:colormatrix=bt709:range=limited','-tag:v','hvc1']),('vp9-4k','libvpx-vp9',['-deadline','realtime','-cpu-used','8','-row-mt','1','-tile-columns','2','-g','240','-crf','30','-b:v','0'])]:
 out=r/(name+'.mp4')
 command=['ffmpeg','-hide_banner','-loglevel','error','-i',str(r/'4k30.mp4'),'-t','8','-an','-c:v',codec,*args,'-pix_fmt','yuv420p','-color_range','tv','-colorspace','bt709','-color_primaries','bt709','-color_trc','bt709','-chroma_sample_location','left','-movflags','+faststart',str(out)]
 start=time.monotonic();p=subprocess.run(command,capture_output=True,timeout=300)
 (r/(name+'.generate.stderr')).write_bytes(p.stderr)
 assert p.returncode==0,p.stderr
 rows.append(dict(command=command,exit=p.returncode,seconds=time.monotonic()-start,bytes=out.stat().st_size,sha256=hashlib.sha256(out.read_bytes()).hexdigest()))
 print(name,rows[-1],flush=True)
(r/'generated-codecs.json').write_text(json.dumps(rows,indent=2)+'\n')
