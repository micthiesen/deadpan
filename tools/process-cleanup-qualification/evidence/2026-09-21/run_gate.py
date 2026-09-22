import datetime, hashlib, json, os, pathlib, re, subprocess, sys, time
root=pathlib.Path(__file__).parent / "gate"
root.mkdir(exist_ok=True)
env=os.environ.copy()
env["DEADPAN_FFMPEG_PREFIX"]="/tmp/deadpan-media-compatible-xyhilms4/prefix"
sources=["native/deadpan-process/src/lib.rs", "native/deadpan-process/Cargo.toml", "native/deadpan-process/tests/linux_ownership.rs", "crates/deadpan-media/src/conversion.rs", "crates/deadpan-media/tests/host_boundary.rs", "crates/deadpan-media/Cargo.toml", "crates/deadpan-jobs/src/supervisor.rs", "crates/deadpan-jobs/tests/supervisor.rs", "crates/deadpan-jobs/Cargo.toml"]
source_hashes={path:hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest() for path in sources}
(root/"source-manifest.json").write_text(json.dumps({"base_revision":subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip(),"files":source_hashes},indent=2)+"\n")
commands=[
 ["cargo","fmt","--all","--","--check"],
 ["cargo","clippy","--workspace","--all-targets","--locked","--","-D","warnings"],
 ["cargo","test","--workspace","--locked"],
 ["cargo","build","--workspace","--locked"],
 ["cargo","run","-p","deadpan-cli","--","doctor"],
]
results=[]
for index,command in enumerate(commands):
 name=f"{index+1:02d}-{command[1]}.log"
 start=time.monotonic()
 with (root/name).open("wb") as out:
  result=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
 data=(root/name).read_bytes()
 item={"command":command,"exit_code":result.returncode,"elapsed_seconds":time.monotonic()-start,"log":name,"sha256":hashlib.sha256(data).hexdigest()}
 item["source_identity_stable"]=all(hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()==expected for path,expected in source_hashes.items())
 if not item["source_identity_stable"]:
  item["command_exit_code"]=item["exit_code"]
  item["exit_code"]=99
 if command[1]=="test":
  counts=re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored",data)
  item["test_totals"]={key:sum(int(row[i]) for row in counts) for i,key in enumerate(["passed","failed","ignored"])}
 results.append(item)
 (root/"results.json").write_text(json.dumps({"created_utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"ffmpeg_prefix":env["DEADPAN_FFMPEG_PREFIX"],"results":results},indent=2)+"\n")
 print(json.dumps(item),flush=True)
 if item["exit_code"]:
  print(data.decode(errors="replace")[-7000:],flush=True)
  sys.exit(item["exit_code"])
