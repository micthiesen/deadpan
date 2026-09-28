# Process launch qualification

The [qualification record](../../docs/qualification/process-launch-2026-09-27.md)
describes the deterministic macOS pipe-inheritance regression, its cooperative
launch boundary, and the retained evidence format.

```sh
python3 tools/process-qualification/check_pipe_inheritance.py --output /tmp/deadpan-pipe-inheritance-evidence
```
