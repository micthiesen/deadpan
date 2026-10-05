//! Resolve `:cutaway` against the selected beat, the Edit range and a copied
//! Original moment into one cutaway list for the project service.
//!
//! A cutaway belongs to a Source or Hold. A Split fragment is a unity
//! Partition over its full Source, so the cutaway goes to that Source in its
//! own clock and stays on the same content through later splits and trims.

use super::*;
use crate::navigation::cutaway::CutawayInput;
use deadpan_core::{Cutaway, ExactSourceSpan, FrameRange, SourcePoint};

impl DeadpanApp {
    pub(super) fn cutaway_edit(
        &self,
        node: &NodeId,
        input: CutawayInput,
    ) -> Result<ProjectEdit, String> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
        let row = self
            .beat_rows
            .iter()
            .find(|row| &row.id == node)
            .ok_or("Select a beat in the current group first.")?;
        let (host_id, offset) = deadpan_core::cutaway_host(&workspace.document, node).ok_or(
            "Cutaways belong to a source or pause beat. Open a group with Enter and select one there.",
        )?;
        // The Edit range, or the whole beat, in the host's own clock.
        let host = match self.selected_edit_range() {
            Some(range) => {
                let (start, end) = (range.start().0 as u64, range.end().0 as u64);
                if start < row.start || end > row.start + row.frames || start == end {
                    return Err(
                        "Select a range inside one beat; a cutaway belongs to that beat.".into(),
                    );
                }
                (start - row.start, end - row.start)
            }
            None => (0, row.frames),
        };
        let range = FrameRange::new(
            ProjectFrame(host.0 as i64 + offset),
            ProjectFrame(host.1 as i64 + offset),
        )
        .map_err(|error| error.to_string())?;
        let existing = &workspace.document.nodes()[&host_id].cutaways;
        let overlaps = |cutaway: &Cutaway| {
            cutaway.range.start() < range.end() && range.start() < cutaway.range.end()
        };
        let mut cutaways: Vec<Cutaway> = existing.clone();
        match input {
            CutawayInput::Clear => {
                cutaways.retain(|cutaway| !overlaps(cutaway));
                if cutaways.len() == existing.len() {
                    return Err("There is no cutaway here to clear.".into());
                }
            }
            CutawayInput::Place { register, fit } => {
                let content = match register {
                    Some(name) => self
                        .copied
                        .entries()
                        .find(|(entry, _)| *entry == name)
                        .map(|(_, content)| content),
                    None => self.copied.selected_content(),
                }
                .ok_or("Copy a moment of the Original first: view the Original, select it with v and copy with y.")?;
                let copied::Content::Original(moment) = content else {
                    return Err("A cutaway shows a moment of the Original; this register holds edited content or a macro.".into());
                };
                if moment.identity.session != workspace.session {
                    return Err("That copy belongs to another project session.".into());
                }
                let index = workspace
                    .sources
                    .get(&moment.identity.asset)
                    .and_then(|source| source.video_index.as_deref())
                    .ok_or("The copied Original has no qualified picture.")?;
                let point = |ordinal: u64| -> Result<SourcePoint, String> {
                    let ticks = match usize::try_from(ordinal)
                        .ok()
                        .and_then(|ordinal| index.frames().get(ordinal))
                    {
                        Some(frame) => frame.pts,
                        None if ordinal as usize == index.frames().len() => index.terminal_end(),
                        None => return Err("The copied moment lies outside the Original.".into()),
                    };
                    Ok(SourcePoint {
                        ticks: deadpan_core::ExactRatio::integer(ticks),
                        time_base: index.time_base(),
                    })
                };
                let selection = ExactSourceSpan::new(
                    point(moment.ordinals.start)?,
                    point(moment.ordinals.end)?,
                )
                .map_err(|error| error.to_string())?;
                if cutaways.iter().any(overlaps) {
                    return Err(
                        "This range overlaps a cutaway; :cutaway clear removes it first.".into(),
                    );
                }
                let position =
                    cutaways.partition_point(|cutaway| cutaway.range.start() < range.start());
                cutaways.insert(
                    position,
                    Cutaway {
                        range,
                        asset: moment.identity.asset.clone(),
                        selection,
                        fit,
                        removed: false,
                    },
                );
            }
        }
        Ok(ProjectEdit::SetCutaways {
            node: node.clone(),
            host: host_id,
            cutaways,
        })
    }
}

impl DeadpanApp {
    /// Registers that hold an Original moment, with the default copy first.
    pub(super) fn cutaway_registers(&self) -> Vec<(char, String)> {
        self.copied
            .entries()
            .filter_map(|(name, content)| match content {
                copied::Content::Original(moment) => Some((
                    name,
                    format!("[{}..{})", moment.ordinals.start, moment.ordinals.end),
                )),
                _ => None,
            })
            .collect()
    }

    /// `,r`: open `:cutaway register=` on the register that will be used,
    /// listing every register that holds an Original moment.
    pub(super) fn pick_cutaway(&mut self, context: &egui::Context) {
        if self.view != View::Sequence {
            self.error = Some("Select a beat in Your edit, then pick a reaction with ,r.".into());
            return;
        }
        let registers = self.cutaway_registers();
        let Some(first) = registers.first() else {
            self.error = Some("No register holds an Original moment. View the Original, select it with v and copy it with y (\"r y names register r).".into());
            return;
        };
        let name = self
            .copied
            .selected()
            .filter(|name| registers.iter().any(|(entry, _)| entry == name))
            .unwrap_or(first.0);
        self.open_command(format!("cutaway register={name}"), context);
    }

    /// Command-entry guidance for `:cutaway`: the registers it can show.
    pub(super) fn cutaway_hint(&self) -> Option<String> {
        let verb = self.command.trim_start().trim_start_matches(':');
        if !verb
            .split_whitespace()
            .next()
            .is_some_and(|verb| verb.eq_ignore_ascii_case("cutaway"))
        {
            return None;
        }
        let registers = self.cutaway_registers();
        Some(if registers.is_empty() {
            "No register holds an Original moment yet; copy one from the Original with v, y.".into()
        } else {
            let listed: Vec<String> = registers
                .iter()
                .map(|(name, range)| format!("{name} {range}"))
                .collect();
            format!(
                "Original moments: {} · change the letter, then Enter",
                listed.join(" · ")
            )
        })
    }
}
