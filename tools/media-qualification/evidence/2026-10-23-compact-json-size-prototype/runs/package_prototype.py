#!/usr/bin/env python3
"""Bounded evidence packager. Root may run only after measurement completion."""
from __future__ import annotations
import gzip, hashlib, json, re, shutil, tarfile
from pathlib import Path

S = Path("/tmp/deadpan-compact-json-size-2026-10-23")
Q = Path("/tmp/deadpan-reused-leaf-result-qualification-2026-10-23")
BASE = Path("/Users/michael/Code/deadpan")
DEST = BASE / "tools/media-qualification/evidence/2026-10-23-compact-json-size-prototype"
LIMIT = 100_000

def need(ok, msg):
    if not ok: raise SystemExit(msg)
def j(path):
    try: return json.loads(path.read_text())
    except Exception as e: raise SystemExit(f"invalid/missing JSON {path}: {e}")
def digest(path):
    h=hashlib.sha256()
    with path.open("rb") as f:
        for b in iter(lambda:f.read(1024*1024),b""): h.update(b)
    return h.hexdigest()
def write_payload(raw,dst,kind):
    dst.parent.mkdir(parents=True,exist_ok=True)
    if len(raw)>LIMIT and kind in {".json",".txt",".md",".log",".stdout",".stderr",".html"}:
        out=Path(str(dst)+".gz")
        with out.open("wb") as f:
            with gzip.GzipFile(filename="",mode="wb",fileobj=f,compresslevel=9,mtime=0) as g: g.write(raw)
        with gzip.open(out,"rb") as f: need(f.read()==raw,f"gzip mismatch: {dst}")
    else: dst.write_bytes(raw)
def copy(src,dst):
    write_payload(src.read_bytes(),dst,src.suffix)

def preflight():
    need(not DEST.exists(),"destination already exists; refusing overwrite")
    initial=j(S/"build-receipt.json")
    need(len(initial)==1 and initial[0]["name"]=="clone-debug-cache" and initial[0]["exit"]==-15,
         "expected recorded debug-cache SIGTERM interruption")
    interruption=j(S/"cache-copy-interruption.json")
    need(interruption.get("signal")=="SIGTERM" and interruption.get("product_test_started") is False,
         "debug-cache interruption metadata mismatch")
    receipt=j(S/"build-resumed-receipt.json")
    names=["clone-release-cache","format","core-count-tests","binding-tests","store-checkpoint-test","release-build"]
    need([r["name"] for r in receipt]==names and all(r.get("exit")==0 for r in receipt),
         "resumed build must have six successful rows in expected order")
    need('"done": true' in (S/"build-resumed-driver.stdout").read_text(),"resumed driver done marker missing")
    fb=j(S/"focused-binaries.json")
    need(fb["core-count-tests"]["selected_passed"]==2 and fb["binding-tests"]["selected_passed"]==9 and
         fb["store-checkpoint-test"]["selected_passed"]==1,"focused counts must be 2 + 9 + 1")
    logs={"core-count-tests":"2 passed; 0 failed", "binding-tests":"9 passed; 0 failed",
          "store-checkpoint-test":"1 passed; 0 failed"}
    for name,row in fb.items():
        for field in ("retained","executed"):
            f=Path(row[field])
            need(f.is_file() and digest(f)==row["sha256"],f"focused {field} executable hash mismatch: {name}")
        out=(S/(name+".stdout")).read_text()
        need(logs[name] in out,f"focused test stdout count mismatch: {name}")
    overlay=j(S/"formatted-overlay-audit.json")
    paths=overlay.get("changed_files",[])
    need(overlay.get("rust_files")==2050 and overlay.get("exact_rust_path_set_preserved") is True and
         overlay.get("changes_match_five_reviewed_draft_files") is True and len(paths)==5,
         "formatted overlay audit mismatch")
    source_map=j(S/"build-source-identities.json")
    live={str(p.relative_to(S/"source")):digest(p) for p in (S/"source").rglob("*.rs")}
    need(len(live)==2050 and live==source_map,"full frozen source map mismatch")
    inputs=j(S/"input-identities.json")
    need(len(inputs)==6 and all((S/"source"/path).is_file() and digest(S/"source"/path)==h for path,h in inputs.items()),
         "six input identities do not match frozen live source")
    with tarfile.open(S/"source.tar","r:") as archive:
        members=[m for m in archive.getmembers() if m.isfile()]
        for path,h in inputs.items():
            hits=[m for m in members if m.name.rstrip("/")==path]
            need(len(hits)==1,"archive input path not unique: "+path)
            f=archive.extractfile(hits[0]); need(f is not None,"cannot read archived input: "+path)
            need(hashlib.sha256(f.read()).hexdigest()==h,"archive input identity mismatch: "+path)
    draft=j(S/"draft-source-hashes.json")
    need(len(draft["files"])==5 and {r["path"] for r in draft["files"]}==set(paths),"draft map mismatch")
    base=j(S/"source-base.json")
    need(base["status"]=="uncompiled-unformatted-private-prototype","unexpected source base status")
    need(digest(S/"source.tar")==base["archive_sha256"],"frozen source archive SHA-256 mismatch")
    for manifest_path in (S/"binaries.json", Q/"release-native-binaries.json"):
        for name,row in j(manifest_path).items():
            exe=Path(row["executed"])
            need(exe.is_file() and digest(exe)==row["sha256"],f"executed app/helper hash mismatch: {manifest_path.name} {name}")
    return source_map,draft

