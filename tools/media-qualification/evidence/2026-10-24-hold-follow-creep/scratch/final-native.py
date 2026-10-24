#!/usr/bin/env python3
"""Run final native regression scenarios sequentially with frozen binaries."""
import json, os, subprocess, sys
from pathlib import Path
s=Path(__file__).resolve().parent
app=s/'bin-final/deadpan-app'
env=dict(os.environ)
env['DEADPAN_REQUIRE_PHYSICAL_MAC']='1'
for scenario in ['hold-follow-creep','faces','targets','zoom','points']:
 label='native-'+('hold' if scenario=='hold-follow-creep' else scenario)+'-final'
 command=[sys.executable,str(s/'run.py'),'--binary',str(app),label,'--',str(app),'--ui-check','--scenario',scenario,'--output',str(s/label),'--retain-projects','--kestrel-source','/Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift']
 result=subprocess.run(command,env=env)
 if result.returncode:
  raise SystemExit(result.returncode)
 print(json.dumps({'completed':scenario}),flush=True)
