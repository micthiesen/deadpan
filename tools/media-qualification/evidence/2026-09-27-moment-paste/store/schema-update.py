from pathlib import Path

root = Path('/Users/michael/Code/deadpan')
def replace(path, old, new, count=1):
    file = root / path
    text = file.read_text()
    assert text.count(old) == count, (path, old, text.count(old))
    file.write_text(text.replace(old, new))

legacy = (root / 'crates/deadpan-core/src/legacy_v25.rs').read_text()
legacy = legacy.replace('//! Frozen schema-25 document and history adapter. Root composite seams and\n//! physical beat interiors are admitted. Nested Sequence interiors remain\n//! forbidden even when a newer command produces valid matching patches.',
'''//! Frozen schema-26 document and history adapter. Ordinary Sequence interiors
//! retain their original pause-insertion semantics. The command grammar is
//! closed and cannot admit later source-splice commands.''')
legacy = legacy.replace('old.schema_version != 25', 'old.schema_version != 26')
legacy = legacy.replace('document schema 25', 'document schema 26')
legacy = legacy.replace('schema_version: 25,', 'schema_version: 26,')
legacy = legacy.replace('// Schema 25 used', '// Schema 26 used')
start = legacy.index('/// Check semantic admission that cannot be represented')
legacy = legacy[:start] + '''/// Schema 26 admits ordinary Sequence pause insertion. Its closed request
/// grammar already rejects source splicing, which first appears in schema 27.
pub fn validate_request_context(
    _document: &ProjectDocument,
    _request: &CommandRequest,
) -> Result<(), EditError> {
    Ok(())
}
'''
(root / 'crates/deadpan-core/src/legacy_v26.rs').write_text(legacy)
replace('crates/deadpan-core/src/document.rs', 'pub const DOCUMENT_SCHEMA_VERSION: u32 = 26;', 'pub const DOCUMENT_SCHEMA_VERSION: u32 = 27;')
replace('crates/deadpan-core/src/lib.rs', 'pub mod legacy_v25;', 'pub mod legacy_v25;\npub mod legacy_v26;')
replace('crates/deadpan-store/src/schema.rs', 'pub const VERSION: u32 = 32;', 'pub const VERSION: u32 = 33;')
for path in ['crates/deadpan-store/src/schema.rs', 'crates/deadpan-store/src/migration.rs']:
    replace(path, '1..=31', '1..=32')
path = 'crates/deadpan-store/src/validation.rs'
replace(path, 'legacy_v23, legacy_v24, legacy_v25,', 'legacy_v23, legacy_v24, legacy_v25, legacy_v26,')
replace(path, '31 => ReplaySchema::V25,', '31 => ReplaySchema::V25,\n        32 => ReplaySchema::V26,')
replace(path, '    V25,', '    V25,\n    V26,')
replace(path, '    V25(legacy_v25::Document),', '    V25(legacy_v25::Document),\n    V26(legacy_v26::Document),')
for expr in ['doc.revision_id()', 'Ok(doc.upgrade()?)']:
    replace(path, f'Self::V25(doc) => {expr},', f'Self::V25(doc) => {expr},\n            Self::V26(doc) => {expr},')
replace(path, 'Self::V25(stored) => stored.matches(doc),', 'Self::V25(stored) => stored.matches(doc),\n            Self::V26(stored) => stored.matches(doc),')
for old in ['ReplaySchema::V25 => StoredDocument::V25(legacy_v25::Document::from_json(&json)?),', 'ReplaySchema::V25 => legacy_v25::upgrade_request(&request_json)?,', 'ReplaySchema::V25 => legacy_v25::matches_edit(&edit_json, &calculated)?,']:
    replace(path, old, old + '\n        ' + old.replace('V25', 'V26').replace('v25', 'v26'))
replace(path, 'if schema == ReplaySchema::V25 {', 'if schema == ReplaySchema::V26 {\n                        legacy_v26::validate_request_context(&current, &request)?;\n                    } else if schema == ReplaySchema::V25 {')
replace('crates/deadpan-cli/src/doctor.rs', 'schema-1-through-31-migration', 'schema-1-through-32-migration')
replace('crates/deadpan-cli/tests/project_commands.rs', 'report["document_schema"], 26', 'report["document_schema"], 27')
replace('crates/deadpan-cli/tests/project_commands.rs', 'report["database_schema"], 32', 'report["database_schema"], 33')
replace('crates/deadpan-cli/tests/project_commands.rs', 'schema-1-through-31-migration', 'schema-1-through-32-migration')
