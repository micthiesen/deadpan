import sys,pathlib,hashlib,json,shutil
scratch=pathlib.Path('/tmp/deadpan-fixed-retime-owned-sound-2026-10-21')
name,path=sys.argv[1:]
p=pathlib.Path(path)
assert p.is_file()
out=scratch/('executed-'+name)
shutil.copyfile(p,out)
j={'path':str(out),'original_path':str(p),'sha256':hashlib.sha256(out.read_bytes()).hexdigest()}
(scratch/(name+'-binary.json')).write_text(json.dumps(j,indent=2)+'\n')
print(j)
