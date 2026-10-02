//! Bounded declarative key paths, independent of input ownership and timing.
//!
//! A terminal executes; a prefix annotation only describes a proper branch.
//! Nodes own indices, never child nodes, so traversal and destruction are flat.
//! Limits cover this container's keys, labels and nodes, not arbitrary payloads.

use std::fmt;

// These exceed the native vocabulary while bounding both storage and the
// allocation-free, quadratic validation pass. Key sequences longer than 16
// strokes are outside the intended interactive command vocabulary.
pub const MAX_BINDINGS: usize = 512;
pub const MAX_PREFIXES: usize = 128;
pub const MAX_PATH_LEN: usize = 16;
pub const MAX_NODES: usize = 4096;
pub const MAX_LABEL_BYTES: usize = 256;

#[derive(Debug)]
pub struct Binding<K, T> {
    pub path: Vec<K>,
    pub label: String,
    pub value: T,
}

#[derive(Debug)]
pub struct Prefix<K, P> {
    pub path: Vec<K>,
    pub label: String,
    pub value: P,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefinitionKind {
    Binding,
    Prefix,
}

impl fmt::Display for DefinitionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Binding => "binding",
            Self::Prefix => "prefix",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabeledPath<K> {
    pub kind: DefinitionKind,
    pub path: Vec<K>,
    pub label: String,
}

impl<K: fmt::Debug> fmt::Display for LabeledPath<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?} at {:?}", self.kind, self.label, self.path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Bindings,
    Prefixes,
    PathLength,
    LabelBytes,
    Nodes,
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Bindings => "binding count",
            Self::Prefixes => "prefix count",
            Self::PathLength => "key path length",
            Self::LabelBytes => "label bytes",
            Self::Nodes => "node count",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError<K> {
    LimitExceeded {
        resource: Resource,
        limit: usize,
        actual: usize,
    },
    SizeOverflow,
    EmptyPath {
        definition: LabeledPath<K>,
    },
    DuplicateTerminal {
        first: LabeledPath<K>,
        second: LabeledPath<K>,
    },
    TerminalIsPrefix {
        terminal: LabeledPath<K>,
        continuation: LabeledPath<K>,
    },
    DuplicatePrefix {
        first: LabeledPath<K>,
        second: LabeledPath<K>,
    },
    MissingPrefix {
        prefix: LabeledPath<K>,
    },
    PrefixAtTerminal {
        prefix: LabeledPath<K>,
        terminal: LabeledPath<K>,
    },
}

impl<K: fmt::Debug> fmt::Display for CompileError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => {
                write!(
                    f,
                    "Binding trie {resource} limit exceeded: {actual} > {limit}"
                )
            }
            Self::SizeOverflow => f.write_str("Binding trie size exceeds usize"),
            Self::EmptyPath { definition } => write!(f, "Empty key path for {definition}"),
            Self::DuplicateTerminal { first, second } => {
                write!(f, "Duplicate terminal: {first} conflicts with {second}")
            }
            Self::TerminalIsPrefix {
                terminal,
                continuation,
            } => {
                write!(
                    f,
                    "Terminal/prefix conflict: {terminal} is also a prefix of {continuation}"
                )
            }
            Self::DuplicatePrefix { first, second } => {
                write!(
                    f,
                    "Duplicate prefix annotation: {first} conflicts with {second}"
                )
            }
            Self::MissingPrefix { prefix } => {
                write!(
                    f,
                    "Missing prefix branch: {prefix} has no binding continuation"
                )
            }
            Self::PrefixAtTerminal { prefix, terminal } => {
                write!(
                    f,
                    "Prefix annotation at a terminal: {prefix} conflicts with {terminal}"
                )
            }
        }
    }
}

impl<K: fmt::Debug> std::error::Error for CompileError<K> {}

#[derive(Debug)]
struct Edge<K> {
    key: K,
    node: usize,
}

#[derive(Debug)]
struct Node<K> {
    terminal: Option<usize>,
    prefix: Option<usize>,
    children: Vec<Edge<K>>,
}

