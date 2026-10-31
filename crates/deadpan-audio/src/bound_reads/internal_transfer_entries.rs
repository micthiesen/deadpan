//! Additive OLD public full-Root and physical-Domain transfer entry observations.
use super::*;
use crate::{
    DomainSignalTransfer, RootSignalTransfer, TransferredDomainBlock, TransferredRootBlock,
};

const TRANSFER_CHECKPOINT: &str = "e8fcdb3cb02f9682bef53f6ba2709e8a5ad692d0";
const PRIOR_INTERNAL_SHA256: &str =
    "e81a3ecd80ad0155e754e7a708e14efd6f045613deab797154ef0999b266e076";
const START: SignalSample = SignalSample(400);
const FRAMES: u32 = 32;

#[derive(Clone, Copy)]
enum Entry {
    FullRoot,
    PhysicalDomain,
}
impl Entry {
    fn name(self) -> &'static str {
        match self {
            Self::FullRoot => "StageAudio::read_transferred",
            Self::PhysicalDomain => "StageAudio::read_domain_transferred",
        }
    }
}
#[expect(
    clippy::large_enum_variant,
    reason = "Keep the observed public descriptors unboxed"
)]
enum Transfer<'plan> {
    Root(RootSignalTransfer),
    Domain(DomainSignalTransfer<'plan>),
}
#[expect(
    clippy::large_enum_variant,
    reason = "Keep the observed public outputs unboxed"
)]
enum Output {
    Root(Result<TransferredRootBlock, StageAudioError>),
    Domain(Result<TransferredDomainBlock, StageAudioError>),
}
fn result(value: &Output) -> Value {
    match value {
        Output::Root(Ok(value)) => {
            let TransferredRootBlock {
                schema_version,
                stage,
                project_id,
                revision_id,
                transfer,
                start,
                samples,
                suppressed,
            } = value;
            json!({"status":"Ok","kind":"TransferredRootBlock",
                "schema_version":schema_version,"stage":stage,"project_id":project_id,
                "revision_id":revision_id,"transfer":transfer,"start":start.0,
                "f32_bits":samples.iter().map(|sample|sample.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":layout::<[f32;2]>(samples.capacity()),
                "suppressed":mask(suppressed.as_slice(),suppressed.capacity(),|at:SignalSample|at.0)})
        }
        Output::Domain(Ok(value)) => {
            let TransferredDomainBlock {
                schema_version,
                stage,
                project_id,
                revision_id,
                definition,
                placement,
                instance,
                gap_after,
                transfer,
                start,
                samples,
                suppressed,
            } = value;
            json!({"status":"Ok","kind":"TransferredDomainBlock",
                "schema_version":schema_version,"stage":stage,"project_id":project_id,
                "revision_id":revision_id,"definition":definition,"placement":placement,
                "instance":instance,"gap_after":gap_after,"transfer":transfer,"start":start.0,
                "f32_bits":samples.iter().map(|sample|sample.map(f32::to_bits)).collect::<Vec<_>>(),
                "sample_storage":layout::<[f32;2]>(samples.capacity()),
                "suppressed":mask(suppressed.as_slice(),suppressed.capacity(),|at:SignalSample|at.0)})
        }
        Output::Root(Err(value)) => {
            json!({"status":"Error","kind":"TransferredRootBlock","error":error(value)})
        }
        Output::Domain(Err(value)) => {
            json!({"status":"Error","kind":"TransferredDomainBlock","error":error(value)})
        }
    }
}
fn ratio_result(value: Result<ExactRatio, deadpan_core::TimeError>) -> Value {
    match value {
        Ok(value) => json!({"status":"Ok","value":value}),
        Err(value) => json!({"status":"Error","error":error(&value.into())}),
    }
}
fn transfer<'a>(
    plan: &'a RenderPlan,
    entry: Entry,
    root_at_anchor: i64,
) -> Result<Transfer<'a>, StageAudioError> {
    let output = START..SignalSample(START.0 + i64::from(FRAMES));
    match entry {
        Entry::FullRoot => Ok(Transfer::Root(RootSignalTransfer::new(
            AudioSample(0)..plan.audio_duration()?,
            ExactRatio::integer(root_at_anchor),
            START,
            ExactRatio::ONE,
            output,
        )?)),
        Entry::PhysicalDomain => {
            // The Source's padding is its own silent physical domain. Select
            // its real source domain while keeping the transfer's old anchor.
            let lookup = root_at_anchor + if root_at_anchor == 0 { 0 } else { 16 };
            let domain = plan.audio_domain_at(AudioSample(lookup), Default::default())?;
            Ok(Transfer::Domain(DomainSignalTransfer::new(
                domain,
                ExactRatio::integer(root_at_anchor),
                START,
                ExactRatio::ONE,
                output,
            )?))
        }
    }
}
fn transfer_value(value: &Transfer<'_>, plan: &RenderPlan) -> Value {
    match value {
        Transfer::Root(value) => json!({"kind":"RootSignalTransfer","serialized":value,
            "root_support":value.root_support(),"output":value.output(),
            "root_position_at_requested_start":ratio_result(value.root_position(START)),
            "scope":"public descriptor; full Root support is checked by the actual public reader"}),
        Transfer::Domain(value) => {
            let domain = value.domain();
            json!({"kind":"DomainSignalTransfer","descriptor":value.descriptor(),
                "root_position_at_requested_start":ratio_result(value.root_position(START)),
                "domain":{"belongs_to_expected_plan":domain.belongs_to(plan),
                    "definition":domain.definition(),"placement":domain.placement(),
                    "root_samples":domain.root_samples(),"root_extent":domain.root_extent(),
                    "visible_samples":domain.visible_samples(),"instance":domain.instance(),
                    "gap_after":domain.gap_after()},
                "scope":"actual public audio_domain_at and DomainSignalTransfer::new; no private replacement"})
        }
    }
}
struct TransferProvider {
    prepared: crate::PreparedSource,
    account_at_entry: Option<point_suppression::Observer>,
    calls: usize,
    events: Vec<Value>,
}
impl AudioSourceProvider for TransferProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        self.calls += 1;
        self.events.push(json!({"ordinal":self.calls,"project_id":project,
            "revision_id":revision,"asset":asset,
            "cancelled_before":cancelled.load(Ordering::Relaxed),
            "outcome":"PreparedSource","prepared_provenance":self.prepared.provenance(),
            "prepared_index":self.prepared.index(),"matrix_layout":self.prepared.matrix_layout(),
            "receipts":receipts(),"account_observer_from_public_entry":observer(&self.account_at_entry)}));
        Ok(&self.prepared)
    }
}
fn case(entry: Entry, root_at_anchor: i64, name: &'static str) -> Value {
    let _session = TraceSession;
    let checked = quiet_and_padded();
    let mut row = json!({"name":name,"entrypoint":entry.name(),
        "checked_document_wire":checked.wire,"historical_document":checked.history,
        "wav":cases::wav_descriptor(cases::Wav::Stereo48000),
        "request":{"start":START.0,"frames":FRAMES,"root_at_anchor":root_at_anchor,
            "signal_anchor":START.0,"root_samples_per_signal_sample":ExactRatio::ONE,
            "timeout_seconds":TIMEOUT.as_secs()},
        "intent":if root_at_anchor==0 {"checked SilentHold window"}
            else {"checked Source SelectedPlacement16..48 inside64-frame allocation"},
        "scope":{"public_budget":"actual method owns inaccessible local ReadWork; no surrogate ledger",
            "callback_account":"entry observer is optional; None is unavailable, never zero or proof of no later account",
            "coverage":"actual result and scalar callbacks decide masks and reached branches; no synthetic ranges",
            "setup":"all runtime public factories remain fallible and errors are recorded"}});
    let document = match checked.document {
        Ok(value) => value,
        Err(value) => {
            row["setup"] = json!({"status":"DocumentValidationError",
                "error":error(&StageAudioError::from(PlanError::from(value))),
                "entrypoint_not_called":true});
            return row;
        }
    };
    row["document_before"] = serde_json::to_value(&document).unwrap();
    let plan = match RenderPlan::compile(&document) {
        Ok(value) => Arc::new(value),
        Err(value) => {
            row["setup"] = json!({"status":"CompileError","error":error(&value.into()),
                "entrypoint_not_called":true});
            row["document_after"] = serde_json::to_value(&document).unwrap();
            assert_eq!(row["document_before"], row["document_after"]);
            return row;
        }
    };
    let transfer = match transfer(&plan, entry, root_at_anchor) {
        Ok(value) => value,
        Err(value) => {
            row["setup"] = json!({"status":"PublicTransferFactoryError",
                "error":error(&value),"entrypoint_not_called":true});
            row["document_after"] = serde_json::to_value(&document).unwrap();
            assert_eq!(row["document_before"], row["document_after"]);
            return row;
        }
    };
    row["setup"] = json!({"status":"Ready","plan":plan_identity(&plan),
        "transfer":transfer_value(&transfer,&plan)});
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let entry_observer = renderer.owned_suppression.observer();
    let mut provider = TransferProvider {
        prepared: cases::prepared(cases::Wav::Stereo48000),
        account_at_entry: entry_observer.clone(),
        calls: 0,
        events: Vec::new(),
    };
    let cancelled = AtomicBool::new(false);
    reset_receipts();
    row["controller_before_public_entry"] = controller(&renderer);
    row["account_observer_before_public_entry"] = observer(&entry_observer);
    row["receipts_before_public_entry"] = receipts();
    let output = match &transfer {
        Transfer::Root(value) => Output::Root(renderer.read_transferred(
            &mut provider,
            value,
            START,
            FRAMES,
            TIMEOUT,
            &cancelled,
        )),
        Transfer::Domain(value) => Output::Domain(renderer.read_domain_transferred(
            &mut provider,
            value,
            START,
            FRAMES,
            TIMEOUT,
            &cancelled,
        )),
    };
    row["result_after_public_method_local_work_drop"] = result(&output);
    row["controller_result_alive"] = controller(&renderer);
    row["provider_calls"] = json!(provider.calls);
    row["provider_events"] = json!(provider.events);
    row["cancelled_at_end"] = json!(cancelled.load(Ordering::Relaxed));
    row["receipts_after_public_entry"] = receipts();
    let account = renderer.owned_suppression.observer();
    row["account_after_public_entry"] = observer(&account);
    drop(provider);
    row["result_after_provider_drop"] = result(&output);
    assert_eq!(
        row["result_after_public_method_local_work_drop"],
        row["result_after_provider_drop"]
    );
    row["transfer_after_provider_drop"] = transfer_value(&transfer, &plan);
    assert_eq!(
        row["setup"]["transfer"],
        row["transfer_after_provider_drop"]
    );
    drop(entry_observer);
    drop(renderer);
    row["result_after_controller_drop"] = result(&output);
    assert_eq!(
        row["result_after_public_method_local_work_drop"],
        row["result_after_controller_drop"]
    );
    row["account_after_controller_drop_result_alive"] = observer(&account);
    drop(output);
    row["account_after_public_result_drop"] = observer(&account);
    row["final_receipts"] = receipts();
    drop(account);
    row["account_observer_dropped"] = json!(true);
    row["document_after"] = serde_json::to_value(&document).unwrap();
    assert_eq!(row["document_before"], row["document_after"]);
    row
}
fn matrix_transfer_entries() -> Value {
    crate::stages::internal_mask_request::with_legacy_internal_masks(
        matrix_transfer_entries_legacy_body,
    )
}
fn matrix_transfer_entries_legacy_body() -> Value {
    json!({"schema":"old-internal-public-transfer-entries-v1",
        "published_checkpoint":TRANSFER_CHECKPOINT,
        "prior_internal_suppression":{"path":"tests/fixtures/internal-suppression-paths-baseline-v1/snapshot.json",
            "sha256":PRIOR_INTERNAL_SHA256},
        "actual_layouts":{"TransferredRootBlock":structure::<TransferredRootBlock>(),
            "TransferredDomainBlock":structure::<TransferredDomainBlock>(),
            "RootSignalBlock":structure::<crate::RootSignalBlock>(),
            "TransferredSignalBlock":structure::<crate::TransferredSignalBlock>(),
            "Vec_Signal_Range":structure::<Vec<Range<SignalSample>>>(),
            "Signal_Range":structure::<Range<SignalSample>>()},
        "cases":[
            case(Entry::FullRoot,0,"public-full-root-silent-hold"),
            case(Entry::FullRoot,64,"public-full-root-selected-source-padding"),
            case(Entry::PhysicalDomain,0,"public-physical-domain-silent-hold"),
            case(Entry::PhysicalDomain,64,"public-physical-domain-selected-source-padding")
        ],
        "missing_cases":[{"name":"routed-retained-label-map",
            "status":"NotImplementedByThisObserver",
            "reason":"requires separate public AudioRoutedRoot input/route or owner-retained factory construction; no private surrogate added"}],
        "scope":"additive actual public transfer entrypoints before internal ownership; no full preparation or routed-retained coverage claim"})
}
fn strip_transfer_identity(mut value: Value) -> Value {
    for field in ["variant", "source_sha256"] {
        value.as_object_mut().unwrap().remove(field);
    }
    value
}

