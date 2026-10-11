from pathlib import Path
import subprocess
import sys

root=Path(__file__).parent
steps=[
    ('lingering-ui', ['cargo','nextest','run','--locked','-p','deadpan-app','--features','ui-harness','--bin','deadpan-app','-E','test(insert_and_remove_reshape_only_affected_segments_and_retain_cubic_controls)','--test-threads','1']),
    ('real-app-job', ['env','DEADPAN_REAL_YOUTUBE_URL=https://youtu.be/Z4C82eyhwgU','DEADPAN_REAL_YOUTUBE_REQUIRE_THUMBNAIL=1','cargo','nextest','run','--locked','-p','deadpan-app','--bin','deadpan-app','-E','test(real_url_import_through_the_app_job_when_requested)','--success-output','immediate']),
    ('chaos', ['cargo','xtask','chaos','--minutes','0.25','--seed','20261011','--only','cli-boundaries','--output',str(root/'chaos')]),
    ('bundle', ['cargo','xtask','bundle','--output','/tmp/deadpan-youtube-network-bundle-20261011']),
    ('bundle-verify', ['cargo','xtask','bundle-verify','/tmp/deadpan-youtube-network-bundle-20261011/Deadpan.app']),
]
for name, command in steps:
    print('START',name,flush=True)
    result=subprocess.run(['python3',str(root/'run.py'),name,*command])
    if result.returncode:
        print((root/(name+'.log')).read_text(errors='replace')[-5000:],flush=True)
        sys.exit(result.returncode)
subprocess.run(['python3',str(root/'packaged.py')],check=True)
