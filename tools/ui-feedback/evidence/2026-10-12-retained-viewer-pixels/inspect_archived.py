"""Read archived PNG pixels without changing the captures."""
import gzip,hashlib,json
from pathlib import Path
from PIL import Image
root=Path('/Users/michael/Code/deadpan')
evidence=root/'tools/audio-qualification/evidence'
cases=[
 ('previous',evidence/'2026-10-12-event-returns/sound-replay-report.json.gz',evidence/'2026-10-12-event-treatments/placement-viewer-base.png'),
 ('current',evidence/'2026-10-12-event-treatments/sound-replay-final-report.json.gz',evidence/'2026-10-12-event-treatments/placement-viewer-current.png'),
]
records=[]
for name,report,png in cases:
 r=json.loads(gzip.decompress(report.read_bytes()))
 s=next(x for x in r['scenarios'] if x['name']=='sound-placement')
 step=next(x for x in s['steps'] if x['input']=='Sound removal is reachable at 960x640')
 widgets=step['semantic']['widgets']
 viewer=next(x for x in widgets if x['role']=='Image' and x['label']=='Showing sequence frame 3')
 ppp=step['semantic']['pixels_per_point']
 left,top,right,bottom=[round(v*ppp) for v in viewer['rect']]
 image=Image.open(png).convert('RGBA')
 colors=[0,0,0]
 for y in range(top,bottom):
  for x in range(left,right):
   pixel=image.getpixel((x,y))
   for c in range(3):
    if pixel[3]>=250 and pixel[c]>200 and pixel[(c+1)%3]<80 and pixel[(c+2)%3]<80:
     colors[c]+=1
 records.append(dict(name=name,png=str(png.relative_to(root)),sha256=hashlib.sha256(png.read_bytes()).hexdigest(),report_sha256=hashlib.sha256(report.read_bytes()).hexdigest(),viewer=viewer['rect'],pixels_per_point=ppp,primary_pixels=colors,displayed=step['semantic']['picture']['displayed']['label']))
 assert all(count>10 for count in colors)
out=Path(__file__).resolve().parent/'archived-pixels.json'
out.write_text(json.dumps(records,indent=2)+'\n')
print(json.dumps(records,indent=2))
