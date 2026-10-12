import datetime,hashlib,json,os,pathlib,re,subprocess,sys,time
root=pathlib.Path('/Users/michael/Code/deadpan')
scratch=pathlib.Path(__file__).resolve().parent
name,*command=sys.argv[1:]
env=os.environ.copy()
env['DEADPAN_FFMPEG_PREFIX']='/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix'
start=time.monotonic()
receipt=dict(name=name,command=command,cwd=str(root),started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),diff_sha256=hashlib.sha256(subprocess.check_output(['git','diff','--binary'],cwd=root)).hexdigest())
receipt['untracked_sha256']={n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=root,text=True).splitlines() if (root/n).is_file()}
path=scratch/(name+'.log')
with path.open('wb') as log:
 result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
receipt.update(exit=result.returncode,seconds=time.monotonic()-start,log_sha256=hashlib.sha256(path.read_bytes()).hexdigest())
text=path.read_text(errors='replace')
paths=set(re.findall(r'Running .*?\((target/[^)]+)\)',text))|{token for token in command if token.startswith('target/')}
receipt['binaries']={n:hashlib.sha256((root/n).read_bytes()).hexdigest() for n in paths if (root/n).is_file()}
result_path=scratch/(name+'.json')
if result_path.exists() and 'log_sha256' not in json.loads(result_path.read_text()):
    (scratch/(name+'-result.json')).write_bytes(result_path.read_bytes())
result_path.write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:receipt[k] for k in ['name','exit','seconds']}))
print(text[-4500:])
sys.exit(result.returncode)
