//! Durable copy writes and historical, media-free register restoration.

use deadpan_core::{FrameRange, ProjectFrame, SliceCaptureSelection};
use deadpan_store::registers::{RegisterName, RegisterValue};

use super::*;
use crate::project::registers::{Bank, OriginalRequest, OriginalUpdate, Value};
use crate::project::slice::{CaptureRequest, Captured, CopyId};

impl Service {
    /// Only current interactive selections may write registers. Historical
    /// copies remain valid for placement through the read-only capture path.
    pub(super) fn check_register_request(&self, id: &CopyId) -> Result<()> {
        self.check_copy_context(id)?;
        self.check_context(id.session, &id.source_revision)?;
        if id.persisted_version.is_some() {
            return Err("Restored copy identities cannot issue a new capture".into());
        }
        Ok(())
    }

    pub(super) fn capture_original_command(&mut self, request: OriginalRequest) {
        let result = self.save_original(&request);
        self.captured_original = Some(OriginalUpdate {
            id: request.id,
            result,
        });
    }

    fn save_original(&mut self, request: &OriginalRequest) -> Result<()> {
        self.check_register_request(&request.id)?;
        let name = name(request.register)?;
        let mut bank = self.prepare_register_write(
            name,
            Value::Original {
                asset: request.asset.clone(),
                qualification: request.qualification.clone(),
                ordinals: request.ordinals.clone(),
            },
        )?;
        let saved = self
            .writer()?
            .save_register(
                &request.id.project,
                &request.id.source_revision,
                name,
                RegisterValue::Original {
                    revision: request.id.source_revision.clone(),
                    asset: request.asset.clone(),
                    qualification: request.qualification.clone(),
                    ordinals: request.ordinals.clone(),
                },
            )
            .map_err(display)?;
        // Everything fallible, including the derived runtime value, precedes
        // durable success. A saved copy must never be reported as a failed one.
        bank.version = saved.version;
        self.registers = Some(Arc::new(bank));
        Ok(())
    }

    pub(super) fn save_edit_slice(&mut self, request: &CaptureRequest) -> Result<Arc<Captured>> {
        self.check_register_request(&request.id)?;
        let name = name(request.register)?;
        let copied = self.capture_edit_slice(request)?;
        let mut bank = self.prepare_register_write(name, Value::Edited(copied.clone()))?;
        let saved = self
            .writer()?
            .save_register(
                &request.id.project,
                &request.id.source_revision,
                name,
                RegisterValue::Edited {
                    slice: copied.slice().clone(),
                },
            )
            .map_err(display)?;
        bank.version = saved.version;
        self.registers = Some(Arc::new(bank));
        Ok(copied)
    }

    pub(super) fn prepare_register_write(
        &mut self,
        name: RegisterName,
        value: Value,
    ) -> Result<Bank> {
        self.refresh_registers()?;
        let mut bank = self
            .registers
            .as_deref()
            .ok_or("No project register bank is open")?
            .clone();
        bank.entries.insert('"', value.clone());
        bank.entries.insert(name.as_char(), value);
        Ok(bank)
    }

    pub(super) fn refresh_registers(&mut self) -> Result<()> {
        let store = self.store.as_ref().ok_or("No project is open")?;
        let version = store.register_version().map_err(display)?;
        if self
            .registers
            .as_ref()
            .is_some_and(|bank| bank.session == self.session && bank.version == version)
        {
            return Ok(());
        }
        self.registers = Some(restore(store, self.session)?);
        Ok(())
    }
}

pub(super) fn name(selected: Option<char>) -> Result<RegisterName> {
    selected
        .map_or(Ok(RegisterName::unnamed()), RegisterName::new)
        .map_err(display)
}

/// Called on the project worker before a candidate session replaces its owner.
/// Loading copies neither qualifies media nor manufactures preview handles.
pub(super) fn restore(store: &ProjectStore, session: u64) -> Result<Arc<Bank>> {
    let stored = store.registers().map_err(display)?;
    let current = store.snapshot().map_err(display)?;
    let project = current.project_id().clone();
    drop(current);
    let mut bank = Bank {
        session,
        project: project.clone(),
        version: stored.version,
        entries: BTreeMap::new(),
    };
    let mut restored: Vec<(Arc<RegisterValue>, Value)> = Vec::new();
    for (name, value) in stored.entries {
        let runtime = if let Some((_, runtime)) =
            restored.iter().find(|(seen, _)| Arc::ptr_eq(seen, &value))
        {
            runtime.clone()
        } else {
            let request = u64::try_from(restored.len())
                .map_err(display)?
                .checked_add(1)
                .ok_or("Restored copy identity exhausted")?;
            let runtime = match value.as_ref() {
                RegisterValue::Original {
                    asset,
                    qualification,
                    ordinals,
                    ..
                } => Value::Original {
                    asset: asset.clone(),
                    qualification: qualification.clone(),
                    ordinals: ordinals.clone(),
                },
                RegisterValue::Edited { slice } => {
                    let document = store.snapshot_at(slice.revision_id()).map_err(display)?;
                    let plan = RenderPlan::compile(&document).map_err(display)?;
                    let scope =
                        SequenceScope::from_historical_parent(&document, &plan, slice.parent())?;
                    let owner = scope.resolve_document(&document, &plan)?;
                    let bounds = FrameRange::new(
                        ProjectFrame(i64::try_from(owner.start).map_err(display)?),
                        ProjectFrame(i64::try_from(owner.end).map_err(display)?),
                    )
                    .map_err(display)?;
                    let source_path = std::iter::once("Your edit".to_owned())
                        .chain(
                            scope
                                .groups()
                                .iter()
                                .map(|id| document.nodes()[id].label.clone()),
                        )
                        .collect();
                    let child_label = match slice.selection() {
                        SliceCaptureSelection::Range { .. } => None,
                        SliceCaptureSelection::Child { node } => {
                            Some(document.nodes()[node].label.clone())
                        }
                    };
                    Value::Edited(Arc::new(Captured {
                        id: CopyId {
                            session,
                            project: project.clone(),
                            source_revision: slice.revision_id().clone(),
                            request,
                            persisted_version: Some(stored.version),
                        },
                        scope,
                        slice: slice.clone(),
                        bounds,
                        source_path,
                        child_label,
                    }))
                }
            };
            restored.push((value, runtime.clone()));
            runtime
        };
        bank.entries.insert(name.as_char(), runtime);
    }
    Ok(Arc::new(bank))
}