impl<K> Node<K> {
    fn new() -> Self {
        Self {
            terminal: None,
            prefix: None,
            children: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Trie<K, T, P> {
    nodes: Vec<Node<K>>,
    bindings: Vec<Binding<K, T>>,
    prefixes: Vec<Prefix<K, P>>,
}

pub struct NodeRef<'a, K, T, P> {
    trie: &'a Trie<K, T, P>,
    node: usize,
}

impl<K, T, P> Copy for NodeRef<'_, K, T, P> {}
impl<K, T, P> Clone for NodeRef<'_, K, T, P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, K, T, P> NodeRef<'a, K, T, P> {
    pub fn terminal(&self) -> Option<&'a Binding<K, T>> {
        self.trie.nodes[self.node]
            .terminal
            .map(|index| &self.trie.bindings[index])
    }

    pub fn prefix(&self) -> Option<&'a Prefix<K, P>> {
        self.trie.nodes[self.node]
            .prefix
            .map(|index| &self.trie.prefixes[index])
    }

    /// Children follow the first binding declaration that introduced each edge.
    /// Prefix annotation order never changes navigation or this presentation order.
    pub fn children(&self) -> impl ExactSizeIterator<Item = (&'a K, Self)> + 'a {
        let trie = self.trie;
        trie.nodes[self.node].children.iter().map(move |edge| {
            (
                &edge.key,
                Self {
                    trie,
                    node: edge.node,
                },
            )
        })
    }
}

impl<K: Clone + Eq, T, P> Trie<K, T, P> {
    /// Input vectors already belong to the caller. Every resource bound is
    /// checked before this function allocates nodes or clones keys or labels,
    /// including the bounded diagnostic copies used for semantic failures.
    pub fn compile(
        bindings: Vec<Binding<K, T>>,
        prefixes: Vec<Prefix<K, P>>,
    ) -> Result<Self, CompileError<K>> {
        let node_count = admit_resources(&bindings, &prefixes)?;
        validate(&bindings, &prefixes)?;
        let mut nodes = Vec::<Node<K>>::with_capacity(node_count);
        nodes.push(Node::new());
        for (index, binding) in bindings.iter().enumerate() {
            let mut node = 0;
            for key in &binding.path {
                node = match nodes[node].children.iter().find(|edge| &edge.key == key) {
                    Some(edge) => edge.node,
                    None => {
                        let next = nodes.len();
                        nodes.push(Node::new());
                        nodes[node].children.push(Edge {
                            key: key.clone(),
                            node: next,
                        });
                        next
                    }
                };
            }
            nodes[node].terminal = Some(index);
        }
        for (index, prefix) in prefixes.iter().enumerate() {
            let node = resolve_index(&nodes, &prefix.path)
                .expect("validated prefix annotations name existing branches");
            nodes[node].prefix = Some(index);
        }
        Ok(Self {
            nodes,
            bindings,
            prefixes,
        })
    }

    /// Empty input resolves to the root; an unbound continuation never falls
    /// back to an earlier prefix or terminal. Resolution allocates nothing.
    pub fn resolve(&self, path: &[K]) -> Option<NodeRef<'_, K, T, P>> {
        if path.len() > MAX_PATH_LEN {
            return None;
        }
        resolve_index(&self.nodes, path).map(|node| NodeRef { trie: self, node })
    }
}

fn resolve_index<K: Eq>(nodes: &[Node<K>], path: &[K]) -> Option<usize> {
    let mut node = 0;
    for key in path {
        node = nodes[node]
            .children
            .iter()
            .find(|edge| &edge.key == key)?
            .node;
    }
    Some(node)
}

fn limit<K>(resource: Resource, actual: usize, maximum: usize) -> Result<(), CompileError<K>> {
    if actual > maximum {
        Err(CompileError::LimitExceeded {
            resource,
            limit: maximum,
            actual,
        })
    } else {
        Ok(())
    }
}

