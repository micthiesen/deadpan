from pathlib import Path
import array, hashlib, json, math, subprocess, sys
r=Path(__file__).parent
report=json.loads((r/'export-report.json').read_text())
movie=Path(report['movie'])
raw=r/'decoded-aac.f32le'
cmd=['ffmpeg','-nostdin','-v','error','-i',str(movie),'-map','0:a:0','-f','f32le',str(raw)]
subprocess.run(cmd,check=True)
a=array.array('f');a.frombytes(raw.read_bytes())
if sys.byteorder != 'little':a.byteswap()
rows=[]
for frame,expected in [(4,None),(16,2.0)]:
 start=(frame*16016+5)//10
 values=[a[2*start+c:2*(start+4096):2] for c in range(2)]
 assert all(len(ch)==4096 and all(math.isfinite(x) for x in ch) for ch in values)
 peaks=[max(abs(x) for x in ch) for ch in values]
 rms=[math.sqrt(sum(x*x for x in ch)/len(ch)) for ch in values]
 row=dict(start=start,count=4096,peaks=peaks,rms=rms,right_over_left=rms[1]/rms[0])
 if expected is None:assert 0.31 < peaks[0] < 0.33 and peaks[1] < 0.00001,row
 else:assert abs(rms[1]/rms[0]-expected)<0.02 and 0.155 < peaks[0] < 0.165 and 0.31 < peaks[1] < 0.33,row
 rows.append(row)
version=subprocess.check_output(['ffmpeg','-version'],text=True)
(r/'ffmpeg-version.txt').write_text(version)
result=dict(command=cmd,passed=True,movie=str(movie),movie_sha256=hashlib.sha256(movie.read_bytes()).hexdigest(),pcm_sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),decoded_frames=len(a)//2,windows=rows,no_alignment_applied=True)
(r/'encoded-channels.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
