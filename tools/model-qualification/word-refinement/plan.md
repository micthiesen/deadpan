# Local word refinement qualification

This is backend qualification for spec §11.2, not evidence that the application
already implements refinement. DP-10.3 remains unimplemented until the qualified
backend, bounded Original preparation, guarded proposals, explicit acceptance,
native controls, correction history and shared consumers work together.

## Objective and adoption gates

Improve approximate word starts and ends in a short Original context without
changing lexical text, existing edits or pinned manual corrections. Return
proposals with explicit unavailable/refused edges. A posterior is not a
probability that the proposed boundary is correct.

The initial backend precision experiment uses these adoption gates:

- Useful positive coverage and separately measured onset/offset improvement
  over the current Whisper boundaries; retain difficult/refused cases in the
  denominators. Report signed/absolute median, tail errors and signal/support
  retention, not only scores on accepted examples.
- No confident offers on the critical silence, noise, clearly wrong text and
  unresolved missing/repeated-context cases. The same text can occur twice;
  a good forced-path score alone does not identify the intended occurrence.
- Exact clocks, bounded input/work/memory and deterministic preprocessing.
  Model frame spans remain approximate annotations, never sample-accuracy claims.
- A frozen candidate and measurement followed by a new untouched final split.
  Final candidate inference must be unable to read grading references; scoring
  runs outside that process. Merely naming a file held-out is insufficient.
- Actual production-worker, source preparation and app/CLI confirmation after
  backend qualification. Direct Python/Rust probes cannot establish app wiring.

## Existing evidence and decisions

Scratch: `/tmp/deadpan-word-refinement-2026-10-25`. Root runner receipts retain
actual commands, scoped source/binary hashes, streams, exit and duration.
Keep rejected variants and setup failures. Do not overwrite earlier candidates.

| Variant | Evidence | Decision |
| --- | --- | --- |
| Existing Whisper token intervals | Twenty real ordinary/DTW runs on ten synthetic cases; conventional intervals are unchanged by DTW | Baseline. Token landmarks are not word boundaries. |
| DTW plus acoustic separators | Frozen policy; training offered 22/46 edges, then held-out offered 0/76 | Reject. Do not tune against that held-out set. |
| 5M phoneme model with label priors, v1 | 29/48 novel words offered; offered onset median 19.81 ms, offset 19.44 ms. Missing repeated context still confidently offers four words | Reject policy for ambiguity safety. Timing improvement alone is insufficient. |
| Context-completeness guard, v2 | Fixed repeated-prefix failure; positive retention and foreign-energy targets fail | Reject under the original numerical gates; no automatic-precision claim. |

### Evaluation audit and product boundary

The v2 experiment subsequently fails its preregistered numerical targets:
65/126 positive-word offers, 65/91 prior offers retained (71.43% versus 80%),
and up to 1.4865% foreign-word energy versus 0.1%. Those results remain rejected
under that measurement. They are not retroactively passed. The guard fixes the
observed omitted repeated-prefix case, reducing its offers from four to zero.
The reported zero among 95 critical negative slots is not zero among all words
in negative contexts: 26 valid-neighbor offers remain among 146 slots.

An independent audit finds genuine approximate edge contamination but no
arithmetic error reversing those failures. It also distinguishes the five
removed offers in the known capital-A source mismatch from proven false
abstentions. Do not silently exclude that case or call all 26 lost offers false.

Spec §9.3/§11.2 deliberately requires approximate annotations, confidence,
waveform adjustment and proposals; it specifies none of the experimental
80% retention, 0.1% foreign-energy or 80 ms limits. Under §29.1, human phonetic
acceptability is an owner verification item. The next work therefore qualifies
an **explicit approximate proposal workflow**, using the frozen guarded backend
without further threshold tuning. This is a different product contract from
automatic precision and does not turn the failed numerical experiment into a
measured gain. Preserve its edge errors, abstentions and reference limits.

That workflow must show the old and suggested edges, approximation and reasons
for unavailable proposals, permit context audition and manual adjustment, and
require explicit acceptance. Never apply a background result or change an
existing cut. Preserve pinned manual timing and captured word/context identity.
Verify refusal/stale-result behavior through the actual native worker and app,
including the known repeated-context counterexample. Broad model accuracy and
listening judgments remain unproven. No runtime UI or completed refinement is
claimed by these backend measurements alone.

The observed first held-out set becomes development/validation data if it informs
v2. It is never relabeled as unseen. Use grouped phrases/voices and keep related
positive/negative cases together. At most two further coherent guard candidates
in this work block; reassess the method if they cannot retain useful coverage
while fixing context errors. No open-ended threshold search.

## Measurement limits

The initial ten cases contain 61 separately synthesized words. The first fresh
set has eight isolated phrases/48 novel words, two natural sentences and four
negatives. Known construction intervals and amplitude-threshold support are
different references. Neither is human phonetic ground truth. Natural sentences
have only utterance-edge references; their interior accuracy is unknown.

The isolated-word source can speak more than its input token: one capital A
render is recognized as capital A. Preserve that case and its mismatch; correcting
the measurement requires a separate version and comparable regrading, not deletion
to improve a score. Synthetic results do not establish conversational/noisy-speech
accuracy, real-person quality, or listening acceptance.

All inference here is local. External model inference services are not used.
Package downloads and every failed/retried attempt remain part of the experiment
record. Full application latency, peak worker memory and packaged relocation
still require measurement.

## Candidate provenance

The dedicated author model is the label-prior alpha 0.6 checkpoint at
`huangruizhe/audio` commit `a4749fe14f17b19c071fe5b20005575ec0509ca3`.
The repository is BSD-2-Clause; no separately explicit weight license/model card
was found. Retain that distinction in the pack's license evidence. Original CMUDict
data is used directly, without the GPL Python wrapper. The 19,217,986-byte ONNX
export has SHA-256
`640bdf5da6928ce096c8d4d9216fb0613dfc7310b42d9182bd64214fa4719cbb`.
Ten development paths, intervals and offer decisions match PyTorch; maximum
log-probability difference is 6.1035e-5. This does not yet qualify a native feature
extractor or a shipped model/runtime pack.

Primary references: [label-prior paper](https://arxiv.org/abs/2406.02560),
[pinned author source](https://github.com/huangruizhe/audio/tree/a4749fe14f17b19c071fe5b20005575ec0509ca3/examples/asr/librispeech_alignment),
[CMUDict license](https://github.com/cmusphinx/cmudict/blob/74790861f652b15e4ac49015a90074ad62a27690/LICENSE).
