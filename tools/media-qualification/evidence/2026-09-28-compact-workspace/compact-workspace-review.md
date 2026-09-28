# Compact workspace review

Base checkpoint: f9b1a14, native gain editing. The parent owns all Cargo,
test and GPU execution. Initial implementation is reviewed in scratch before
application to the shared checkout.

The initial production design retains the empty panel's ID at zero height and
measures a distinct Sounds target beside the scrolling Beats breadcrumbs.
Eligibility is captured once per paint pass so Original-to-Sounds pointer
activation cannot emit the same focus target in two locations.

Independent review found a P2 in the initial draft: the eligibility mismatch
requested discard only after viewer rendering. Clicking Sounds from Original
could submit a target sized for the old empty panel before the pass was
discarded. Accepted correction: invalidate before picture submission, including
view changes from the viewer header and guaranteed late command closure.
The final patch and regression results must establish that boundary; this
review note alone does not claim the fix or layout measurements passed.

The parent narrowed that correction instead of moving modal rendering:
room-tone is an overlay, so compact empty-Sounds placement remains stable behind
it. Removing the unnecessary room-tone eligibility restriction avoids a
64-point background jump on modal closure. Gain and Camera exclusions remain.
Room-tone rendering keeps its existing order.

Independent follow-up source review is clean: the captured/current placement
comparison follows viewer header actions and precedes render_picture, whose
existing will_discard check rejects the superseded size. The command-close
pre-discard requires command_open and an actual command result; the existing
end-of-pass path closes that footer. No further concrete focus/click/clip
defect was found in the three production files. Runtime checks remain pending.

The first app run passed 296 unit tests and failed the two new CPU fixtures on
unhandled egui texture deltas, before any layout assertion failed. Explicit
cleanup of all three unused Context upload outputs preserved the assertions.
The next app run passed 298 unit tests and two headless integration tests.

Independent harness review found that retained-picture expectations incorrectly
covered viewer tabs and :sequence, whose established behavior clears display.
Those paths now require zero submissions in the actual transition frame,
cleared display/target with the new pending label, then settled exact
session/project/revision/view and final target dimensions. Sounds entry keeps
its separate retained-picture oracle. Review then identified an Original
canvas=None assumption; the corresponding 14.66-second failed visual run is
retained. The corrected oracle uses the same whole-viewer fallback as production.
Only that replay helper changed after the passing 300-test app run; the
continuation preserves those results and reruns compilation, lint and paint.

Required qualification includes actual clipped text and hit geometry, copied
range controls, playback preparation/play/pause/resume, nested group navigation,
empty-to-first-placement/undo, display scaling and default-size restoration.
Preserve the native pane router and one external-update read per outer frame.

The next sound-placement replay exposed an existing Browse lookup after the
new minimum-size checks had scrolled the catalog. Its accessibility rectangle
overlapped the File header while the painted label was clipped; the click
opened File. The retained failed capture proves the mismatch. The corrected
helper uses bounded real wheel input, waits for settled geometry, and requires
the complete enabled text and hit clips before activating Browse. Assertions
were strengthened, not relaxed.

A later three-scenario batch failed on eager `then_some(node.rect())` evaluation
for accessibility containers with no bounds. Lazy `then` is the only source
change between inventory 54722ed2 and final 78395620. Independent static review
found no other new eager rectangle conversion. Workspace and Gain passed in
the original batch and were preserved; the three affected scenarios were rerun.

Final sound-placement, room-tone and nested-pause visual runs pass 217, 221 and
73 checks respectively, each with the live 5,456-case Kestrel audit. The earlier
independent Workspace and Gain runs pass 81 and 266 checks plus their audits.
The normal app passes 263 unit tests and two headless integration tests on the
final source. Strict workspace/all-target Clippy passes both configurations.

The parent inspected the selected final minimum, default, native 2x, nested and
populated captures against the single-Original board. The copied Hold picture
is 143 points high; other exercised compact states retain 169 or 175 points.
Nested breadcrumbs remain intentionally scrollable, with visible parent
navigation and a separate fully painted empty Sounds entry. The populated list
retains its ordinary panel. These are scoped layout results, not full feature,
device, acoustic or physical input acceptance.

The final locked release workspace build passes in 448.612 seconds on source
783956201811134574d665523081dbae2be239bfaf067ecb134cfcf6847932dc.
The complete release performance replay passes all 2,323 checks in 27.969
seconds with no findings or failed/timed-out timing samples. Navigation to GPU
completion p95 is 6.699 ms, cached Repeat 9.721 ms, Hold fallback 11.917 ms and
10,000-beat navigation CPU 1.765 ms. All verification processes reached a
terminal outcome normally; completed independent results were retained.
