# Independent source review

A read-only reviewer inspected the production layout, transport measurement, focus transitions and new replay. No Cargo or native app launches were delegated.

## Accepted findings

- Valid long configured Play/Loop paths could exceed a fixed transport reserve. The final implementation measures the same galleys used for buttons, status and context, and reserves all wrapped rows.
- Below-picture controls could change transport after the renderer had submitted a resized target. The production Play-release plus 2x-scale witness submitted 1200x282 although final layout required 1200x270. The first-pass guard now lets pointer, focused Enter/Space and pending accessibility Click input run before rendering.

## Harness corrections

- Filter enabled viewer tabs before inspecting geometry, because Your edit also names a disabled breadcrumb. Guard missing AccessKit rectangles before calling node.rect().
- The copied-register explanation can scroll below the compact inspector. Its state remains asserted; primary endpoint fields and selection actions must remain visible.
- :source can preserve Viewer focus. Explicitly focus Sources before asserting reverse-Tab from Sources to Sounds.

Final read-only follow-up found no remaining findings in the scoped layout/focus changes. It confirmed the first-pass input guard precedes render_picture, the measured galleys are reused for painting, and Sounds entry retains cursors, selected beat and picture. Root owns all runtime verification.

## Final image review correction

Root inspection of the initial passing full release replay found raw Original pixels stretched while the decoded frame was revoked and a different-height viewer retained the old GPU texture. The strengthened independent mesh witness fails: a 600x109 target is painted in a 600x135 rectangle instead of its centered uniform fit. Independent source review accepted fitting the registered target by its actual raster aspect inside the viewer, while keeping the intended canvas as the next render target. This preserves composed and raw textures without adding state. The generic mesh check and scoped replay now inspect submitted texture geometry, including actual painted height.

Final applied-source follow-up: no findings. The reviewer confirmed that retained textures use their raster aspect, new submissions keep the intended canvas input, and the harness checks actual target/mesh geometry and minimum painted height. Source review only; no launches.
