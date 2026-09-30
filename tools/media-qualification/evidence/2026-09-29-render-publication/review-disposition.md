# Publication review disposition

The retained host review is an earlier source snapshot. Its diagnostic-code finding
was corrected through typed provenance codes and preservation of filesystem errors
inside controlled I/O. The final filesystem review independently checked this fix.
The preliminary outcome name was replaced by `PublishedUnconfirmed` before shipping.

The filesystem review found a content-change window across rename's legitimate
ctime update. Published movie/report descriptors are now hashed against their
trusted identities, then their final entries are confirmed. The restored-mtime
regression exercises the real host hash helper and passes with
`published_hash_mismatch`. Failure preserves the committed file and returns an
unconfirmed publication outcome. The independent reviewer accepted this correction.

The provenance review found no actionable issue. The final documentation review
preserved the difference between library publication and a native Render workflow.
Durable jobs/recovery and automatic encoder policy remain open.

Parent-owned execution passed all 26 publication tests. Seven fresh project
movies passed real Metal/encoding, isolated verification and destination publication.
Independent readers inspected the final published filenames and passed complete
picture/PCM comparisons. A separate audit reopened every movie/report, checked
the receipt identities, reconstructed historical document hashes from SQLite,
compared Original receipt references and hashed retained Original bytes, and
checked the exact effective Generated interval. Full check commands and source
bindings are retained separately in the journals and summary.

No native C implementation, app UI, shader, audio DSP or packaging code changed.
No sanitizer rerun or GUI check is claimed for this Rust publication increment.
