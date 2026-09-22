import json, os, pathlib, signal, subprocess, tempfile, time
root=pathlib.Path(__file__).parent
records=[]
with tempfile.TemporaryDirectory(prefix="deadpan-shell-diagnostics-") as scratch:
 path=pathlib.Path(scratch)/"fixture.sh"
 path.write_text("#!/bin/sh\n"+"(sleep 1; printf alive > survived) &\n"*32+"printf '%s' '{\"ok\":true}' >&2\nexit 0\n")
 path.chmod(0o700)
 for trial in range(100):
  child=subprocess.Popen([str(path)],cwd=scratch,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,start_new_session=True)
  data=bytearray()
  try:
   os.waitid(os.P_PID,child.pid,os.WEXITED|os.WNOWAIT)
   os.set_blocking(child.stderr.fileno(),False)
   end=time.monotonic()+0.25
   while True:
    os.waitid(os.P_PID,child.pid,os.WEXITED|os.WNOWAIT|os.WNOHANG)
    try: os.killpg(child.pid,signal.SIGKILL)
    except (PermissionError,ProcessLookupError): pass
    eof=False
    while True:
     try: chunk=os.read(child.stderr.fileno(),8192)
     except BlockingIOError: break
     if not chunk:
      eof=True
      break
     data.extend(chunk)
    if eof or time.monotonic()>=end: break
    time.sleep(.002)
   records.append({"trial":trial,"eof":eof,"stderr_hex":data.hex(),"stderr_text":data.decode(errors="replace")})
  finally:
   os.waitid(os.P_PID,child.pid,os.WEXITED|os.WNOWAIT|os.WNOHANG)
   try: os.killpg(child.pid,signal.SIGKILL)
   except (PermissionError,ProcessLookupError): pass
   child.wait()
   child.stderr.close()
  if data != b'{"ok":true}':
   print(json.dumps(records[-1]),flush=True)
   break
(root/"shell-diagnostics.json").write_text(json.dumps(records,indent=2)+"\n")
print(json.dumps({"trials":len(records),"unexpected_replies":sum(r["stderr_text"]!='{\"ok\":true}' for r in records)}),flush=True)
