use super::*;
use std::cell::Cell;
use std::rc::Rc;

fn binding(path: &str, label: &str) -> Binding<char, String> {
    Binding {
        path: path.chars().collect(),
        label: label.into(),
        value: label.into(),
    }
}

fn prefix(path: &str, label: &str) -> Prefix<char, usize> {
    Prefix {
        path: path.chars().collect(),
        label: label.into(),
        value: path.len(),
    }
}

fn error(
    bindings: Vec<Binding<char, String>>,
    prefixes: Vec<Prefix<char, usize>>,
) -> CompileError<char> {
    Trie::compile(bindings, prefixes).unwrap_err()
}

#[test]
fn runtime_resolution_keeps_prefix_notification_separate_from_execution() {
    let trie = Trie::compile(
        vec![
            binding("gh", "left"),
            binding("gl", "right"),
            binding("x", "cut"),
        ],
        vec![prefix("g", "go")],
    )
    .unwrap();
    let root = trie.resolve(&[]).unwrap();
    assert!(root.terminal().is_none());
    assert!(root.prefix().is_none());
    assert_eq!(
        root.children().map(|(key, _)| *key).collect::<Vec<_>>(),
        ['g', 'x']
    );
    let branch = trie.resolve(&['g']).unwrap();
    assert!(branch.terminal().is_none());
    assert_eq!(branch.prefix().unwrap().label, "go");
    assert_eq!(branch.prefix().unwrap().value, 1);
    assert_eq!(
        branch.children().map(|(key, _)| *key).collect::<Vec<_>>(),
        ['h', 'l']
    );
    let terminal = trie.resolve(&['g', 'l']).unwrap();
    assert_eq!(terminal.terminal().unwrap().value, "right");
    assert_eq!(terminal.terminal().unwrap().path, ['g', 'l']);
    assert!(terminal.prefix().is_none());
    assert_eq!(terminal.children().len(), 0);
    let (_, child) = branch.children().next().unwrap();
    assert_eq!(child.terminal().unwrap().label, "left");
}

#[test]
fn unannotated_branches_resolve_but_missing_continuations_never_fall_back() {
    let trie = Trie::<_, _, ()>::compile(vec![binding("abc", "deep"), binding("x", "cut")], vec![])
        .unwrap();
    assert!(trie.resolve(&['a']).unwrap().prefix().is_none());
    assert!(trie.resolve(&['a', 'b']).unwrap().terminal().is_none());
    for path in [
        vec!['z'],
        vec!['a', 'x'],
        vec!['x', 'x'],
        vec!['a', 'b', 'c', 'd'],
    ] {
        assert!(trie.resolve(&path).is_none());
    }
    assert!(trie.resolve(&['a'; MAX_PATH_LEN + 1]).is_none());
}

#[test]
fn empty_map_has_a_nonexecuting_root_and_prefixes_cannot_create_ghost_nodes() {
    let trie = Trie::<char, (), ()>::compile(vec![], vec![]).unwrap();
    let root = trie.resolve(&[]).unwrap();
    assert!(root.terminal().is_none() && root.prefix().is_none());
    assert_eq!(root.children().len(), 0);
    assert!(trie.resolve(&['g']).is_none());
    assert_eq!(
        error(vec![], vec![prefix("g", "ghost")]).to_string(),
        "Missing prefix branch: prefix \"ghost\" at ['g'] has no binding continuation"
    );
}

#[test]
fn empty_binding_and_annotation_paths_report_the_named_definition() {
    assert_eq!(
        error(vec![binding("", "root action")], vec![]).to_string(),
        "Empty key path for binding \"root action\" at []"
    );
    assert_eq!(
        error(vec![binding("gg", "start")], vec![prefix("", "root hint")]).to_string(),
        "Empty key path for prefix \"root hint\" at []"
    );
}

#[test]
fn duplicate_terminal_diagnostic_names_both_paths_independently_of_declaration_order() {
    for reversed in [false, true] {
        let mut rules = vec![binding("gg", "alpha"), binding("gg", "zeta")];
        if reversed {
            rules.reverse();
        }
        let error = error(rules, vec![]);
        assert!(matches!(error, CompileError::DuplicateTerminal { .. }));
        assert_eq!(
            error.to_string(),
            "Duplicate terminal: binding \"alpha\" at ['g', 'g'] conflicts with binding \"zeta\" at ['g', 'g']"
        );
    }
}

#[test]
fn terminal_prefix_ambiguity_rejects_shorter_first_and_longer_first_identically() {
    for reversed in [false, true] {
        let mut rules = vec![binding("g", "go now"), binding("gg", "go to start")];
        if reversed {
            rules.reverse();
        }
        let error = error(rules, vec![]);
        assert!(matches!(error, CompileError::TerminalIsPrefix { .. }));
        assert_eq!(
            error.to_string(),
            "Terminal/prefix conflict: binding \"go now\" at ['g'] is also a prefix of binding \"go to start\" at ['g', 'g']"
        );
    }
}

