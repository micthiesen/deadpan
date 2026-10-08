//! Pure final-provider decisions for at most two boundary mismatch terms per Hold.
//!
//! Constants propagate before one fixed residual SCC partition. Components are
//! evaluated after their dependencies. A cycle first tries all retained, then
//! all fallback; otherwise its complete group uses the explicit conservative
//! policy. That policy does not assert that no mixed fixed point exists.

use std::ops::Range;

const MAX_NODES: usize = deadpan_core::MAX_DOCUMENT_NODES;
const DEFAULT_WORK: usize = 64 * MAX_NODES;
const UNASSIGNED: usize = usize::MAX;

/// Whether an endpoint differs from the accepted Hold's required input.
/// Same fills an unused equation slot; a missing picture is still compared by
/// the caller as an actual endpoint identity, never assumed to be Same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MismatchTerm {
    Same,
    Different,
    IfReplaced(usize),
    IfRetained(usize),
}

impl MismatchTerm {
    pub(super) fn from_matches(
        dependency: usize,
        accepted_matches: bool,
        fallback_matches: bool,
    ) -> Self {
        match (accepted_matches, fallback_matches) {
            (true, true) => Self::Same,
            (true, false) => Self::IfReplaced(dependency),
            (false, true) => Self::IfRetained(dependency),
            (false, false) => Self::Different,
        }
    }