def check_trials():
    mr=j(S/"measure-receipt.json")
    need(mr.get("exit")==0,"measurement receipt absent or nonzero")
    summary=j(S/"summary.json"); trials=j(S/"trials.json"); commands=j(S/"commands.json")
    need(len(trials)==10 and len(commands)==20 and all(c.get("exit")==0 for c in commands),
         "measurement requires ten trials and twenty successful commands")
    need(summary.get("n_per_kind")==5 and summary.get("prototype_only") is True and
         summary.get("no_percentile_or_target_pass") is True and set(summary.get("kinds",{}))=={"control","prototype"},
         "summary is incomplete or overclaims")
    need(sum(r["kind"]=="control" for r in trials)==5 and sum(r["kind"]=="prototype" for r in trials)==5,
         "need five trials of each kind")
    control=j(Q/"release-native-binaries.json")["deadpan-app"]["sha256"]
    prototype=j(S/"binaries.json")["deadpan-app"]["sha256"]
    expected={"control":"1864c3acd798d7ce68a8d3a62ee144ec6956cdc4750c568bf0001d7c2a67be4c",
              "prototype":"561c2072b910e42d9defe29a502e0d7fe7055d37e120d889663fb8a253d5ee18"}
    need(control==expected["control"] and prototype==expected["prototype"],"app binary identity differs")
    for r in trials:
        key=f'{r["pair"]}-{r["kind"]}'
        need(r.get("checks")==7 and r.get("source_and_binary_guards") is True and
             r.get("authored_inputs_unchanged") is True and r.get("empty_wal") is True,
             f"trial checks/guards failed: {key}")
        need(len(r.get("load",[]))==3 and r["load"][0]<=3.0,f"invalid load: {key}")
        need(r["binary_sha256"]==expected[r["kind"]],f"trial binary mismatch: {key}")
        before=j(S/f"{key}-input-before.json"); after=j(S/f"{key}-input-after.json")
        sidecars={"project.sqlite-shm","project.sqlite-wal"}
        norm=lambda m:{k:v for k,v in m.items() if Path(k).name not in sidecars}
        need(norm(before)==norm(after),f"input map changed beyond SQLite sidecars: {key}")
        need(before["project.sqlite"]["sha256"]==after["project.sqlite"]["sha256"],f"database changed: {key}")
        need(after.get("project.sqlite-wal",{}).get("bytes",0)==0,f"WAL nonempty: {key}")
        report=j(S/f"{key}-report"/"report.json")
        need(report.get("failed") is False and report["metadata"]["binary_sha256"]==expected[r["kind"]] and
             report["metadata"]["fixture_sha256"]==before["project.sqlite"]["sha256"]==after["project.sqlite"]["sha256"],
             f"report failed or binary/fixture identity mismatch: {key}")
        pause=[c for c in report.get("scenarios",[]) if c.get("name")=="large-first-pause"]
        kestrel=[c for c in report.get("scenarios",[]) if c.get("name")=="kestrel-shortcuts"]
        need(len(pause)==len(kestrel)==1 and len(pause[0].get("checks",[]))==7 and
             all(c.get("passed") is True for c in pause[0]["checks"]),f"pause checks failed: {key}")
        need(len(kestrel[0].get("checks",[]))==1 and all(c.get("passed") is True for c in kestrel[0]["checks"]),
             f"Kestrel check failed: {key}")
        need(len(pause[0].get("timings",[]))>=2,f"timing summaries missing: {key}")
    return trials,summary