#[test]
fn duplicate_prefix_annotations_name_both_declarations_in_either_order() {
    for reversed in [false, true] {
        let mut prefixes = vec![prefix("g", "alpha hint"), prefix("g", "zeta hint")];
        if reversed {
            prefixes.reverse();
        }
        let error = error(vec![binding("gg", "start")], prefixes);
        assert!(matches!(error, CompileError::DuplicatePrefix { .. }));
        assert_eq!(
            error.to_string(),
            "Duplicate prefix annotation: prefix \"alpha hint\" at ['g'] conflicts with prefix \"zeta hint\" at ['g']"
        );
    }
}

#[test]
fn annotations_reject_terminal_paths_and_nonexistent_continuations() {
    assert_eq!(
        error(vec![binding("g", "go")], vec![prefix("g", "go hint")]).to_string(),
        "Prefix annotation at a terminal: prefix \"go hint\" at ['g'] conflicts with binding \"go\" at ['g']"
    );
    for path in ["x", "gg", "gx"] {
        let error = error(vec![binding("g", "go")], vec![prefix(path, "missing")]);
        assert!(matches!(error, CompileError::MissingPrefix { .. }));
    }
}

#[test]
fn children_follow_first_binding_declaration_without_annotation_reordering() {
    let trie = Trie::compile(
        vec![
            binding("zh", "one"),
            binding("ag", "two"),
            binding("zl", "three"),
            binding("ab", "four"),
        ],
        vec![prefix("a", "A"), prefix("z", "Z")],
    )
    .unwrap();
    assert_eq!(
        trie.resolve(&[])
            .unwrap()
            .children()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        ['z', 'a']
    );
    assert_eq!(
        trie.resolve(&['z'])
            .unwrap()
            .children()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        ['h', 'l']
    );
    assert_eq!(
        trie.resolve(&['a'])
            .unwrap()
            .children()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        ['g', 'b']
    );
    for reversed in [false, true] {
        let mut bindings = vec![
            binding("zh", "one"),
            binding("ag", "two"),
            binding("zl", "three"),
            binding("ab", "four"),
        ];
        let mut prefixes = vec![prefix("a", "A"), prefix("z", "Z")];
        if reversed {
            bindings.reverse();
            prefixes.reverse();
        }
        let trie = Trie::compile(bindings, prefixes).unwrap();
        for (path, value) in [
            ("zh", "one"),
            ("ag", "two"),
            ("zl", "three"),
            ("ab", "four"),
        ] {
            assert_eq!(
                trie.resolve(&path.chars().collect::<Vec<_>>())
                    .unwrap()
                    .terminal()
                    .unwrap()
                    .value,
                value
            );
        }
    }
}

#[test]
fn compilation_moves_payloads_without_requiring_clone_or_debug() {
    #[derive(Clone, PartialEq, Eq)]
    struct Key(u8);
    struct Payload(u8);
    let result = Trie::compile(
        vec![Binding {
            path: vec![Key(1), Key(2)],
            label: "action".into(),
            value: Payload(7),
        }],
        vec![Prefix {
            path: vec![Key(1)],
            label: "hint".into(),
            value: Payload(9),
        }],
    );
    let trie = match result {
        Ok(trie) => trie,
        Err(_) => panic!("Valid non-Debug keys and payloads were rejected"),
    };
    assert_eq!(
        trie.resolve(&[Key(1)]).unwrap().prefix().unwrap().value.0,
        9
    );
    assert_eq!(
        trie.resolve(&[Key(1), Key(2)])
            .unwrap()
            .terminal()
            .unwrap()
            .value
            .0,
        7
    );
}

