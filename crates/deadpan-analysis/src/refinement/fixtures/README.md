# Deterministic phoneme proposal regression data

Observed backend outputs copied from the oracle-free native-reference export in
/tmp/deadpan-word-refinement-2026-10-25/phoneme/native-reference. These fixtures
contain model emissions and exploratory proposals, never timing truth or grader
data. The failed numerical qualification remains failed.

goldens.json retains the unchanged raw, guard iteration 1, and filtered words,
the input text/sample extent, and SHA-256 of each exact float64 emission tensor.
The 24 files are row-major [T,41] little-endian float64. No model is run by tests.
case01..10 use saved PyTorch output, held01..14 saved folded ONNX output.

Base align.py SHA256 bcc450c5f628b59b22db7f903ccd5d82d4c9f730908f1f81c6b9c1dbdb57e787.
Guard SHA256 6c10053ba98f2b700f191473ddbebb915cf3c793d60a51c1507cd9e2ecf0bdc7.
CMUDict subset preserves all source-order variants for every fixture word.
Original CMUDict commit 74790861f652b15e4ac49015a90074ad62a27690.
Original dictionary SHA256 81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22.
Phone-order SHA256 ffb588a5e55684723582c7256e1d2f9fadb130011392d9e59237c76e34c2cfd6.
See CMUDICT-LICENSE for the raw data license.
