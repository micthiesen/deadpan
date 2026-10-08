use super::*;
use MismatchTerm::{Different, IfReplaced as Replaced, IfRetained as Retained, Same};

fn run(equations: &[Vec<MismatchTerm>]) -> Decisions {
    decide(equations, DecisionLimits::default()).unwrap()
}

fn replaced(result: &Decisions) -> Vec<bool> {
    result.nodes.iter().map(|node| node.replace).collect()
}

#[test]
fn variable_terms_keep_hidden_dependencies_beyond_k_plus_two() {
    let count = 96;
    let mut equations: Vec<Vec<_>> = (0..count)
        .map(|node| {
            (1..=16)
                .map(|step| Replaced((node + step).min(count - 1)))
                .collect()
        })
        .collect();
    let retained = run(&equations);
    assert!(retained.nodes.iter().all(|node| !node.replace));
    assert!(retained.cyclic_groups.is_empty());
    // All earlier terms depend on retained positive cycles. Only the final
    // term observes the forced fallback; a K+2 truncation would miss it.
    equations[count - 1] = vec![Different];
    let changed = run(&equations);
    assert!(changed.nodes.iter().all(|node| node.replace));
    assert!(changed.cyclic_groups.is_empty());
    let mut hidden = vec![vec![Replaced(0); 16], vec![Different]];
    hidden[0].push(Replaced(1));
    assert_eq!(replaced(&run(&hidden)), [true, true]);
}

#[test]
fn high_degree_cycles_keep_uniform_and_conservative_policies() {
    let count = 24;
    let positive: Vec<Vec<_>> = (0..count)
        .map(|node| {
            (1..=16)
                .map(|step| Replaced((node + step) % count))
                .collect()
        })
        .collect();
    let result = run(&positive);
    assert!(result.nodes.iter().all(|node| !node.replace));
    assert!(result.cyclic_groups.is_empty());
    let negative: Vec<Vec<_>> = positive
        .iter()
        .map(|terms| {
            terms
                .iter()
                .map(|term| Retained(term.dependency().unwrap()))
                .collect()
        })
        .collect();
    let result = run(&negative);
    assert!(result.nodes.iter().all(|node| node.replace));
    assert_eq!(result.cyclic_groups, [Vec::from_iter(0..count)]);

    // An external retained self-cycle makes every member of the high-degree
    // component true. All-fallback is exact, so no conservative reason is used.
    let mut fallback = positive;
    for terms in &mut fallback {
        terms.push(Retained(count));
    }
    fallback.push(vec![Replaced(count)]);
    let result = run(&fallback);
    assert!(result.nodes[..count].iter().all(|node| node.replace));
    assert!(!result.nodes[count].replace);
    assert!(result.cyclic_groups.is_empty());
}

#[test]
fn high_degree_node_and_term_permutations_preserve_decisions_and_groups() {
    let count = 31;
    let mut equations: Vec<Vec<_>> = (0..count)
        .map(|node| {
            (1..=16)
                .map(|step| Retained((node + step) % count))
                .collect()
        })
        .collect();
    equations.push(vec![Retained(0); 19]);
    equations.push(vec![Replaced(0); 23]);
    let expected = run(&equations);
    // Multiplication is a permutation modulo 33 when the multiplier is 10.
    let order: Vec<_> = (0..equations.len())
        .map(|n| (n * 10 + 7) % equations.len())
        .collect();
    let mut inverse = vec![0; order.len()];
    for (new, &old) in order.iter().enumerate() {
        inverse[old] = new;
    }
    let permuted: Vec<Vec<_>> = order
        .iter()
        .map(|&old| {
            equations[old]
                .iter()
                .rev()
                .map(|term| match *term {
                    Replaced(index) => Replaced(inverse[index]),
                    Retained(index) => Retained(inverse[index]),
                    other => other,
                })
                .collect()
        })
        .collect();
    let actual = run(&permuted);
    for (new, &old) in order.iter().enumerate() {
        assert_eq!(actual.nodes[new].replace, expected.nodes[old].replace);
        match (actual.nodes[new].reason, expected.nodes[old].reason) {
            (
                DecisionReason::CyclicBoundaryDependencies { group: a },
                DecisionReason::CyclicBoundaryDependencies { group: b },
            ) => {
                let mut mapped: Vec<_> =
                    actual.cyclic_groups[a].iter().map(|&n| order[n]).collect();
                mapped.sort_unstable();
                assert_eq!(mapped, expected.cyclic_groups[b]);
            }
            (a, b) => assert_eq!(a, b),
        }
    }
}

