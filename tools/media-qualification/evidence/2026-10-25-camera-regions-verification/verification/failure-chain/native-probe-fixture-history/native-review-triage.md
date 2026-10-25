# Native review triage

The proposed rejection of all salient-object bounds outside [0,1] is refuted
by the primary SDK contract. No production geometry policy was changed.

Evidence: Xcode26.6 SDK26.5 Vision.framework/Headers/VNObservation.h, line263,
VNRectangleObservation discussion:
"All coordinates are normalized and the coordinates can be outside the image."

Exact header retained at native-review-VNObservation.h.txt.
SHA256 c7ee0eec0d151431341a651bdbe93e18f2e20bb800b3ecb4a94bf4923b81452e.
Original path:
/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk/System/Library/Frameworks/Vision.framework/Headers/VNObservation.h

The same retained SDK header describes VNSaliencyImageObservation.salientObjects
as an NSArray<VNRectangleObservation*>. The retained Apple salientObjects Markdown
does not impose tighter bounds. Root explicitly accepted this refutation and
instructed retaining intentional clipping.

Focused regression additions check partial overflow on all4 edges, clipping
before each quarter-turn rotation, exact valid [0,1] full-image edges, wholly
outside rectangles producing empty results in all rotations, and invalid
nonfinite/zero/negative geometry failing rather than being repaired.
Existing face clipping and confidence behavior are unchanged.

The separate runtime-engine identity review finding is being fixed by consuming
canonical jobs engine constants, with exact integration-test equality. No Cargo
was run before Root integration/allocation. Original Native sources are copied
under native-review-before and hashed in native-review-before.json.
