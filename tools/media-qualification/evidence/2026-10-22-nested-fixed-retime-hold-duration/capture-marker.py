from pathlib import Path
import hashlib,json,subprocess,time
s=Path("/tmp/deadpan-nested-fixed-retime-duration-2026-10-22")
rows=json.loads((s/"first-focused-binaries.json").read_text())
row=next(r for r in rows if any("nested_fixed_retime_duration_preserves_fractional_input_phase_and_all_other_owners" in t for t in r["tests"]))
binary=Path(row["retained"]); test=next(t for t in row["tests"] if "nested_fixed_retime_duration_preserves_fractional_input_phase_and_all_other_owners" in t)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==row["sha256"]
start=time.monotonic()
result=subprocess.run([str(binary),test,"--exact","--nocapture"],capture_output=True,timeout=30)
elapsed=time.monotonic()-start
(s/"capture-marker.stdout").write_bytes(result.stdout); (s/"capture-marker.stderr").write_bytes(result.stderr)
report={"stage":"first focused audio marker","binary":str(binary),"sha256":row["sha256"],"test":test,"exit_code":result.returncode,"elapsed_seconds":elapsed,"captured_eof":True,"cause":"unknown; no production workaround","original_log":"first-focused.log"}
(s/"capture-marker.json").write_text(json.dumps(report,indent=2)+"\n")
assert result.returncode==0,report
print(json.dumps(report))
