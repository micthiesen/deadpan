use super::{
    ContextGuard, EditOperation, EditStep, LABELS, PhoneSpan, Pronunciation, RefinementControl,
    RefinementError, ResidualRun,
    alignment::{ChoiceKey, Graph, span},
};

#[derive(Clone, Copy)]
struct Cell {
    distance: u32,
    key: ChoiceKey,
}

fn choose(best: &mut Cell, candidate: Cell) {
    if candidate.distance < best.distance
        || (candidate.distance == best.distance && candidate.key.slice() < best.key.slice())
    {
        *best = candidate;
    }
}

/// Shortest unit edit distance through a pronunciation DAG. Only two columns
/// are retained. Replaying ordinary edit DP for the winning tuple below gives
/// exactly the reference diagonal/delete/insert trace without exponential paths.
fn select(
    observed: &[PhoneSpan],
    graph: &Graph,
    control: &mut RefinementControl<'_>,
) -> Result<(u32, Vec<usize>), RefinementError> {
    let count = graph.nodes.len();
    let unreachable = Cell {
        distance: u32::MAX / 2,
        key: ChoiceKey::empty(),
    };
    let mut previous = vec![unreachable; count];
    previous[0] = Cell {
        distance: 0,
        key: ChoiceKey::empty(),
    };
    for id in 1..count {
        control.visit(graph.nodes[id].incoming.len())?;
        for &from in &graph.nodes[id].incoming {
            let candidate = Cell {
                distance: previous[from].distance + 1,
                key: graph.key(previous[from].key, id),
            };
            choose(&mut previous[id], candidate);
        }
    }
    let mut current = previous.clone();
    for (j, phone) in observed.iter().enumerate() {
        control.visit(count)?;
        current[0] = Cell {
            distance: (j + 1) as u32,
            key: ChoiceKey::empty(),
        };
        for id in 1..count {
            let node = &graph.nodes[id];
            let mut best = Cell {
                distance: previous[id].distance + 1,
                key: previous[id].key,
            };
            for &from in &node.incoming {
                control.visit(2)?;
                choose(
                    &mut best,
                    Cell {
                        distance: previous[from].distance + u32::from(node.label != phone.label),
                        key: graph.key(previous[from].key, id),
                    },
                );
                choose(
                    &mut best,
                    Cell {
                        distance: current[from].distance + 1,
                        key: graph.key(current[from].key, id),
                    },
                );
            }
            current[id] = best;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let mut best = unreachable;
    for &id in &graph.terminals {
        choose(&mut best, previous[id]);
    }
    Ok((best.distance, best.key.indices()))
}

pub(super) fn edit(
    expected: &[u8],
    observed: &[PhoneSpan],
    control: &mut RefinementControl<'_>,
) -> Result<(u32, Vec<EditStep>), RefinementError> {
    let width = observed.len() + 1;
    let mut distance = vec![0u32; (expected.len() + 1) * width];
    let mut trace = vec![0u8; distance.len()];
    for i in 1..=expected.len() {
        distance[i * width] = i as u32;
        trace[i * width] = 1;
    }
    for j in 1..width {
        distance[j] = j as u32;
        trace[j] = 2;
    }
    for i in 1..=expected.len() {
        control.visit(width)?;
        for j in 1..width {
            let candidates = [
                distance[(i - 1) * width + j - 1]
                    + u32::from(expected[i - 1] != observed[j - 1].label),
                distance[(i - 1) * width + j] + 1,
                distance[i * width + j - 1] + 1,
            ];
            let mut best = 0;
            for k in 1..3 {
                if candidates[k] < candidates[best] {
                    best = k;
                }
            }
            distance[i * width + j] = candidates[best];
            trace[i * width + j] = best as u8;
        }
    }
    let mut i = expected.len();
    let mut j = observed.len();
    let mut path = Vec::with_capacity(i + j);
    while i > 0 || j > 0 {
        let step = match trace[i * width + j] {
            0 => {
                i -= 1;
                j -= 1;
                EditStep {
                    operation: if expected[i] == observed[j].label {
                        EditOperation::Match
                    } else {
                        EditOperation::Substitute
                    },
                    expected_index: Some(i),
                    observed_index: Some(j),
                }
            }
            1 => {
                i -= 1;
                EditStep {
                    operation: EditOperation::DeleteExpected,
                    expected_index: Some(i),
                    observed_index: None,
                }
            }
            _ => {
                j -= 1;
                EditStep {
                    operation: EditOperation::InsertObserved,
                    expected_index: None,
                    observed_index: Some(j),
                }
            }
        };
        path.push(step);
    }
    path.reverse();
    Ok((distance[expected.len() * width + observed.len()], path))
}

pub(super) fn inspect(
    logp: &[f64],
    frames: usize,
    graph: &Graph,
    choices: &[&[Pronunciation]],
    control: &mut RefinementControl<'_>,
) -> Result<ContextGuard, RefinementError> {
    let mut labels = Vec::with_capacity(frames);
    for row in logp.chunks_exact(LABELS.len()) {
        control.visit(LABELS.len())?;
        let mut best = 0;
        for i in 1..row.len() {
            if row[i] > row[best] {
                best = i;
            }
        }
        labels.push(best as u8);
    }
    let mut greedy = Vec::new();
    let mut t = 0;
    while t < frames {
        let begin = t;
        let label = labels[t];
        while t < frames && labels[t] == label {
            t += 1;
        }
        if label != 0 {
            greedy.push(span(logp, label, begin, t));
        }
    }
    let (minimum_edit_distance, selected) = select(&greedy, graph, control)?;
    let selected_pronunciations: Vec<_> = selected
        .iter()
        .enumerate()
        .map(|(i, p)| choices[i][*p].clone())
        .collect();
    let expected: Vec<_> = selected_pronunciations
        .iter()
        .flat_map(|p| p.labels.iter().copied())
        .collect();
    let (distance, alignment_steps) = edit(&expected, &greedy, control)?;
    debug_assert_eq!(distance, minimum_edit_distance);
    let mut residual_runs = Vec::new();
    let mut run = Vec::new();
    for step in &alignment_steps {
        if let Some(index) = step.observed_index
            && step.operation == EditOperation::InsertObserved
            && greedy[index].mean_posterior >= 0.5
        {
            run.push(index);
            continue;
        }
        flush(&mut run, &greedy, &mut residual_runs);
    }
    flush(&mut run, &greedy, &mut residual_runs);
    let abstain = residual_runs.iter().any(|run| run.qualifies);
    Ok(ContextGuard {
        greedy_phones: greedy,
        selected_pronunciations,
        minimum_edit_distance,
        alignment_steps,
        residual_runs,
        abstain,
    })
}

fn flush(run: &mut Vec<usize>, phones: &[PhoneSpan], out: &mut Vec<ResidualRun>) {
    if run.is_empty() {
        return;
    }
    let start_frame = phones[run[0]].start_frame;
    let end_frame = phones[run[run.len() - 1]].end_frame;
    let supported_frames = run
        .iter()
        .map(|i| phones[*i].end_frame - phones[*i].start_frame)
        .sum();
    let minimum_mean_posterior = run
        .iter()
        .map(|i| phones[*i].mean_posterior)
        .fold(f64::INFINITY, f64::min);
    out.push(ResidualRun {
        start_frame,
        end_frame,
        supported_frames,
        minimum_mean_posterior,
        qualifies: run.len() >= 3 && supported_frames >= 6,
        observed_indices: std::mem::take(run),
    });
}