def base_sources(draft):
    found={}
    with tarfile.open(S/"source.tar","r:") as t:
        members=[m for m in t.getmembers() if m.isfile()]
        for row in draft["files"]:
            matches=[m for m in members if m.name.rstrip("/")==row["path"]]
            need(len(matches)==1,f"source.tar path not unique: {row['path']}")
            f=t.extractfile(matches[0]); need(f is not None,"source archive read failed")
            data=f.read()
            need(len(data)==row["original_bytes"] and hashlib.sha256(data).hexdigest()==row["original_sha256"],
                 f"original source hash mismatch: {row['path']}")
            found[row["path"]]=data
    return found

def make_report(trials,summary):
    metrics=list(trials[0]["metrics"])
    rows=[]
    for m in metrics:
        c=summary["kinds"]["control"][m]["median"]; p=summary["kinds"]["prototype"][m]["median"]
        delta=c-p; pct=delta/c*100
        label="Input to commit" if "commit" in m else "Input to offscreen background composition"
        rows.append(f"| {label} | {c:.6f} ms | {p:.6f} ms | {delta:.6f} ms ({pct:.2f}%) |")
    return ("# Compact JSON size-counting prototype (private, unqualified)\n\n"
      "This five-file private experiment adds an exact compact-JSON size counter with Store entry, final, and uncaptured counts. "
      "Captured checkpoint bytes, ordering, and limits remain unchanged; the Store checkpoint regression checks those invariants. "
      "This is a prototype, not a qualified production change.\n\n"
      "## Checks\n\n"
      "- Resumed build: six successful commands. The initial debug-cache clone was stopped with SIGTERM after 422.7 seconds, before formatting, compilation, or tests; it is retained as an interruption, not a product failure.\n"
      "- Focused tests: 2 Core serialization tests, 9 binding tests, and 1 Store checkpoint test passed. Full Core/UI suites, strict Clippy, and full media/native qualification were not run for this prototype.\n"
      "- Timing: five fresh-project control/prototype pairs (ten trials); all seven pause checks and one Kestrel routing check passed in each report. All twenty commands succeeded, authored project DBs were unchanged, WAL size was zero, and per-trial load1 was at most 3.\n\n"
      "## Timing medians\n\n| Measure | Control | Prototype | Difference |\n|---|---:|---:|---:|\n"+ "\n".join(rows) + "\n\n"
      "These ten results are diagnostic medians, not p95 evidence or a target pass. The 100 ms fallback and 50 ms ordinary-edit targets remain open. No physical-display, source-decode, PCM, or listening performance claim is made.\n\n"
      "## Identity and scope\n\n"
      "- App SHA-256: prototype 561c2072b910e42d9defe29a502e0d7fe7055d37e120d889663fb8a253d5ee18; control 1864c3acd798d7ce68a8d3a62ee144ec6956cdc4750c568bf0001d7c2a67be4c.\n"
      "- Five changed Rust files are named in the formatted-overlay audit. The full 2,050-file Rust source identity map and six input identities are retained. Original sources come from the frozen archive; formatted tested sources are retained separately.\n"
      "- Core schema 58 and SQLite schema 87 are unchanged. Full product qualification remains open.\n\n"
      "See inventory.json for retained paths, byte count, and digest.\n")

