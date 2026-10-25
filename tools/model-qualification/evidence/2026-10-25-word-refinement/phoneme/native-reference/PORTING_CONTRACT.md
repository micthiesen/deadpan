# Frozen phoneme proposal backend port contract

This is deterministic implementation parity for an explicitly approximate, manually accepted annotation proposal. It does not turn the failed candidate-v2 evaluation into a pass or establish human phonetic accuracy. Authored corrections, cuts and the captured context remain independent. A refusal retains diagnostic raw intervals and changes no annotation.

## Source identities and reference material

- Base policy: ../align.py SHA256 bcc450c5f628b59b22db7f903ccd5d82d4c9f730908f1f81c6b9c1dbdb57e787.
- Guard: ../candidate-v2/guard.py SHA256 6c10053ba98f2b700f191473ddbebb915cf3c793d60a51c1507cd9e2ecf0bdc7, iteration 1 only.
- Original author alpha0.6 checkpoint: ../upstream/model-0.6.ckpt, 57,562,900 bytes, SHA256 4070c3b89fb27129e2999f550ddb55c747ec77da843fa8a520989b8c5d09723f.
- Author commit a4749fe14f17b19c071fe5b20005575ec0509ca3 in huangruizhe/audio. Exact source URLs, sizes, Git blob IDs and SHA256 values are in ../download-pins.json.
- Original CMUDict data commit 74790861f652b15e4ac49015a90074ad62a27690, ../upstream/cmudict.dict SHA256 81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22; phones SHA256 ffb588a5e55684723582c7256e1d2f9fadb130011392d9e59237c76e34c2cfd6.
- manifest.json indexes 24 oracle-free backend cases. goldens/case01 through case10 use saved PyTorch emissions; goldens/held01 through held14 use saved folded-ONNX emissions. All are already observed development/parity data. No truth, construction bounds or grader data is supplied to this backend.
- Each case contains input.json plus copied audio.f32le, features.f32le, emissions.f64le, raw-suggestion.json, guard-output.json and filtered-suggestion.json. Shapes and hashes are in manifest.json. These binary files have no headers and use little-endian contiguous row-major storage.
- algorithm/viterbi.json and matching binary emissions cover repeated phones, nonzero phone duration and exact ties. algorithm/normalization.json records exact lexical and pronunciation outcomes/refusals. These are algorithm checks, not acoustic validation.

## Input and bounds

One mono, 16,000 Hz float32 PCM context, more than 400 and at most 480,000 samples. Reject nonfinite samples and invalid format. Do not peak normalize, remove DC, dither, preemphasize, resample at another phase, time stretch, or concatenate extra context in this backend. The caller prepares an explicitly captured Original window and supplies the unchanged transcript text for it. Bounds refer to the already prepared mono context, not the entire Original.

All sample/frame indices and array dimensions are nonnegative integers, held in checked native integer types. Reference paths use int64; traceback choices use uint8; edit distances use int32. Feature and model input/output tensors are float32. Model log probabilities are widened elementwise to float64 before CTC, exp, mean, guard and ranking operations. Scores and confidence summaries are float64. Reject nonfinite model outputs or shape/length mismatches. There is no probability-of-correctness interpretation of any score.

## Exact deterministic feature pipeline