/// This pass needs no Clone bound and performs no internal allocation. Count
/// distinct binding prefixes directly; summing lengths would wrongly reject
/// dense maps with many shared prefixes. Annotations cannot introduce nodes.
fn admit_resources<K: Eq, T, P>(
    bindings: &[Binding<K, T>],
    prefixes: &[Prefix<K, P>],
) -> Result<usize, CompileError<K>> {
    limit(Resource::Bindings, bindings.len(), MAX_BINDINGS)?;
    limit(Resource::Prefixes, prefixes.len(), MAX_PREFIXES)?;
    for (path, label) in bindings
        .iter()
        .map(|rule| (&rule.path, &rule.label))
        .chain(prefixes.iter().map(|rule| (&rule.path, &rule.label)))
    {
        limit(Resource::PathLength, path.len(), MAX_PATH_LEN)?;
        limit(Resource::LabelBytes, label.len(), MAX_LABEL_BYTES)?;
    }
    let mut nodes = 1_usize;
    for (index, binding) in bindings.iter().enumerate() {
        for length in 1..=binding.path.len() {
            let path = &binding.path[..length];
            if !bindings[..index]
                .iter()
                .any(|prior| prior.path.starts_with(path))
            {
                nodes = nodes.checked_add(1).ok_or(CompileError::SizeOverflow)?;
                limit(Resource::Nodes, nodes, MAX_NODES)?;
            }
        }
    }
    Ok(nodes)
}

fn labeled<K: Clone>(kind: DefinitionKind, path: &[K], label: &str) -> LabeledPath<K> {
    LabeledPath {
        kind,
        path: path.to_vec(),
        label: label.to_owned(),
    }
}

fn binding_path<K: Clone, T>(binding: &Binding<K, T>) -> LabeledPath<K> {
    labeled(DefinitionKind::Binding, &binding.path, &binding.label)
}

fn prefix_path<K: Clone, P>(prefix: &Prefix<K, P>) -> LabeledPath<K> {
    labeled(DefinitionKind::Prefix, &prefix.path, &prefix.label)
}

fn validate<K: Clone + Eq, T, P>(
    bindings: &[Binding<K, T>],
    prefixes: &[Prefix<K, P>],
) -> Result<(), CompileError<K>> {
    for binding in bindings {
        if binding.path.is_empty() {
            return Err(CompileError::EmptyPath {
                definition: binding_path(binding),
            });
        }
    }
    for prefix in prefixes {
        if prefix.path.is_empty() {
            return Err(CompileError::EmptyPath {
                definition: prefix_path(prefix),
            });
        }
    }
    for (index, binding) in bindings.iter().enumerate() {
        for prior in &bindings[..index] {
            if binding.path == prior.path {
                let (first, second) = if prior.label <= binding.label {
                    (prior, binding)
                } else {
                    (binding, prior)
                };
                return Err(CompileError::DuplicateTerminal {
                    first: binding_path(first),
                    second: binding_path(second),
                });
            }
            let (short, long) = if prior.path.len() < binding.path.len() {
                (prior, binding)
            } else {
                (binding, prior)
            };
            if long.path.starts_with(&short.path) {
                return Err(CompileError::TerminalIsPrefix {
                    terminal: binding_path(short),
                    continuation: binding_path(long),
                });
            }
        }
    }
    for (index, prefix) in prefixes.iter().enumerate() {
        if let Some(prior) = prefixes[..index]
            .iter()
            .find(|prior| prior.path == prefix.path)
        {
            let (first, second) = if prior.label <= prefix.label {
                (prior, prefix)
            } else {
                (prefix, prior)
            };
            return Err(CompileError::DuplicatePrefix {
                first: prefix_path(first),
                second: prefix_path(second),
            });
        }
        if let Some(terminal) = bindings.iter().find(|binding| binding.path == prefix.path) {
            return Err(CompileError::PrefixAtTerminal {
                prefix: prefix_path(prefix),
                terminal: binding_path(terminal),
            });
        }
        if !bindings
            .iter()
            .any(|binding| binding.path.starts_with(&prefix.path))
        {
            return Err(CompileError::MissingPrefix {
                prefix: prefix_path(prefix),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