#[test]
fn empty_equations_and_distant_duplicate_complements_fold_exactly() {
    let mut terms = vec![Replaced(1); 4096];
    terms.extend([Same, Retained(1)]);
    let result = run(&[terms, vec![], vec![Retained(0)], vec![Same; 31]]);
    assert_eq!(replaced(&result), [true, false, false, false]);
    assert!(result.cyclic_groups.is_empty());
}

#[test]
fn validation_checks_terms_hidden_by_constants_and_complements() {
    for mut terms in [vec![Different; 20], vec![Replaced(0), Retained(0)]] {
        terms.push(Retained(99));
        assert_eq!(
            decide(&[terms], DecisionLimits::default()),
            Err(DecisionError::InvalidDependency {
                node: 0,
                dependency: 99
            })
        );
    }
}

#[test]
fn aggregate_term_bound_counts_duplicates_and_clamps_caller_limits() {
    let equations = [vec![Same; 9], vec![Different; 8], vec![Replaced(0); 7]];
    let expected = run(&equations);
    assert_eq!(
        decide(
            &equations,
            DecisionLimits {
                max_terms: 24,
                ..DecisionLimits::default()
            }
        )
        .unwrap(),
        expected
    );
    assert_eq!(
        decide(
            &equations,
            DecisionLimits {
                max_terms: 23,
                ..DecisionLimits::default()
            }
        ),
        Err(DecisionError::TermLimit { maximum: 23 })
    );
    assert!(
        decide(
            &[Vec::<MismatchTerm>::new()],
            DecisionLimits {
                max_terms: 0,
                ..DecisionLimits::default()
            }
        )
        .is_ok()
    );
    let over = [vec![Same; MAX_TERMS + 1]];
    assert_eq!(
        decide(
            &over,
            DecisionLimits {
                max_terms: usize::MAX,
                max_work: usize::MAX,
                ..DecisionLimits::default()
            }
        ),
        Err(DecisionError::TermLimit { maximum: MAX_TERMS })
    );
    let at = [vec![Same; MAX_TERMS]];
    assert!(!run(&at).nodes[0].replace);
}

#[test]
fn exact_work_bound_covers_high_degree_graphs_in_every_phase() {
    let cases = [
        vec![vec![Same; 19]],
        vec![vec![Different; 19]],
        vec![vec![Replaced(0); 19]],
        vec![vec![Retained(0); 19]],
        (0..20)
            .map(|node| (1..=16).map(|step| Retained((node + step) % 20)).collect())
            .collect(),
    ];
    for equations in cases {
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
        assert_eq!(
            decide(
                &equations,
                DecisionLimits {
                    max_work: expected.work - 1,
                    ..DecisionLimits::default()
                }
            ),
            Err(DecisionError::WorkLimit {
                maximum: expected.work - 1
            })
        );
        for maximum in 0..expected.work.min(100) {
            assert!(matches!(
                decide(
                    &equations,
                    DecisionLimits {
                        max_work: maximum,
                        ..DecisionLimits::default()
                    }
                ),
                Err(DecisionError::WorkLimit { .. })
            ));
        }
    }
}

#[test]
fn wide_constant_propagation_and_duplicate_folding_have_linear_work() {
    let mut previous = None;
    for count in [10_000, 20_000] {
        // One consumer waits for all distinct false dependencies. Rescanning
        // its remaining terms at each propagation would take quadratic work.
        let mut equations = vec![Vec::new(); count + 1];
        equations[0] = (1..=count).map(Replaced).collect();
        let result = run(&equations);
        assert!(result.nodes.iter().all(|node| !node.replace));
        // Eight node passes plus six term/edge passes: no degree-dependent
        // rescan is hidden in the accounted propagation work.
        assert_eq!(result.work, 14 * count + 8);
        if let Some(work) = previous {
            assert_eq!(result.work - 8, 2 * (work - 8));
        }
        previous = Some(result.work);
    }
    let short = run(&[vec![Replaced(0); 4096]]);
    let long = run(&[vec![Replaced(0); 8192]]);
    assert_eq!(long.work - short.work, 3 * 4096);
    assert_eq!(replaced(&long), [false]);
}

fn truth(terms: &[MismatchTerm], assignment: usize) -> bool {
    terms.iter().any(|term| match *term {
        Same => false,
        Different => true,
        Replaced(index) => assignment & (1 << index) != 0,
        Retained(index) => assignment & (1 << index) == 0,
    })
}

