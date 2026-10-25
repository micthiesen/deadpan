use super::{
    LABELS, MAX_EDGES, MAX_STATES, MAX_TRACE_CELLS, MAX_WORDS, PhoneSpan, Pronunciation,
    RefinementControl, RefinementError,
};

pub(super) struct Node {
    pub label: u8,
    pub word: usize,
    pub alternative: usize,
    pub incoming: Vec<usize>,
}

pub(super) struct Graph {
    pub nodes: Vec<Node>,
    pub terminals: Vec<usize>,
}

impl Graph {
    pub fn new(choices: &[&[Pronunciation]]) -> Result<Self, RefinementError> {
        let mut nodes = vec![Node {
            label: 0,
            word: 0,
            alternative: 0,
            incoming: Vec::new(),
        }];
        let mut terminals = vec![0];
        let mut edges = 0usize;
        for (word, alternatives) in choices.iter().enumerate() {
            let mut next = Vec::new();
            for (alternative, pronunciation) in alternatives.iter().enumerate() {
                let mut incoming = terminals.clone();
                for &label in &pronunciation.labels {
                    edges = edges
                        .checked_add(incoming.len())
                        .ok_or(RefinementError::Limit("graph edges"))?;
                    if edges > MAX_EDGES || nodes.len() * 2 + 1 > MAX_STATES {
                        return Err(RefinementError::Limit("pronunciation graph"));
                    }
                    let id = nodes.len();
                    nodes.push(Node {
                        label,
                        word,
                        alternative,
                        incoming,
                    });
                    incoming = vec![id];
                }
                next.push(incoming[0]);
            }
            terminals = next;
        }
        Ok(Self { nodes, terminals })
    }

    pub fn key(&self, mut previous: ChoiceKey, node: usize) -> ChoiceKey {
        if node != 0 {
            let n = &self.nodes[node];
            if previous.len == n.word {
                previous.values[n.word] = n.alternative as u8;
                previous.len += 1;
            }
        }
        previous
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChoiceKey {
    values: [u8; MAX_WORDS],
    len: usize,
}

impl ChoiceKey {
    pub fn empty() -> Self {
        Self {
            values: [0; MAX_WORDS],
            len: 0,
        }
    }
    pub fn slice(&self) -> &[u8] {
        &self.values[..self.len]
    }
    pub fn indices(&self) -> Vec<usize> {
        self.slice().iter().map(|v| usize::from(*v)).collect()
    }
}

#[derive(Clone, Copy)]
struct Cell {
    score: f64,
    key: ChoiceKey,
}

impl Cell {
    fn unreachable() -> Self {
        Self {
            score: f64::NEG_INFINITY,
            key: ChoiceKey::empty(),
        }
    }
}

pub(super) struct Alignment {
    pub score: f64,
    pub labels: Vec<u8>,
    pub phones: Vec<PhoneSpan>,
    pub choices: Vec<usize>,
}

/// A phone owns a label state and its following blank. Incoming DAG edges
/// generate advance-from-blank and unequal-label skip transitions.
pub(super) fn align(
    logp: &[f64],
    frames: usize,
    graph: &Graph,
    control: &mut RefinementControl<'_>,
) -> Result<Alignment, RefinementError> {
    let states = graph.nodes.len() * 2 - 1;
    let cells = frames
        .checked_mul(states)
        .ok_or(RefinementError::Limit("CTC trace cells"))?;
    if cells > MAX_TRACE_CELLS {
        return Err(RefinementError::Limit("CTC trace cells"));
    }
    let mut trace = vec![0u16; cells];
    let mut previous = vec![Cell::unreachable(); states];
    previous[0] = Cell {
        score: logp[0],
        key: ChoiceKey::empty(),
    };
    for (id, node) in graph.nodes.iter().enumerate().skip(1) {
        if node.incoming == [0] {
            previous[id * 2 - 1] = Cell {
                score: logp[usize::from(node.label)],
                key: graph.key(ChoiceKey::empty(), id),
            };
        }
    }
    let mut current = previous.clone();
    for t in 1..frames {
        control.visit(states)?;
        for state in 0..states {
            let node = state.div_ceil(2);
            let blank = state % 2 == 0;
            // Same pronunciation: first equal maximum wins, stay then advance then skip.
            let mut best = previous[state];
            let mut predecessor = state;
            let mut consider = |from: usize| {
                let mut candidate = previous[from];
                if !blank {
                    candidate.key = graph.key(candidate.key, node);
                }
                if candidate.score > best.score
                    || (candidate.score == best.score && candidate.key.slice() < best.key.slice())
                {
                    best = candidate;
                    predecessor = from;
                }
            };
            if blank && state > 0 {
                consider(state - 1);
            }
            if !blank {
                for &incoming in &graph.nodes[node].incoming {
                    control.visit(1)?;
                    consider(incoming * 2);
                }
                for &incoming in &graph.nodes[node].incoming {
                    control.visit(1)?;
                    if incoming != 0 && graph.nodes[incoming].label != graph.nodes[node].label {
                        consider(incoming * 2 - 1);
                    }
                }
            }
            let label = if blank { 0 } else { graph.nodes[node].label };
            best.score += logp[t * LABELS.len() + usize::from(label)];
            current[state] = best;
            trace[t * states + state] =
                u16::try_from(predecessor).map_err(|_| RefinementError::Limit("trace state"))?;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let mut terminal = 0;
    let mut best = Cell::unreachable();
    // Blank first ensures the reference terminal-blank preference within a tuple.
    for &node in &graph.terminals {
        for state in [node * 2, node * 2 - 1] {
            let candidate = previous[state];
            if candidate.score > best.score
                || (candidate.score == best.score && candidate.key.slice() < best.key.slice())
            {
                best = candidate;
                terminal = state;
            }
        }
    }
    if !best.score.is_finite() {
        return Err(RefinementError::Unalignable);
    }
    let mut path = vec![0usize; frames];
    path[frames - 1] = terminal;
    for t in (1..frames).rev() {
        path[t - 1] = usize::from(trace[t * states + path[t]]);
    }
    let labels = path
        .iter()
        .map(|s| {
            if s % 2 == 0 {
                0
            } else {
                graph.nodes[s.div_ceil(2)].label
            }
        })
        .collect();
    let mut phones = Vec::new();
    let mut t = 0;
    while t < frames {
        let state = path[t];
        let begin = t;
        while t < frames && path[t] == state {
            t += 1;
        }
        if !state.is_multiple_of(2) {
            let label = graph.nodes[state.div_ceil(2)].label;
            phones.push(span(logp, label, begin, t));
        }
    }
    Ok(Alignment {
        score: best.score,
        labels,
        phones,
        choices: best.key.indices(),
    })
}

pub(super) fn span(logp: &[f64], label: u8, begin: usize, end: usize) -> PhoneSpan {
    let mut sum = 0.0;
    let mut minimum = f64::INFINITY;
    for frame in begin..end {
        let p = logp[frame * LABELS.len() + usize::from(label)].exp();
        sum += p;
        minimum = minimum.min(p);
    }
    PhoneSpan {
        label,
        start_frame: begin as u32,
        end_frame: end as u32,
        mean_posterior: sum / (end - begin) as f64,
        minimum_posterior: minimum,
    }
}
