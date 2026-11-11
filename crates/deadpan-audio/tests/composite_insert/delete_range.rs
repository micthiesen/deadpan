use super::*;

#[path = "delete_range/nested_partition.rs"]
mod nested_partition;

fn cut(document: &ProjectDocument, parent: &str, range: Range<i64>, name: &str) -> ProjectDocument {
    let range = FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap();
    let required = document
        .range_deletion(&id(parent), range)
        .unwrap()
        .required_ids;
    let after = edit(
        document,
        name,
        Command::DeleteRange {
            parent: id(parent),
            range,
            identities: SplitIdentities {
                nodes: (0..required)
                    .map(|index| id(&format!("{name}-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(
        after.duration().unwrap().frames(),
        document.duration().unwrap().frames() - (range.end().0 - range.start().0)
    );
    after
}
