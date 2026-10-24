import datetime,hashlib,json,os,pathlib,subprocess,sys,time
root=pathlib.Path('/Users/michael/Code/deadpan'); scratch=pathlib.Path(__file__).resolve().parent
paths=['crates/deadpan-plan/src/lib.rs','crates/deadpan-plan/src/plan.rs','crates/deadpan-plan/src/follow_context.rs','crates/deadpan-plan/tests/picture_plan.rs','crates/deadpan-plan/tests/picture_plan/captured_follow.rs']
label=sys.argv[1]; argv=sys.argv[2:]
def pins():
 return {s:{'bytes':(root/s).stat().st_size,'sha256':hashlib.sha256((root/s).read_bytes()).hexdigest()} for s in paths}
record={'command':argv,'cwd':str(root),'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_before':pins(),'environment':{k:os.environ[k] for k in ['CARGO_TARGET_DIR','DEADPAN_FFMPEG_PREFIX'] if k in os.environ}}
receipt=scratch/(label+'.json');assert not receipt.exists()
receipt.write_text(json.dumps(record,indent=2)+'\n');start=time.monotonic()
with (scratch/(label+'.stdout')).open('xb') as stdout,(scratch/(label+'.stderr')).open('xb') as stderr:
 result=subprocess.run(argv,cwd=root,stdout=stdout,stderr=stderr)
record.update(exit_code=result.returncode,seconds=time.monotonic()-start,finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),source_after=pins())
record['sources_unchanged']=record['source_before']==record['source_after']
receipt.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps({k:record[k] for k in ['exit_code','seconds','sources_unchanged']}));sys.exit(result.returncode)
