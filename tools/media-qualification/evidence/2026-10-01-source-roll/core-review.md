# Independent Roll core review

Reviewed the frozen `core-roll.patch` (SHA256 `063b3c7678459a53529c086457b2136db581c34fda104274e6f731c07e9fb088`), the staged `work/` sources and tests, `next-roll-design.md`, and `roll-implementation-readiness.md`. I also checked unchanged callers that own the outer command transaction, editorial-neighbor marking, and root-sound capture.

**Findings: none.** The common clamp intersects the two exact edge intervals before converting to whole-frame bounds and gives exclusive ties priority. Both candidates resolve from the same pre-edit document; the reducer captures bindings once, installs both sides, marks the changed seam once, transforms marks once, and validates the final unchanged project duration. Prefix handling is confined to the right In expansion, with the physical binding, owner framing/treatments, and source-local marks rebased together. Fixed-duration root sounds and allowances are detached/restored by the outer transaction. The historical command adapters explicitly refuse Roll, and the new tests cover fractional/picture/output clamps, direct and retained views, prefix/binding/mark behavior, root sounds, metadata rejection, overflow, and wire closure.

I did not run Cargo or native tests, as requested. The independent review is static; the root-owned focused and workspace gates remain the execution evidence.
