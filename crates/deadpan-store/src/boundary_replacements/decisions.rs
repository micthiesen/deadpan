//! Pure final-provider decisions for bounded, variable-degree mismatch equations.
//!
//! Constants propagate before one fixed residual SCC partition. Components are
//! evaluated after their dependencies. A cycle first tries all retained, then
//! all fallback; otherwise its complete group uses the explicit conservative
//! policy. That policy does not assert that no mixed fixed point exists.

use std::ops::Range;

const MAX_NODES: usize = deadpan_core::MAX_DOCUMENT_NODES;
/// Aggregate input terms, including constants and duplicates. This is not a
/// per-Hold degree limit: temporal support can cross more providers than K+2.
const MAX_TERMS: usize = 8 * MAX_NODES;
const DEFAULT_WORK: usize = 64 * MAX_NODES;
const UNASSIGNED: usize = usize::MAX;

/// Whether an endpoint differs from the accepted Hold's required input.
/// Same can fill an unused equation slot; a missing picture is still compared by
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
    pub(super) max_terms: usize,
    pub(super) max_work: usize,
}

impl Default for DecisionLimits {
    fn default() -> Self {
        Self {
            max_nodes: MAX_NODES,
            max_terms: MAX_TERMS,
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
    /// max_nodes/max_terms and covered by the corresponding node/term visits.
    pub(super) work: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum DecisionError {
    #[error("boundary decision graph has {nodes} nodes; maximum is {maximum}")]
    NodeLimit { nodes: usize, maximum: usize },
    #[error("boundary decision graph exceeds its {maximum}-term limit")]
    TermLimit { maximum: usize },
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

/// OR every mismatch term; an empty equation is false. Input indices name
/// automatically eligible, currently accepted Holds. Already selected fallbacks
/// are constants in their consumers;
/// renewing live intent at them is a separate host decision, never a variable
/// that can restore an old accepted provider here. Invalid dependencies are
/// rejected even in a constant-true OR.
/// The function returns no partial decisions when any bound or invariant fails.
pub(super) fn decide<E: AsRef<[MismatchTerm]>>(
    equations: &[E],
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
    // Capture each borrowed slice once and check the aggregate bound before
    // allocating term/edge storage. Every dependency is validated, even after a
    // true constant or a complementary pair has made its equation true.
    let mut inputs = Vec::with_capacity(count);
    let mut term_count = 0_usize;
    for (node, equation) in equations.iter().enumerate() {
        budget.visit()?;
        let equation = equation.as_ref();
        let maximum = limits.max_terms.min(MAX_TERMS);
        term_count = term_count
            .checked_add(equation.len())
            .filter(|total| *total <= maximum)
            .ok_or(DecisionError::TermLimit { maximum })?;
        for term in equation {
            budget.visit()?;
            if let Some(dependency) = term.dependency()
                && dependency >= count
            {
                return Err(DecisionError::InvalidDependency { node, dependency });
            }
        }
        inputs.push(equation);
    }
    let mut folded = FlatEquations::new(&inputs, term_count, &mut budget)?;
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
                    folded.equation(node),
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
    for (node, equation) in inputs.iter().enumerate() {
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

/// Compact residual equations. Generation-stamped polarity entries deduplicate
/// dependencies and detect x OR !x in constant time per input term, without a
/// hash table, per-equation sort, or repeatedly clearing a node-sized table.
struct FlatEquations {
    terms: Vec<MismatchTerm>,
    ranges: Vec<Range<usize>>,
    constants: Vec<Option<bool>>,
}

impl FlatEquations {
    fn new(
        inputs: &[&[MismatchTerm]],
        term_count: usize,
        budget: &mut Budget,
    ) -> Result<Self, DecisionError> {
        let mut terms = Vec::with_capacity(term_count);
        let mut ranges = Vec::with_capacity(inputs.len());
        let mut constants = Vec::with_capacity(inputs.len());
        let mut seen = vec![(UNASSIGNED, 0_u8); inputs.len()];
        for (node, input) in inputs.iter().enumerate() {
            budget.visit()?;
            let start = terms.len();
            let mut different = false;
            for &term in *input {
                budget.visit()?;
                let (dependency, polarity) = match term {
                    MismatchTerm::Same => continue,
                    MismatchTerm::Different => {
                        different = true;
                        continue;
                    }
                    MismatchTerm::IfReplaced(index) => (index, 1),
                    MismatchTerm::IfRetained(index) => (index, 2),
                };
                let (generation, seen_polarities) = &mut seen[dependency];
                if *generation != node {
                    *generation = node;
                    *seen_polarities = 0;
                }
                if *seen_polarities & polarity == 0 {
                    *seen_polarities |= polarity;
                    different |= *seen_polarities == 3;
                    terms.push(term);
                }
            }
            if different {
                terms.truncate(start);
            }
            constants.push(if different {
                Some(true)
            } else if terms.len() == start {
                Some(false)
            } else {
                None
            });
            ranges.push(start..terms.len());
        }
        Ok(Self {
            terms,
            ranges,
            constants,
        })
    }

    fn equation(&self, node: usize) -> &[MismatchTerm] {
        &self.terms[self.ranges[node].clone()]
    }
}

fn evaluate(
    terms: &[MismatchTerm],
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

/// Compressed reverse adjacency. Slots index the bounded flat term array.
struct ReverseEdges {
    offsets: Vec<usize>,
    edges: Vec<(usize, usize)>,
}

impl ReverseEdges {
    fn new(equations: &FlatEquations, budget: &mut Budget) -> Result<Self, DecisionError> {
        let count = equations.ranges.len();
        let mut offsets = vec![0; count + 1];
        for term in &equations.terms {
            budget.visit()?;
            if let Some(dependency) = term.dependency() {
                offsets[dependency + 1] += 1;
            }
        }
        for index in 1..=count {
            budget.visit()?;
            let preceding = offsets[index - 1];
            offsets[index] += preceding;
        }
        let mut next = offsets[..count].to_vec();
        let mut edges = vec![(0, 0); offsets[count]];
        for (node, range) in equations.ranges.iter().enumerate() {
            budget.visit()?;
            for slot in range.clone() {
                budget.visit()?;
                if let Some(dependency) = equations.terms[slot].dependency() {
                    edges[next[dependency]] = (node, slot);
                    next[dependency] += 1;
                }
            }
        }
        Ok(Self { offsets, edges })
    }
}

fn propagate(
    equations: &mut FlatEquations,
    reverse: &ReverseEdges,
    budget: &mut Budget,
) -> Result<Vec<Option<bool>>, DecisionError> {
    let mut values = vec![None; equations.ranges.len()];
    let mut remaining = Vec::with_capacity(equations.ranges.len());
    let mut queue = Vec::new();
    for (node, range) in equations.ranges.iter().enumerate() {
        budget.visit()?;
        remaining.push(range.len());
        if let Some(value) = equations.constants[node] {
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
            let mismatch = match equations.terms[slot] {
                MismatchTerm::IfReplaced(_) => value,
                MismatchTerm::IfRetained(_) => !value,
                _ => {
                    return Err(DecisionError::InvalidGraph(
                        "constant edge was consumed twice",
                    ));
                }
            };
            equations.terms[slot] = if mismatch {
                MismatchTerm::Different
            } else {
                MismatchTerm::Same
            };
            remaining[node] = remaining[node]
                .checked_sub(1)
                .ok_or(DecisionError::InvalidGraph("constant edge count underflow"))?;
            if mismatch || remaining[node] == 0 {
                values[node] = Some(mismatch);
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
    equations: &FlatEquations,
    values: &[Option<bool>],
    budget: &mut Budget,
) -> Result<Components, DecisionError> {
    let count = equations.ranges.len();
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
            if slot < equations.ranges[node].len() {
                frames
                    .last_mut()
                    .ok_or(DecisionError::InvalidGraph("DFS frame is absent"))?
                    .1 += 1;
                let Some(dependency) = equations.equation(node)[slot].dependency() else {
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
        let input = [
            [Replaced(1), Replaced(1)],
            [Retained(1), Replaced(1)],
            [Same, Retained(1)],
            [Different, Retained(1)],
        ];
        let borrowed: Vec<&[_]> = input.iter().map(|terms| terms.as_slice()).collect();
        let flat = FlatEquations::new(
            &borrowed,
            8,
            &mut Budget {
                maximum: 12,
                used: 0,
            },
        )
        .unwrap();
        assert_eq!(flat.equation(0), [Replaced(1)]);
        assert!(flat.equation(1).is_empty());
        assert_eq!(flat.equation(2), [Retained(1)]);
        assert!(flat.equation(3).is_empty());
        assert_eq!(flat.constants, [None, Some(true), None, Some(true)]);
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
            decide::<[MismatchTerm; 2]>(
                &[],
                DecisionLimits {
                    max_nodes: 0,
                    max_terms: 0,
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
                    max_terms: usize::MAX,
                    max_work: usize::MAX
                }
            ),
            Err(DecisionError::NodeLimit { .. })
        ));
    }
}

#[cfg(test)]
#[path = "decisions/variable_tests.rs"]
mod variable_tests;