/// Small independent oracle: exhaustive truth tables discover constants and
/// functional dependencies; transitive closure finds the one residual SCC
/// partition. It does not reuse folding, propagation, Tarjan or evaluation.
fn oracle(equations: &[Vec<MismatchTerm>]) -> (Vec<bool>, Vec<Vec<usize>>) {
    let count = equations.len();
    let mut known = vec![None; count];
    let consistent = |mask: usize, values: &[Option<bool>]| {
        values
            .iter()
            .enumerate()
            .all(|(n, value)| value.is_none_or(|v| (mask & (1 << n) != 0) == v))
    };
    loop {
        let mut changed = false;
        for node in 0..count {
            if known[node].is_some() {
                continue;
            }
            let mut possible = [false; 2];
            for mask in 0..1 << count {
                if consistent(mask, &known) {
                    possible[usize::from(truth(&equations[node], mask))] = true;
                }
            }
            if possible[0] != possible[1] {
                known[node] = Some(possible[1]);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut edges = vec![vec![false; count]; count];
    for node in 0..count {
        if known[node].is_some() {
            continue;
        }
        for dependency in 0..count {
            if known[dependency].is_some() {
                continue;
            }
            edges[node][dependency] = (0..1 << count).any(|mask| {
                consistent(mask, &known)
                    && truth(&equations[node], mask)
                        != truth(&equations[node], mask ^ (1 << dependency))
            });
        }
    }
    let mut reach = edges.clone();
    for (n, row) in reach.iter_mut().enumerate() {
        row[n] = true;
    }
    for via in 0..count {
        for from in 0..count {
            for to in 0..count {
                let through = reach[from][via] && reach[via][to];
                reach[from][to] |= through;
            }
        }
    }
    let mut groups = Vec::new();
    let mut assigned = vec![false; count];
    for node in 0..count {
        if known[node].is_some() || assigned[node] {
            continue;
        }
        let group: Vec<_> = (node..count)
            .filter(|&other| known[other].is_none() && reach[node][other] && reach[other][node])
            .collect();
        for &member in &group {
            assigned[member] = true;
        }
        groups.push(group);
    }
    let mut conservative = Vec::new();
    while groups.iter().any(|group| known[group[0]].is_none()) {
        let group = groups
            .iter()
            .find(|group| {
                known[group[0]].is_none()
                    && group.iter().all(|&node| {
                        (0..count).all(|dependency| {
                            !edges[node][dependency]
                                || group.contains(&dependency)
                                || known[dependency].is_some()
                        })
                    })
            })
            .expect("condensation has a dependency-first component");
        let selected = [false, true].into_iter().find(|candidate| {
            let mask = (0..count).fold(0, |mask, n| {
                let value = if group.contains(&n) {
                    *candidate
                } else {
                    known[n].unwrap_or(false)
                };
                mask | (usize::from(value) << n)
            });
            group
                .iter()
                .all(|&node| truth(&equations[node], mask) == *candidate)
        });
        if selected.is_none() {
            conservative.push(group.clone());
        }
        for &node in group {
            known[node] = Some(selected.unwrap_or(true));
        }
    }
    conservative.sort_by_key(|group| group[0]);
    (
        known.into_iter().map(Option::unwrap).collect(),
        conservative,
    )
}

#[test]
fn variable_equations_agree_with_exhaustive_small_graph_policy_oracle() {
    let mut seed = 0x7823_5239_41ad_8961_u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        seed >> 32
    };
    for case in 0..600 {
        let count = 1 + (next() as usize % 6);
        let equations: Vec<Vec<_>> = (0..count)
            .map(|_| {
                let degree = next() as usize % 25;
                let polarity = next();
                (0..degree)
                    .map(|_| {
                        let choice = next() as usize % (2 * count + 4);
                        match choice {
                            0 => Same,
                            1 if case % 3 == 0 => Different,
                            _ => {
                                let dependency = choice % count;
                                let negative = if case % 2 == 0 {
                                    polarity & (1 << dependency) != 0
                                } else {
                                    next() & 1 != 0
                                };
                                if negative {
                                    Retained(dependency)
                                } else {
                                    Replaced(dependency)
                                }
                            }
                        }
                    })
                    .collect()
            })
            .collect();
        let expected = oracle(&equations);
        let actual = run(&equations);
        assert_eq!(
            (replaced(&actual), actual.cyclic_groups),
            expected,
            "case {case}: {equations:?}"
        );
    }
}