Use the constants/*.f32le files directly for cross-language parity. Hann is 400 floats; Mel filter is [201,80]; mean and inverse_stddev are each 80 floats. constants/manifest.json gives exact SHA256 values. They are structural preprocessing constants, not checkpoint learned weights.

1. TorchAudio 2.8.0 MelSpectrogram defaults, with sample_rate 16000, n_fft 400, win_length 400, hop_length 160, n_mels 80. Center true, reflect padding of 200 samples at each end, no extra pad, periodic Hann window, onesided FFT, normalized false, power 2. Reflection excludes the edge sample: x[-1] reflects to x[1], x[N] to x[N-2].
2. Frame k is centered at original sample k*160. Multiply the centered 400-sample window by the pinned Hann, take an unnormalized forward real FFT, then real^2+imag^2 for bins 0..200. No one-sided energy doubling. Frame count F=floor(N/160)+1.
3. Multiply each [201] power row by the pinned [201,80] Mel matrix. Original defaults are f_min 0, f_max 8000, HTK Mel scale, norm None. Reference case01 additionally stores power.f32le [201,F] and mel.f32le [F,80] before gain.
4. Multiply by gain=pow(10,0.05*(2*20*log10(32767))) in reference host float64, 1073676289.0000038; scalar promotion into float32 tensor rounds it to 1073676288.
5. Apply the author's two sequential in-place masks. First, where current value > e, replace it with natural log(current value). Second, recompute the mask from the MUTATED values and divide every current value <= e by e. Tensor e is float32 rounded from math.e. This is not equivalent to a conventional single-pass log-or-linear branch. Do not simplify it.
6. Subtract each pinned mean, then multiply pinned inverse_stddev in float32. Shape [1,F,80], feature_lengths [F] int64. Preserve operation order; do not fuse normalization into the model or use random speed perturbation. Author Val/Test code contains random 0.9/1.0/1.1 speed perturbation, which this frozen backend intentionally never invokes.

Native FFT and math libraries may have numerical differences. Report feature and emission error against the supplied goldens and require identical selected full paths, word sample intervals, refusal/offer decisions and guard decisions for these fixtures. Existing ONNX/PyTorch qualification allowed max log-probability error <0.001 and measured at most 0.00006103515625. That is an implementation comparison bound, not a timing-quality guarantee. No native feature tolerance has yet been measured.

## Model and clocks

The author constructor is tdnn_blstm_ctc_model_base(41), despite its name containing no recurrent layer in this configuration: TDNN Conv1d/ReLU/BatchNorm blocks (kernel5,stride2,dilation1; kernel3,stride1,dilation1 twice), then five hidden linear/ReLU/dropout layers with skip, then projection and log-softmax. Hidden size 640; eval mode disables dropout. BatchNorm affine false uses original running mean/variance. Exact ONNX is preferable to rebuilding these operators manually.

Features [1,F,80] float32 and feature_lengths [1] int64 produce log_probs [T,1,41] float32 and output_lengths [1] float32. Length is integer-valued despite its float type; validate before conversion. T=ceil(F/2). Slice to output_lengths before alignment and widen each log probability to float64.

The output step is 320 input samples, 20 ms. Frame zero is centered at context sample zero. Do not subtract a learned or guessed receptive-field offset. A phone state occupies [start_frame,end_frame), including all of its assigned frames. A word raw interval is [first_phone_start*320,min(N,last_phone_end*320)). Never use the next word start as a word end, and never infer endpoints from a single emission maximum.

## Dictionary and lexical correspondence

Label order is exactly manifest.json labels: blank 0, the first whitespace token of each nonempty cmudict.phones line in file order as IDs 1..39, then unknown ID40. Unknown has no allowed dictionary pronunciation.

Parse cmudict.dict in source order. Ignore empty lines and those starting with ;;;. Split whitespace; remove a trailing (digits) alternate suffix from the first token and Unicode-casefold it. For later tokens not starting with #, remove all digit characters; if resulting pronunciation is empty or ANY phone is unknown, skip the whole line. This mirrors the frozen parser exactly, including its handling of inline comments. Deduplicate identical unstressed pronunciations per word while preserving first occurrence order.

Supplied transcript: split on Unicode whitespace; Unicode-casefold each token, replace curly right apostrophe with ASCII apostrophe, retain only Unicode alphabetic characters and ASCII apostrophe. No NFC/NFKC transform. Punctuation inside a token is removed, not converted to a word separator: co-op becomes coop. A token that normalizes empty is refused; a missing dictionary word refuses the entire context. No G2P, fuzzy spelling, number expansion or recognition-based text replacement. This is separate from the production Whisper subword-to-word grouping upstream.

The Cartesian product of pronunciation alternatives must have cardinality <=128 BEFORE alignment; reject the whole context otherwise, never silently truncate it. Enumerate in dictionary order with the last word alternative varying fastest. Each alternative path may have at most 256 phones; reject if any enumerated path exceeds the bound. Empty target ultimately refuses CTC. Retain normalization.json vector behavior.

## CTC Viterbi and word gating

For U phones use S=2U+1 states: blank at even states, target phone at odd states. At t=0, initialize only state0 with logp[0,blank] and state1 with logp[0,firstphone]; others negative infinity. Per later frame, candidates are stay(s), advance(s-1), skip(s-2). Skip is legal only for a nonblank state whose phone differs from the previous phone. Equal target phones therefore require an intervening blank. Select the maximum score with exact tie order stay, advance, skip; add current state's emission. Trace choices uint8 0/1/2.

Finish at the final blank if its score >= final phone score, otherwise final phone. Reject nonfinite final score or any phone state with no occupied frame. Trace back through every frame. Each phone state's occupied range must be contiguous. For each phone compute arithmetic mean(exp(logp[t,phone])) and minimum posterior over its own occupied frames, in float64.

For pronunciation alternatives select the largest TOTAL path score, not average score. Exact ties retain the first enumerated pronunciation. Word posterior is the occupied-frame-weighted mean of its phone means; internal blank frames are excluded. Minimum_phone_mean is the minimum of phone MEANS, not the minimum individual-frame posterior. Refuse if word mean <0.5, else if minimum phone mean <0.1; empty clipped sample interval overrides either refusal reason. Threshold equality passes. Do not normalize the word energy.

Partial offers are intentional: a word passing the model gates may be proposed when other words in that context are refused, unless the context guard abstains. Retain every raw word interval and path for diagnostics even when refused. Only explicit caller acceptance can apply an eligible proposal.

## Iteration-1 context guard

Collapse consecutive equal argmax frame labels, with argmax ties selecting lowest label ID. Remove blank runs AFTER this collapse, so the same phone separated by blank remains two phone events. Each event has label, phone string, start_frame, end_frame, frames and float64 mean_posterior.

Compare observed phone IDs to every allowed pronunciation phone sequence by full Levenshtein minimum-unit-edit distance. DP initialization is ordinary insertion/deletion counts. At each cell choose diagonal(match/substitute), delete_expected, insert_observed in that tie order. Select the first pronunciation with minimum distance. This pronunciation may differ from the acoustically selected Viterbi pronunciation. Preserve both choices, do not silently substitute one.

Iteration 1 residuals use insert_observed ONLY. A phone qualifies for a residual run when its mean_posterior >=0.5 and its edit op is insert_observed. Every other observed op or an expected-phone deletion flushes the current run. Runs also require consecutive observed-event indices. Blanks are absent from this event list, and their gap duration does not count toward supported_frames. A run qualifies with >=3 events and sum(event.frames)>=6. Its calendar span can include blank gaps; do not confuse that span with its occupied-frame sum. These thresholds are heuristics, never calibrated correctness probabilities.

If any run qualifies, context status incomplete_or_placement_ambiguous, abstain true, and every word is refused with the context reason; original base_status and base_refusal remain separately recorded. Otherwise status no_large_supported_residual_detected; absence of this residual does not prove completeness or unique placement. No endpoints or base confidence values change.

Guard output fields: iteration(integer1), configuration(object), greedy_phones(array of event objects), selected_pronunciation_path(nonnegative index), selected_pronunciations(array of word phone arrays), expected_phones(array of strings), minimum_edit_distance(integer), alignment_steps(array), edit_counts(object), residual_runs(array), context_status(string), abstain(bool), reason(nullable string), confidence(string). Each alignment step contains op, expected_index/observed_index nullable integer, expected/observed nullable label ID. A residual includes observed_indices, phones, start_frame, end_frame, supported_frames, events, minimum_mean_posterior and qualifies. Scores are float64; coordinates/counts are integers.

## Known limitations preserved in parity

The old numerical evaluation remains FAILED. In its 44 diagnostic rows there were 0/95 designated critical negative slots after guarding, not 0/all146 word slots in negative contexts: 26 valid neighboring words retained offers. A substituted-word diagnostic designates only the substituted word as critical. Context-wide wrong transcript and repeated-phrase ambiguity designate all supplied words.

The guard leaves 65/126 positive word offers, retaining 65/91 original positive offers (71.43%, below the preregistered80%). It removes offers in six contexts. Case02 A contains undeclared spoken capital A, so its five removed offers are not proven false abstentions. Do not exclude or relabel this case to improve a score.

Case06 we retains 0.112067% foreign-word energy and safe 1.486482%, above the old0.1% gate. Safe starts122.375ms before the support reference. Case09 first No ends61.875ms early and retains94.59% own energy; threshold support coverage is lower. Dedicated-model frame bounds do not prove phonetic boundaries. Natural held09/10 have only utterance outer-boundary truth; interior word timing and isolated word energy are UNKNOWN. All observed results remain exploratory until native end-to-end qualification. Human phonetic acceptability/listening stays on the owner verification list under the standing scope decision.