    fn dependency(self) -> Option<usize> {
        match self {
            Self::IfReplaced(index) | Self::IfRetained(index) => Some(index),
            Self::Same | Self::Different => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DecisionLimits {
    pub(super) max_nodes: usize,
    pub(super) max_work: usize,
}

impl Default for DecisionLimits {
    fn default() -> Self {
        Self {
            max_nodes: MAX_NODES,
            max_work: DEFAULT_WORK,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DecisionReason {
    InputsMatch,
    BoundaryChanged,
    /// Index into Decisions::cyclic_groups. The group failed both uniform
    /// fixed-point tests; mixed solutions were not searched or ruled out.
    CyclicBoundaryDependencies {
        group: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NodeDecision {
    pub(super) replace: bool,
    pub(super) reason: DecisionReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Decisions {
    pub(super) nodes: Vec<NodeDecision>,
    /// Conservative groups only, sorted by smallest member index. Members are
    /// ascending input indices. Exact all-fallback cycles are not listed here.
    pub(super) cyclic_groups: Vec<Vec<usize>>,
    /// Node and term/edge visits across validation, folding, reverse indexing,
    /// propagation, SCC traversal, evaluation, and final verification. Fixed
    /// size vector initialization and map-free O(1) bookkeeping are bounded by
    /// max_nodes and the two-terms-per-node contract, not charged separately.
    pub(super) work: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum DecisionError {
    #[error("boundary decision graph has {nodes} nodes; maximum is {maximum}")]
    NodeLimit { nodes: usize, maximum: usize },
    #[error("boundary decision graph exceeded its {maximum}-visit work limit")]
    WorkLimit { maximum: usize },
    #[error("boundary equation {node} names absent dependency {dependency}")]
    InvalidDependency { node: usize, dependency: usize },
    #[error("boundary decision graph invariant failed: {0}")]
    InvalidGraph(&'static str),
}

struct Budget {
    maximum: usize,
    used: usize,
}

impl Budget {
    fn visit(&mut self) -> Result<(), DecisionError> {
        if self.used == self.maximum {
            return Err(DecisionError::WorkLimit {
                maximum: self.maximum,
            });
        }
        self.used += 1;
        Ok(())
    }
}

/// OR both endpoint terms. Input indices name automatically eligible, currently
/// accepted Holds. Already selected fallbacks are constants in their consumers;
/// renewing live intent at them is a separate host decision, never a variable
/// that can restore an old accepted provider here. Invalid dependencies are
/// rejected even in a constant-true OR.
/// The function returns no partial decisions when any bound or invariant fails.
pub(super) fn decide(
    equations: &[[MismatchTerm; 2]],
    limits: DecisionLimits,
) -> Result<Decisions, DecisionError> {
    let count = equations.len();
    let maximum = limits.max_nodes.min(MAX_NODES);
    if count > maximum {
        return Err(DecisionError::NodeLimit {
            nodes: count,
            maximum,
        });
    }
    if count > limits.max_work {
        return Err(DecisionError::WorkLimit {
            maximum: limits.max_work,
        });
    }
    let mut budget = Budget {
        maximum: limits.max_work,
        used: 0,
    };
    let mut folded = Vec::with_capacity(count);
    for (node, equation) in equations.iter().enumerate() {
        budget.visit()?;
        for term in equation {
            budget.visit()?;
            if let Some(dependency) = term.dependency()
                && dependency >= count
            {
                return Err(DecisionError::InvalidDependency { node, dependency });
            }
        }
        folded.push(fold(*equation));
    }
    let reverse = ReverseEdges::new(&folded, &mut budget)?;
    let mut values = propagate(&mut folded, &reverse, &mut budget)?;
    let components = components(&folded, &values, &mut budget)?;
    let mut conservative = vec![false; components.ranges.len()];
    // Tarjan traverses consumer -> dependency, so completed SCCs already have
    // every external dependency finalized. Never repartition after this point.
    for (component, range) in components.ranges.iter().enumerate() {
        let mut choice = None;
        for candidate in [false, true] {
            let mut matches = true;
            for &node in &components.members[range.clone()] {
                budget.visit()?;
                let output = evaluate(
                    &folded[node],
                    |dependency| {
                        if components.by_node[dependency] == component {
                            Ok(candidate)
                        } else {
                            values[dependency].ok_or(DecisionError::InvalidGraph(
                                "component dependency was not finalized first",
                            ))
                        }
                    },
                    &mut budget,
                )?;
                matches &= output == candidate;
            }
            if matches {
                choice = Some(candidate);
                break;
            }
        }
        conservative[component] = choice.is_none();
        for &node in &components.members[range.clone()] {
            budget.visit()?;
            values[node] = Some(choice.unwrap_or(true));
        }
    }
    let mut nodes = Vec::with_capacity(count);
    let mut cyclic_groups: Vec<Vec<usize>> = Vec::new();
    let mut group_for_component = vec![None; components.ranges.len()];
    for (node, equation) in equations.iter().enumerate() {
        budget.visit()?;
        let replace = values[node].ok_or(DecisionError::InvalidGraph("unassigned final node"))?;
        let mismatch = evaluate(
            equation,
            |dependency| {
                values[dependency].ok_or(DecisionError::InvalidGraph("unassigned final dependency"))
            },
            &mut budget,
        )?;
        let component = components.by_node[node];
        let cyclic = component != UNASSIGNED && conservative[component];
        if (!replace && mismatch) || (!cyclic && replace != mismatch) {
            return Err(DecisionError::InvalidGraph(
                "final provider assignment is inconsistent",
            ));
        }
        let reason = if cyclic {
            let group = *group_for_component[component].get_or_insert_with(|| {
                cyclic_groups.push(Vec::new());
                cyclic_groups.len() - 1
            });
            cyclic_groups[group].push(node);
            DecisionReason::CyclicBoundaryDependencies { group }
        } else if replace {
            DecisionReason::BoundaryChanged
        } else {
            DecisionReason::InputsMatch
        };
        nodes.push(NodeDecision { replace, reason });
    }
    Ok(Decisions {
        nodes,
        cyclic_groups,
        work: budget.used,
    })
}

fn fold(mut terms: [MismatchTerm; 2]) -> [MismatchTerm; 2] {
    use MismatchTerm::{Different, IfReplaced, IfRetained, Same};
    if terms.contains(&Different)
        || matches!(terms, [IfReplaced(a), IfRetained(b)] | [IfRetained(a), IfReplaced(b)] if a == b)
    {
        return [Different, Same];
    }
    if terms[0] == Same {
        terms.swap(0, 1);
    }
    if terms[0] == terms[1] {
        terms[1] = Same;
    }
    terms
}

fn constant(terms: &[MismatchTerm; 2]) -> Option<bool> {
    if terms.contains(&MismatchTerm::Different) {
        Some(true)
    } else if *terms == [MismatchTerm::Same; 2] {
        Some(false)
    } else {
        None
    }
}

fn evaluate(
    terms: &[MismatchTerm; 2],
    mut value: impl FnMut(usize) -> Result<bool, DecisionError>,
    budget: &mut Budget,
) -> Result<bool, DecisionError> {
    let mut mismatch = false;
    for term in terms {
        budget.visit()?;
        mismatch |= match *term {
            MismatchTerm::Same => false,
            MismatchTerm::Different => true,
            MismatchTerm::IfReplaced(index) => value(index)?,
            MismatchTerm::IfRetained(index) => !value(index)?,
        };
    }
    Ok(mismatch)
}

/// Compressed reverse adjacency; two slots per equation bound all edges.
struct ReverseEdges {
    offsets: Vec<usize>,
    edges: Vec<(usize, usize)>,
}

impl ReverseEdges {
    fn new(equations: &[[MismatchTerm; 2]], budget: &mut Budget) -> Result<Self, DecisionError> {
        let count = equations.len();
        let mut offsets = vec![0; count + 1];
        for equation in equations {
            for term in equation {
                budget.visit()?;
                if let Some(dependency) = term.dependency() {
                    offsets[dependency + 1] += 1;
                }
            }
        }
        for index in 1..=count {
            budget.visit()?;
            let preceding = offsets[index - 1];
            offsets[index] += preceding;
        }
        let mut next = offsets[..count].to_vec();
        let mut edges = vec![(0, 0); offsets[count]];
        for (node, equation) in equations.iter().enumerate() {
            for (slot, term) in equation.iter().enumerate() {
                budget.visit()?;
                if let Some(dependency) = term.dependency() {
                    edges[next[dependency]] = (node, slot);
                    next[dependency] += 1;
                }
            }
        }
        Ok(Self { offsets, edges })
    }
}

fn propagate(
    equations: &mut [[MismatchTerm; 2]],
    reverse: &ReverseEdges,
    budget: &mut Budget,
) -> Result<Vec<Option<bool>>, DecisionError> {
    let mut values = vec![None; equations.len()];
    let mut queue = Vec::new();
    for (node, equation) in equations.iter().enumerate() {
        budget.visit()?;
        if let Some(value) = constant(equation) {
            values[node] = Some(value);
            queue.push(node);
        }
    }
    let mut first = 0;
    while first < queue.len() {
        budget.visit()?;
        let dependency = queue[first];
        first += 1;
        let value =
            values[dependency].ok_or(DecisionError::InvalidGraph("queued constant is absent"))?;
        for &(node, slot) in
            &reverse.edges[reverse.offsets[dependency]..reverse.offsets[dependency + 1]]
        {
            budget.visit()?;
            if values[node].is_some() {
                continue;
            }
            let mismatch = match equations[node][slot] {
                MismatchTerm::IfReplaced(_) => value,
                MismatchTerm::IfRetained(_) => !value,
                _ => {
                    return Err(DecisionError::InvalidGraph(
                        "constant edge was consumed twice",
                    ));
                }
            };
            equations[node][slot] = if mismatch {
                MismatchTerm::Different
            } else {
                MismatchTerm::Same
            };
            if let Some(value) = constant(&equations[node]) {
                values[node] = Some(value);
                queue.push(node);
            }
        }
    }
    Ok(values)
}

struct Components {
    members: Vec<usize>,
    ranges: Vec<Range<usize>>,
    by_node: Vec<usize>,
}

/// Iterative Tarjan traversal. Edges point from consumer to dependency, giving
/// dependency-first component completion without another condensation sort.
fn components(
    equations: &[[MismatchTerm; 2]],
    values: &[Option<bool>],
    budget: &mut Budget,
) -> Result<Components, DecisionError> {
    let count = equations.len();
    let mut indices = vec![UNASSIGNED; count];
    let mut low = vec![UNASSIGNED; count];
    let mut active = vec![false; count];
    let mut stack = Vec::new();
    let mut frames: Vec<(usize, usize)> = Vec::new();
    let mut serial = 0;
    let mut result = Components {
        members: Vec::new(),
        ranges: Vec::new(),
        by_node: vec![UNASSIGNED; count],
    };
    for start in 0..count {
        budget.visit()?;
        if values[start].is_some() || indices[start] != UNASSIGNED {
            continue;
        }
        indices[start] = serial;
        low[start] = serial;
        serial += 1;
        active[start] = true;
        stack.push(start);
        frames.push((start, 0));
        while let Some(&(node, slot)) = frames.last() {
            budget.visit()?;
            if slot < 2 {
                frames
                    .last_mut()
                    .ok_or(DecisionError::InvalidGraph("DFS frame is absent"))?
                    .1 += 1;
                let Some(dependency) = equations[node][slot].dependency() else {
                    continue;
                };
                if values[dependency].is_some() {
                    return Err(DecisionError::InvalidGraph(
                        "constant dependency survived propagation",
                    ));
                }
                if indices[dependency] == UNASSIGNED {
                    indices[dependency] = serial;
                    low[dependency] = serial;
                    serial += 1;
                    active[dependency] = true;
                    stack.push(dependency);
                    frames.push((dependency, 0));
                } else if active[dependency] {
                    low[node] = low[node].min(indices[dependency]);
                }
            } else {
                frames.pop();
                if low[node] == indices[node] {
                    let beginning = result.members.len();
                    loop {
                        budget.visit()?;
                        let member = stack
                            .pop()
                            .ok_or(DecisionError::InvalidGraph("SCC stack is absent"))?;
                        active[member] = false;
                        result.by_node[member] = result.ranges.len();
                        result.members.push(member);
                        if member == node {
                            break;
                        }
                    }
                    result.ranges.push(beginning..result.members.len());
                }
                if let Some(&(parent, _)) = frames.last() {
                    low[parent] = low[parent].min(low[node]);
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use MismatchTerm::{Different, IfReplaced as Replaced, IfRetained as Retained, Same};

    fn run(equations: &[[MismatchTerm; 2]]) -> Decisions {
        decide(equations, DecisionLimits::default()).unwrap()
    }

    fn replaced(result: &Decisions) -> Vec<bool> {
        result.nodes.iter().map(|node| node.replace).collect()
    }

    #[test]
    fn endpoint_truth_tables_and_local_constant_folding() {
        assert_eq!(MismatchTerm::from_matches(7, true, true), Same);
        assert_eq!(MismatchTerm::from_matches(7, true, false), Replaced(7));
        assert_eq!(MismatchTerm::from_matches(7, false, true), Retained(7));
        assert_eq!(MismatchTerm::from_matches(7, false, false), Different);
        assert_eq!(fold([Replaced(1), Replaced(1)]), [Replaced(1), Same]);
        assert_eq!(fold([Retained(1), Replaced(1)]), [Different, Same]);
        assert_eq!(fold([Same, Retained(1)]), [Retained(1), Same]);
        assert_eq!(fold([Different, Retained(1)]), [Different, Same]);
        let result = run(&[[Replaced(0), Retained(0)], [Retained(0), Retained(0)]]);
        assert_eq!(replaced(&result), [true, false]);
        assert!(result.cyclic_groups.is_empty());
    }

    #[test]
    fn forced_a_fallback_preserves_b_that_matches_only_that_fallback() {
        let result = run(&[[Different, Replaced(1)], [Retained(0), Same]]);
        assert_eq!(replaced(&result), [true, false]);
        assert_eq!(result.nodes[1].reason, DecisionReason::InputsMatch);
        assert!(result.cyclic_groups.is_empty());
    }

    #[test]
    fn unchanged_positive_cycles_and_their_dependents_stay_accepted() {
        let result = run(&[
            [Replaced(1), Same],
            [Replaced(0), Same],
            [Replaced(1), Replaced(2)],
        ]);
        assert_eq!(replaced(&result), [false, false, false]);
        assert!(result.cyclic_groups.is_empty());
    }

    #[test]
    fn mixed_solution_even_cycle_uses_an_explicit_conservative_group() {
        let result = run(&[[Retained(1), Same], [Retained(0), Same]]);
        // [false,true] and [true,false] are fixed points. The policy does not
        // search them or falsely report that the graph has no fixed point.
        assert_eq!(replaced(&result), [true, true]);
        assert_eq!(result.cyclic_groups, [vec![0, 1]]);
        assert!(
            result
                .nodes
                .iter()
                .all(|node| node.reason == DecisionReason::CyclicBoundaryDependencies { group: 0 })
        );
    }

    #[test]
    fn odd_negation_cycle_uses_the_same_truthful_conservative_policy() {
        let result = run(&[
            [Retained(1), Same],
            [Retained(2), Same],
            [Retained(0), Same],
        ]);
        assert_eq!(replaced(&result), [true, true, true]);
        assert_eq!(result.cyclic_groups, [vec![0, 1, 2]]);
    }

    #[test]
    fn cycle_can_choose_an_exact_all_fallback_fixed_point() {
        let result = run(&[
            [Replaced(1), Retained(2)],
            [Replaced(0), Same],
            [Replaced(2), Same],
        ]);
        assert_eq!(replaced(&result), [true, true, false]);
        assert!(result.cyclic_groups.is_empty());
        assert_eq!(result.nodes[0].reason, DecisionReason::BoundaryChanged);
        assert_eq!(result.nodes[1].reason, DecisionReason::BoundaryChanged);
    }

    #[test]
    fn residual_partition_stays_fixed_after_external_values_are_known() {
        let result = run(&[
            [Replaced(1), Retained(2)],
            [Retained(0), Same],
            [Replaced(2), Same],
        ]);
        // Final z=0 forces x=1, which could simplify this group to y=0. The
        // declared residual SCC policy does not repartition or seek that mix.
        assert_eq!(replaced(&result), [true, true, false]);
        assert_eq!(result.cyclic_groups, [vec![0, 1]]);
    }

    #[test]
    fn downstream_dag_uses_final_conservative_values_exactly() {
        let result = run(&[
            [Retained(1), Same],
            [Retained(0), Same],
            [Retained(0), Same],
            [Replaced(1), Same],
            [Replaced(2), Retained(3)],
        ]);
        assert_eq!(replaced(&result), [true, true, false, true, false]);
        assert_eq!(result.cyclic_groups, [vec![0, 1]]);
        assert_eq!(result.nodes[2].reason, DecisionReason::InputsMatch);
        assert_eq!(result.nodes[3].reason, DecisionReason::BoundaryChanged);
    }

    #[test]
    fn node_and_endpoint_permutations_do_not_choose_provider_outcomes() {
        let equations = [
            [Retained(1), Same],
            [Retained(0), Same],
            [Retained(0), Same],
            [Replaced(4), Retained(5)],
            [Replaced(3), Same],
            [Replaced(5), Same],
            [Retained(6), Same],
        ];
        let baseline = run(&equations);
        for order in [
            [6, 5, 4, 3, 2, 1, 0],
            [3, 0, 6, 1, 5, 2, 4],
            [2, 4, 1, 6, 0, 3, 5],
        ] {
            let mut inverse = [0; 7];
            for (new, &old) in order.iter().enumerate() {
                inverse[old] = new;
            }
            let remap = |term| match term {
                Replaced(index) => Replaced(inverse[index]),
                Retained(index) => Retained(inverse[index]),
                other => other,
            };
            let permuted: Vec<_> = order
                .iter()
                .map(|&old| [remap(equations[old][1]), remap(equations[old][0])])
                .collect();
            let actual = run(&permuted);
            for (new, &old) in order.iter().enumerate() {
                assert_eq!(actual.nodes[new].replace, baseline.nodes[old].replace);
                match (actual.nodes[new].reason, baseline.nodes[old].reason) {
                    (
                        DecisionReason::CyclicBoundaryDependencies { group: a },
                        DecisionReason::CyclicBoundaryDependencies { group: b },
                    ) => {
                        let mut mapped: Vec<_> = actual.cyclic_groups[a]
                            .iter()
                            .map(|&member| order[member])
                            .collect();
                        mapped.sort_unstable();
                        assert_eq!(mapped, baseline.cyclic_groups[b]);
                    }
                    (a, b) => assert_eq!(a, b),
                }
            }
        }
    }

    #[test]
    fn malformed_indices_and_exact_work_limits_fail_without_partial_results() {
        assert!(matches!(
            decide(&[[Different, Replaced(1)]], DecisionLimits::default()),
            Err(DecisionError::InvalidDependency {
                node: 0,
                dependency: 1
            })
        ));
        assert!(matches!(
            decide(
                &[[Same; 2]],
                DecisionLimits {
                    max_nodes: 0,
                    ..DecisionLimits::default()
                }
            ),
            Err(DecisionError::NodeLimit { .. })
        ));
        let equations = [
            [Retained(1), Same],
            [Retained(0), Same],
            [Replaced(1), Same],
        ];
        let expected = run(&equations);
        assert_eq!(
            decide(
                &equations,
                DecisionLimits {
                    max_work: expected.work,
                    ..DecisionLimits::default()
                }
            )
            .unwrap(),
            expected
        );
        assert!(matches!(
            decide(
                &equations,
                DecisionLimits {
                    max_work: expected.work - 1,
                    ..DecisionLimits::default()
                }
            ),
            Err(DecisionError::WorkLimit { .. })
        ));
        assert_eq!(
            decide(
                &[],
                DecisionLimits {
                    max_nodes: 0,
                    max_work: 0
                }
            )
            .unwrap(),
            Decisions {
                nodes: Vec::new(),
                cyclic_groups: Vec::new(),
                work: 0
            }
        );
    }

    #[test]
    fn one_hundred_thousand_node_chains_use_bounded_linear_iterative_work() {
        let mut previous = None;
        for count in [50_000, 100_000] {
            // Starting DFS at zero traverses the complete unresolved chain
            // before the final positive self-cycle can be finalized.
            let equations: Vec<_> = (0..count)
                .map(|node| [Replaced((node + 1).min(count - 1)), Same])
                .collect();
            let result = run(&equations);
            assert!(result.nodes.iter().all(|node| !node.replace));
            assert!(result.cyclic_groups.is_empty());
            assert!(
                result.work <= 32 * count,
                "{} visits for {count} nodes",
                result.work
            );
            if let Some(prior) = previous {
                assert_eq!(result.work, 2 * prior);
            }
            previous = Some(result.work);
        }
        let count = 100_000;
        let mut forced: Vec<_> = (0..count)
            .map(|node| [Retained((node + 1).min(count - 1)), Same])
            .collect();
        forced[count - 1] = [Different, Same];
        let result = run(&forced);
        for (index, node) in result.nodes.iter().enumerate() {
            assert_eq!(node.replace, (count - index) % 2 == 1);
        }
        assert!(result.work <= 32 * count);
        assert!(result.cyclic_groups.is_empty());
        let over = vec![[Same; 2]; MAX_NODES + 1];
        assert!(matches!(
            decide(
                &over,
                DecisionLimits {
                    max_nodes: usize::MAX,
                    max_work: usize::MAX
                }
            ),
            Err(DecisionError::NodeLimit { .. })
        ));
    }
}