def main():
    need(not DEST.exists(),"refusing to overwrite an existing output")
    source_map,draft=preflight()
    trials,summary=check_trials()
    originals=base_sources(draft)
    stage=DEST.with_name(DEST.name+".staging")
    need(not stage.exists(),f"staging path exists: {stage}")
    stage.mkdir(parents=True)
    try:
        # Bounded root-level records/scripts/logs; never copy source trees, DBs, targets, or executables.
        for src in sorted(S.iterdir()):
            is_trial_artifact = re.match(r"^[1-5]-(control|prototype)(-|\.)", src.name) is not None
            if src.is_file() and not is_trial_artifact and src.name not in {"source.tar","build-source-identities.json"} and src.suffix in {
                ".json",".py",".md",".txt",".patch",".stdout",".stderr",".log"}:
                copy(src,stage/"runs"/src.name)
        for src,dst in [
            (S/"build-source-identities.json",stage/"source/rust-source-identities.json"),
            (S/"input-identities.json",stage/"source/input-identities.json"),
            (S/"draft-source-hashes.json",stage/"source/draft-source-hashes.json"),
            (S/"source-base.json",stage/"source/source-base.json"),
            (S/"draft.patch",stage/"source/draft.patch"),
            (Q/"release-native-binaries.json",stage/"source/control-binary-manifest.json"),
            (S/"binaries.json",stage/"source/prototype-binary-manifest.json")]:
            copy(src,dst)
        for row in draft["files"]:
            path=row["path"]; live=S/"source"/path
            need(live.is_file() and digest(live)==source_map[path],f"formatted source mismatch: {path}")
            stem=path.replace("/","__")
            write_payload(live.read_bytes(),stage/"source"/(stem+".formatted.rs.txt"),".txt")
            write_payload(originals[path],stage/"source"/(stem+".original.rs.txt"),".txt")
        # Include all ten reports and each trial's fixture guards and streams; exclude project packages.
        for pair in range(1,6):
            for kind in ("control","prototype"):
                key=f"{pair}-{kind}"; report=S/f"{key}-report"
                for name in ("report.json","report.html"):
                    copy(report/name,stage/"reports"/key/name)
                for suffix in ("input-before.json","input-after.json","pre-run.json","make.stdout","make.stderr"):
                    src=S/f"{key}-{suffix}"; need(src.is_file(),f"missing trial evidence {src.name}")
                    copy(src,stage/"trials"/src.name)
                for suffix in ("stdout","stderr"):
                    src=S/(key+"."+suffix); need(src.is_file(),f"missing trial stream {src.name}")
                    copy(src,stage/"trials"/src.name)
        (stage/"README.md").write_text(make_report(trials,summary))
        # Preserve the exact measurement producer used for the ten trials.
        copy(S/"measure.py",stage/"scripts/measure.py")
        copy(S/"measure_after_build.py",stage/"scripts/measure_after_build.py")
        copy(S/"build_prototype_resumed.py",stage/"scripts/build_prototype_resumed.py")
        entries={}
        for f in sorted(stage.rglob("*")):
            if f.is_file() and f != stage/"inventory.json":
                entries[str(f.relative_to(stage))]={"bytes":f.stat().st_size,"sha256":digest(f)}
                if f.suffix==".gz":
                    with gzip.open(f,"rb") as g: g.read()
        inv={"files":[{"path":path,"bytes":meta["bytes"],"sha256":meta["sha256"]} for path,meta in entries.items()],
             "total_files":len(entries),"total_bytes":sum(v["bytes"] for v in entries.values())}
        (stage/"inventory.json").write_text(json.dumps(inv,indent=2)+"\n")
        check=j(stage/"inventory.json")
        need(check["total_files"]==len(check["files"]) and
             check["total_bytes"]==sum(v["bytes"] for v in check["files"]),"inventory totals failed")
        for item in check["files"]:
            f=stage/item["path"]
            need(f.is_file() and f.stat().st_size==item["bytes"] and digest(f)==item["sha256"],f"inventory reread failed: {item['path']}")
        # Report is installed last; do not leave a partial pack on any failure.
        inv_hash=digest(stage/"inventory.json")
        DEST.parent.mkdir(parents=True,exist_ok=True)
        stage.rename(DEST)
        print(json.dumps({"evidence":str(DEST),"total_files":inv["total_files"],
                          "total_bytes":inv["total_bytes"],"inventory_sha256":inv_hash},indent=2))
    except Exception:
        shutil.rmtree(stage,ignore_errors=True)
        raise

if __name__=="__main__": main()