#[derive(Debug)]
struct CountedKey {
    value: usize,
    clones: Rc<Cell<usize>>,
}
impl Clone for CountedKey {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self {
            value: self.value,
            clones: Rc::clone(&self.clones),
        }
    }
}
impl PartialEq for CountedKey {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for CountedKey {}

fn key(value: usize, clones: &Rc<Cell<usize>>) -> CountedKey {
    CountedKey {
        value,
        clones: Rc::clone(clones),
    }
}

fn limit_without_key_clones(
    bindings: Vec<Binding<CountedKey, ()>>,
    prefixes: Vec<Prefix<CountedKey, ()>>,
    clones: &Cell<usize>,
    expected: Resource,
) {
    let error = Trie::compile(bindings, prefixes).unwrap_err();
    assert!(matches!(error, CompileError::LimitExceeded { resource, .. } if resource == expected));
    assert_eq!(clones.get(), 0);
}

#[test]
fn rule_limits_reject_before_key_cloning_or_duplicate_diagnostics() {
    let clones = Rc::new(Cell::new(0));
    let bindings = (0..=MAX_BINDINGS)
        .map(|_| Binding {
            path: vec![key(1, &clones)],
            label: "duplicate".into(),
            value: (),
        })
        .collect();
    limit_without_key_clones(bindings, vec![], &clones, Resource::Bindings);
    let prefixes = (0..=MAX_PREFIXES)
        .map(|_| Prefix {
            path: vec![key(1, &clones)],
            label: "duplicate".into(),
            value: (),
        })
        .collect();
    limit_without_key_clones(vec![], prefixes, &clones, Resource::Prefixes);
}

#[test]
fn path_and_utf8_label_limits_cover_both_definition_kinds_before_cloning() {
    let clones = Rc::new(Cell::new(0));
    let path = || {
        (0..=MAX_PATH_LEN)
            .map(|value| key(value, &clones))
            .collect()
    };
    limit_without_key_clones(
        vec![Binding {
            path: path(),
            label: "long path".into(),
            value: (),
        }],
        vec![],
        &clones,
        Resource::PathLength,
    );
    limit_without_key_clones(
        vec![],
        vec![Prefix {
            path: path(),
            label: "long path".into(),
            value: (),
        }],
        &clones,
        Resource::PathLength,
    );
    let label = format!("{}x", "é".repeat(MAX_LABEL_BYTES / 2));
    assert_eq!(label.len(), MAX_LABEL_BYTES + 1);
    limit_without_key_clones(
        vec![Binding {
            path: vec![key(1, &clones)],
            label: label.clone(),
            value: (),
        }],
        vec![],
        &clones,
        Resource::LabelBytes,
    );
    limit_without_key_clones(
        vec![],
        vec![Prefix {
            path: vec![key(1, &clones)],
            label,
            value: (),
        }],
        &clones,
        Resource::LabelBytes,
    );
    let trie = Trie::<_, (), ()>::compile(
        vec![Binding {
            path: vec!['x'; MAX_PATH_LEN],
            label: "é".repeat(MAX_LABEL_BYTES / 2),
            value: (),
        }],
        vec![],
    )
    .unwrap();
    assert!(
        trie.resolve(&['x'; MAX_PATH_LEN])
            .unwrap()
            .terminal()
            .is_some()
    );
}

#[test]
fn node_limit_precedes_even_an_earlier_semantic_conflict_and_key_cloning() {
    let clones = Rc::new(Cell::new(0));
    let make = |value| Binding {
        path: std::iter::once(key(value, &clones))
            .chain((1..MAX_PATH_LEN).map(|_| key(0, &clones)))
            .collect(),
        label: "branch".into(),
        value: (),
    };
    let mut bindings = vec![make(0), make(0)];
    bindings.extend((1..256).map(make));
    limit_without_key_clones(bindings, vec![], &clones, Resource::Nodes);
}

#[test]
fn actual_node_limit_accepts_the_boundary_and_rejects_the_next_node_in_either_order() {
    for over in [false, true] {
        for reversed in [false, true] {
            let mut bindings = (0..256)
                .map(|branch| {
                    let len = if branch == 255 && !over {
                        MAX_PATH_LEN - 1
                    } else {
                        MAX_PATH_LEN
                    };
                    Binding {
                        path: (0..len).map(|depth| (branch, depth)).collect(),
                        label: "branch".into(),
                        value: (),
                    }
                })
                .collect::<Vec<_>>();
            if reversed {
                bindings.reverse();
            }
            match Trie::<_, (), ()>::compile(bindings, vec![]) {
                Ok(trie) => {
                    assert!(!over);
                    assert_eq!(trie.nodes.len(), MAX_NODES);
                }
                Err(error) => {
                    assert!(over);
                    assert_eq!(
                        error,
                        CompileError::LimitExceeded {
                            resource: Resource::Nodes,
                            limit: MAX_NODES,
                            actual: MAX_NODES + 1
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn shared_prefixes_admit_maximum_rules_without_pessimistic_node_accounting() {
    for reversed in [false, true] {
        let mut bindings = (0..MAX_BINDINGS)
            .map(|branch| Binding {
                path: (0..MAX_PATH_LEN - 1)
                    .map(|depth| (0, depth))
                    .chain(std::iter::once((branch + 1, MAX_PATH_LEN - 1)))
                    .collect(),
                label: "shared".into(),
                value: branch,
            })
            .collect::<Vec<_>>();
        if reversed {
            bindings.reverse();
        }
        let trie = Trie::<_, _, ()>::compile(bindings, vec![]).unwrap();
        assert_eq!(trie.nodes.len(), MAX_PATH_LEN + MAX_BINDINGS);
        let shared = (0..MAX_PATH_LEN - 1)
            .map(|depth| (0, depth))
            .collect::<Vec<_>>();
        assert_eq!(
            trie.resolve(&shared).unwrap().children().len(),
            MAX_BINDINGS
        );
    }
}

#[test]
fn maximum_prefix_annotations_attach_without_allocating_extra_nodes() {
    let bindings = (0..MAX_PREFIXES)
        .map(|branch| Binding {
            path: vec![branch, usize::MAX],
            label: "action".into(),
            value: (),
        })
        .collect();
    let prefixes = (0..MAX_PREFIXES)
        .rev()
        .map(|branch| Prefix {
            path: vec![branch],
            label: "hint".into(),
            value: branch,
        })
        .collect();
    let trie = Trie::compile(bindings, prefixes).unwrap();
    assert_eq!(trie.nodes.len(), 1 + MAX_PREFIXES * 2);
    for branch in 0..MAX_PREFIXES {
        assert_eq!(
            trie.resolve(&[branch]).unwrap().prefix().unwrap().value,
            branch
        );
    }
}