pub(super) fn current_owned_transfer_observations() -> Value {
    let cases = [
        (Entry::FullRoot, 0, "public-full-root-silent-hold"),
        (
            Entry::FullRoot,
            64,
            "public-full-root-selected-source-padding",
        ),
        (
            Entry::PhysicalDomain,
            0,
            "public-physical-domain-silent-hold",
        ),
        (
            Entry::PhysicalDomain,
            64,
            "public-physical-domain-selected-source-padding",
        ),
    ]
    .into_iter()
    .map(|(entry, anchor, name)| {
        point_suppression::ops::trace::reset();
        let mut row = case(entry, anchor, name);
        row["native_mask_work_whole_case"] =
            serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
        point_suppression::ops::trace::disable();
        row
    })
    .collect::<Vec<_>>();
    json!({
        "schema":"active-internal-public-transfer-entries-v1",
        "cases":cases,
        "scope":"Four actual public method calls, real account and output lifetimes; native work is the complete case, with no per-nested-query attribution"
    })
}
#[test]
#[ignore = "manual additive OLD public transfer capture; pinned source and fresh/tmp output"]
fn capture_old_internal_transfer_entries_v1() {
    let output = std::env::var("DEADPAN_INTERNAL_TRANSFER_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant =
        std::env::var("DEADPAN_INTERNAL_TRANSFER_VARIANT").expect("actual source variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_INTERNAL_TRANSFER_SOURCE_SHA256")
        .expect("actual complete source manifest");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_INTERNAL_TRANSFER_PUBLISHED_CHECKPOINT")
        .expect("published checkpoint");
    assert_eq!(checkpoint, TRANSFER_CHECKPOINT);
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut body = matrix_transfer_entries();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &body).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
}
#[test]
#[ignore = "external independent complete OLD public transfer entry regression"]
fn internal_transfer_entries_match_external_old_v1() {
    let expected = std::env::var("DEADPAN_INTERNAL_TRANSFER_EXPECTED")
        .expect("independent immutable OLD transfer body");
    let body: Value = serde_json::from_slice(&std::fs::read(expected).unwrap()).unwrap();
    assert_eq!(matrix_transfer_entries(), strip_transfer_identity(body));
}
