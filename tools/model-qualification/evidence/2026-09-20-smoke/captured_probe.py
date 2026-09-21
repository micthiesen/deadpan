import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import time
import traceback

started=time.monotonic()
run=Path(sys.argv[1])
(run/'pid').write_text(str(os.getpid()))
cache=Path('/tmp/deadpan-model-qualification-cache')
corpus=Path('/tmp/deadpan-hold-corpus/tears-of-steel')
manifest=json.loads((cache/'manifest.json').read_text())
results=json.loads((cache/'download-results.json').read_text())['results']
assert len(results)==12 and all(r['status'].startswith('verified') for r in results)
for asset in manifest['assets']:
    assert Path(asset['destination']).stat().st_size==asset['size']
for flag in ['HF_HUB_OFFLINE','TRANSFORMERS_OFFLINE','HF_HUB_DISABLE_TELEMETRY','PYTHONNOUSERSITE']:
    assert os.environ.get(flag)=='1',flag
report={
    'scope':'Single cold-process local keyframe smoke; OS file cache not flushed. Not a corpus bake-off, app worker integration, UI interference measurement, or shipped model choice.',
    'runtime_commit':manifest['runtime_commit'],
    'source':json.loads((corpus/'source-provenance.json').read_text()),
    'source_frames':[7632,7633],
    'input_sha256':{f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in [corpus/'smoke-endpoint-01.png',corpus/'smoke-endpoint-02.png']},
    'configuration':{'width':768,'height':320,'frames':25,'frame_rate':24,'seed':1,'stage1_steps':20,'stage2_steps':3,'cfg_scale':3.0,'low_memory':True,'low_ram_streaming':False,'generate_audio':False,'memory_guideline_bytes':64*1024**3,'cache_limit_bytes':8*1024**3},
    'prompt':'Locked camera. The person maintains the same identity, pose, expression, and composition, with quiet minimal natural motion. No speech, no new objects, no scene change.',
    'swap_before':subprocess.check_output(['sysctl','vm.swapusage'],text=True).strip(),
    'memory_before':subprocess.check_output(['memory_pressure','-Q'],text=True).strip(),
}
try:
    import mlx.core as mx
    import mlx_lm
    from ltx_core_mlx.components.guiders import MultiModalGuiderParams
    from ltx_core_mlx.text_encoders.gemma.encoders.base_encoder import GemmaLanguageModel
    from ltx_pipelines_mlx.keyframe_interpolation import KeyframeInterpolationPipeline
    # Explicit private adapter restriction; no model-supplied executable code.
    def load_local_gemma(self, model_path=None):
        path=model_path or self._model_path
        assert path and Path(path).is_dir()
        self._model,self._tokenizer=mlx_lm.load(path,tokenizer_config={'trust_remote_code':False,'local_files_only':True})
    GemmaLanguageModel.load=load_local_gemma
    report['device_info']=mx.device_info()
    report['previous_memory_guideline_bytes']=mx.set_memory_limit(64*1024**3)
    report['previous_cache_limit_bytes']=mx.set_cache_limit(8*1024**3)
    mx.reset_peak_memory()
    pipe=KeyframeInterpolationPipeline(
        model_dir=str(cache/'mlx_ltx_q4_pack'/'56a5866d638ecfe37c54d348e88938235185c2d4'),
        gemma_model_id=str(cache/'mlx_gemma_default_text_encoder'/'86cc6a8dedbc456dd0e4af01a9d09f396f77e558'),
        low_memory=True,low_ram_streaming=False,
        dev_transformer='transformer-dev.safetensors',
        distilled_lora='ltx-2.3-22b-distilled-lora-384.safetensors',
    )
    pipe.generate_audio=False
    pipe.verbose=True
    report['imports_setup_seconds']=time.monotonic()-started
    (run/'report.json').write_text(json.dumps(report|{'status':'running'},indent=2)+'\n')
    generation_started=time.monotonic()
    output=pipe.generate_and_save(
        prompt=report['prompt'],output_path=str(run/'candidate.mp4'),
        keyframe_images=[str(corpus/'smoke-endpoint-01.png'),str(corpus/'smoke-endpoint-02.png')],
        keyframe_indices=[0,24],keyframe_strengths=[1.0,1.0],
        height=320,width=768,num_frames=25,frame_rate=24,seed=1,
        stage1_steps=20,stage2_steps=3,cfg_scale=3.0,
        video_guider_params=MultiModalGuiderParams(cfg_scale=3.0,stg_scale=1.0,rescale_scale=0.7,modality_scale=3.0,stg_blocks=[28]),
        audio_guider_params=MultiModalGuiderParams(cfg_scale=7.0,stg_scale=1.0,rescale_scale=0.7,modality_scale=3.0,stg_blocks=[28]),
    )
    mx.synchronize()
    report.update(status='generated_unvalidated',generation_call_seconds=time.monotonic()-generation_started,output=output,peak_mlx_bytes=mx.get_peak_memory(),active_mlx_bytes=mx.get_active_memory(),cache_mlx_bytes=mx.get_cache_memory())
except Exception as error:
    report.update(status='failed',error=repr(error),traceback=traceback.format_exc())
    traceback.print_exc()
finally:
    report.update(process_seconds=time.monotonic()-started,ru_maxrss_native=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,swap_after=subprocess.check_output(['sysctl','vm.swapusage'],text=True).strip())
    (run/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report),flush=True)
if report['status']=='failed':
    sys.exit(1)
