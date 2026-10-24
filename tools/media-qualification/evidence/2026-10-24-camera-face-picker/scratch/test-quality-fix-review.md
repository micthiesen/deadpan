# Camera face picker test-quality fix review

Reviewed the bounded follow-up changes in `preview/harness/faces/camera_picker.rs` and `preview/camera/face_picker.rs`.

The saved-target stability gap is addressed: the replay snapshots the existing `face-1` entry before waiting for proposals and verifies the same entry afterward, including ID, label, and digit. The page-numbering unit fixture now has actual saved-target entries and compares their identity and numbering across proposal append. The async receipt regression also checks that a later refusal/success for another request does not discard the matching successful face-save receipt, while a different saved target or refusal of the save itself clears the acceptance.

No remaining concrete test-quality findings in these bounded changes.
