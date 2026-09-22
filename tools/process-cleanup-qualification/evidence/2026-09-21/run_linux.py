import hashlib,json,pathlib,re,subprocess,sys,time
root=pathlib.Path(__file__).parent/"linux"
root.mkdir(exist_ok=True)
sources=["native/deadpan-process/src/lib.rs", "native/deadpan-process/Cargo.toml", "native/deadpan-process/tests/linux_ownership.rs", "crates/deadpan-jobs/src/supervisor.rs", "crates/deadpan-jobs/tests/supervisor.rs", "crates/deadpan-jobs/Cargo.toml"]
source_hashes={p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() for p in sources}
(root/"source-manifest.json").write_text(json.dumps(source_hashes,indent=2)+"\n")
image="rust@sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3"
commands=[
 ["docker","image","inspect","--format","{{json .RepoDigests}} {{.Os}} {{.Architecture}}",image],
 ["docker","run","--rm","--network","none",image,"rustc","--version"],
 ["docker","run","--rm","--mount",f"type=bind,src={pathlib.Path.cwd()},dst=/src,readonly","--workdir","/src","--env","CARGO_TARGET_DIR=/tmp/deadpan-linux-target",image,"cargo","test","--locked","-p","deadpan-native-process","-p","deadpan-jobs"],
]
results=[]
for i,cmd in enumerate(commands):
 start=time.monotonic()
 path=root/f"{i+1:02d}.log"
 with path.open("wb") as out:
  result=subprocess.run(cmd,stdout=out,stderr=subprocess.STDOUT)
 data=path.read_bytes()
 item={"command":cmd,"exit_code":result.returncode,"elapsed_seconds":time.monotonic()-start,"log":path.name,"sha256":hashlib.sha256(data).hexdigest()}
 item["source_identity_stable"]=all(hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()==h for p,h in source_hashes.items())
 if not item["source_identity_stable"]:
  item["command_exit_code"]=item["exit_code"]
  item["exit_code"]=99
 if i==2:
  rows=re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored",data)
  item["test_totals"]={key:sum(int(row[j]) for row in rows) for j,key in enumerate(["passed","failed","ignored"])}
 results.append(item)
 (root/"results.json").write_text(json.dumps(results,indent=2)+"\n")
 print(json.dumps(item),flush=True)
 if item["exit_code"]:
  print(data.decode(errors="replace")[-5000:],flush=True)
  sys.exit(item["exit_code"])
