//! One chronological proof of every retained scope and request address.

use std::collections::BTreeMap;

use deadpan_core::{ProjectDocument, ScopedNodeTarget, ValidatedScopedIsolation};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use super::{GenerationScopeId, integrity, parse_id, parse_target, read, target_json};
use crate::StoreError;

// Historical clocks are retained after deletion. Bound the complete address
// index, independently of the number of nodes in the current document.
const MAX_SCOPE_BYTES: i64 = 64 * 1024 * 1024;

pub(crate) fn check_budget(connection: &Connection) -> Result<(), StoreError> {
    let (count, bytes): (i64, i64) = connection.query_row(
        "SELECT count(*),coalesce(sum(length(CAST(scope_id AS BLOB)) + length(CAST(origin_revision AS BLOB))
            + length(CAST(origin_target AS BLOB)) + length(CAST(current_target AS BLOB))),0)
         FROM generation_scopes", [], |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if count > deadpan_core::MAX_DOCUMENT_NODES as i64 || bytes > MAX_SCOPE_BYTES {
        return Err(integrity(
            "generation scope index exceeds its count or byte budget",
        ));
    }
    Ok(())
}

pub(crate) fn digest(connection: &Connection) -> Result<crate::audit::Chain, StoreError> {
    crate::generation::check_stored_sizes(connection)?;
    let mut hasher = Sha256::new();
    hasher.update(b"deadpan-generation-scopes-v1");
    fn field(hasher: &mut Sha256, value: &str) {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    let mut scopes = connection.prepare(
        "SELECT scope_id,origin_revision,origin_target,current_target,high_water
         FROM generation_scopes ORDER BY scope_id",
    )?;
    let mut rows = scopes.query([])?;
    while let Some(row) = rows.next()? {
        hasher.update([1]);
        for index in 0..4 {
            field(&mut hasher, &row.get::<_, String>(index)?);
        }
        hasher.update(row.get::<_, i64>(4)?.to_le_bytes());
    }
    hasher.update([0]);
    // The receipt must also invalidate if a request is rebound to another
    // valid scope/origin, including a request that is already stale.
    let mut requests = connection.prepare(
        "SELECT request_id,scope_id,origin_revision,origin_target,hold_id,request_version
         FROM generation_requests ORDER BY request_id",
    )?;
    let mut rows = requests.query([])?;
    while let Some(row) = rows.next()? {
        hasher.update([1]);
        for index in 0..5 {
            field(&mut hasher, &row.get::<_, String>(index)?);
        }
        hasher.update(row.get::<_, i64>(5)?.to_le_bytes());
    }
    hasher.update([0]);
    hasher.update(crate::generation_preparations::digest(connection)?);
    hasher.update(crate::generation_origins::digest(connection)?);
    hasher.update(crate::generation_intents::digest(connection)?);
    Ok(hasher.finalize().into())
}

struct Address {
    target: ScopedNodeTarget,
    version: i64,
}

pub(crate) struct Replay {
    addresses: BTreeMap<GenerationScopeId, Address>,
    address_bytes: usize,
    enabled: bool,
}

impl Replay {
    pub(crate) fn request_has_target(
        &self,
        connection: &Connection,
        request: &deadpan_jobs::RequestId,
        target: &ScopedNodeTarget,
    ) -> Result<bool, StoreError> {
        use rusqlite::OptionalExtension;
        let scope: Option<String> = connection
            .query_row(
                "SELECT scope_id FROM generation_requests WHERE request_id=?1",
                [request.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(scope) = scope else {
            return Ok(false);
        };
        Ok(self
            .addresses
            .get(&parse_id(scope)?)
            .is_some_and(|address| &address.target == target))
    }

    pub(crate) fn new(
        connection: &Connection,
        initial: &ProjectDocument,
    ) -> Result<Self, StoreError> {
        check_budget(connection)?;
        let enabled = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM generation_scopes)",
            [],
            |row| row.get(0),
        )?;
        let mut value = Self {
            addresses: BTreeMap::new(),
            address_bytes: 0,
            enabled,
        };
        value.arrive(connection, initial)?;
        Ok(value)
    }

    pub(crate) fn map(
        &mut self,
        proof: &ValidatedScopedIsolation<'_>,
        forward: bool,
    ) -> Result<(), StoreError> {
        let mut bytes = 0;
        for (id, address) in &mut self.addresses {
            let target = if forward {
                proof.map_retained_forward(&address.target)
            } else {
                proof.map_retained_backward(&address.target)
            };
            bytes = address_budget(bytes, id, &target)?;
            address.target = target;
        }
        self.address_bytes = bytes;
        Ok(())
    }

    /// Requests are allocated between authored revisions. Births at this
    /// revision happen after its transition, before the next transition.
    pub(crate) fn arrive(
        &mut self,
        connection: &Connection,
        document: &ProjectDocument,
    ) -> Result<(), StoreError> {
        if !self.enabled {
            return Ok(());
        }
        let mut scopes = connection.prepare(
            "SELECT scope_id FROM generation_scopes WHERE origin_revision=?1 ORDER BY scope_id",
        )?;
        let mut rows = scopes.query([document.revision_id().as_str()])?;
        while let Some(row) = rows.next()? {
            let id = parse_id(row.get(0)?)?;
            let scope = read(connection, &id)?;
            scope.origin_target.validate(document).map_err(|_| {
                integrity("generation scope origin is not owned by its captured revision")
            })?;
            self.address_bytes = address_budget(self.address_bytes, &id, &scope.origin_target)?;
            if self
                .addresses
                .insert(
                    id,
                    Address {
                        target: scope.origin_target,
                        version: 0,
                    },
                )
                .is_some()
            {
                return Err(integrity("generation scope was born more than once"));
            }
        }
        let mut requests = connection.prepare(
            "SELECT scope_id,origin_target,request_version,request_id FROM generation_requests
             WHERE origin_revision=?1 ORDER BY scope_id,request_version",
        )?;
        let mut rows = requests.query([document.revision_id().as_str()])?;
        while let Some(row) = rows.next()? {
            let id = parse_id(row.get(0)?)?;
            let target = parse_target(&row.get::<_, String>(1)?)?;
            let version: i64 = row.get(2)?;
            let request_id: String = row.get(3)?;
            let address = self
                .addresses
                .get_mut(&id)
                .ok_or_else(|| integrity("generation request predates its authoring scope"))?;
            if address.target != target
                || version <= address.version
                || (address.version == 0 && request_id != id.as_str())
            {
                return Err(integrity(
                    "generation request origin or version disagrees with its scope history",
                ));
            }
            address.version = version;
        }
        Ok(())
    }

    pub(crate) fn finish(self, connection: &Connection) -> Result<(), StoreError> {
        if !self.enabled {
            return Ok(());
        }
        let count: i64 =
            connection.query_row("SELECT count(*) FROM generation_scopes", [], |row| {
                row.get(0)
            })?;
        if usize::try_from(count).ok() != Some(self.addresses.len()) {
            return Err(integrity(
                "generation scope origin is outside the revision chronology",
            ));
        }
        for (id, address) in self.addresses {
            let scope = read(connection, &id)?;
            if address.target != scope.target || address.version != scope.high_water {
                return Err(integrity(
                    "generation scope address or clock disagrees with isolation history",
                ));
            }
            // The stable clock's identity is the first request captured here.
            let first_matches: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM generation_requests WHERE request_id=?1 AND scope_id=?1
                    AND origin_revision=?2 AND origin_target=?3)",
                params![id.as_str(), scope.origin_revision.as_str(), target_json(&scope.origin_target)?],
                |row| row.get(0),
            )?;
            if !first_matches {
                return Err(integrity(
                    "generation scope does not match its first request",
                ));
            }
        }
        Ok(())
    }
}

fn address_budget(
    previous: usize,
    id: &GenerationScopeId,
    target: &ScopedNodeTarget,
) -> Result<usize, StoreError> {
    let target_bytes = target_json(target)?.len();
    previous
        .checked_add(id.as_str().len())
        .and_then(|bytes| bytes.checked_add(target_bytes))
        .filter(|bytes| *bytes <= MAX_SCOPE_BYTES as usize)
        .ok_or_else(|| {
            integrity("historical generation scope addresses exceed the replay byte budget")
        })
}
