# Declarative editor bindings

Normal and timeline Visual editor paths compile from
[`editor_map.rs`](../crates/deadpan-app/src/navigation/editor_map.rs) through the
bounded [`binding_trie.rs`](../crates/deadpan-app/src/navigation/binding_trie.rs).
Each terminal carries a typed action, count policy, short explanation and held-key
policy. Proper prefixes carry separate notifications and semantic capture roles.
The native project service and authored commands are unchanged.

## Input ownership

The host gives native controls, text and IME their existing priority before an
editor command can run. Platform menu shortcuts and logical-symbol handling
remain explicit in the outer router. Shift/Option needed to type punctuation
never turns a reserved Control/Command chord into plain input. Camera, Trim,
Gain, room-tone and Place slice keep their own mode routers.

Mark names resolve before ordinary editor actions. `mx`, `'d` and uppercase
names remain marks. Escape, Tab and native menu actions retain their defined
interrupt behavior. Transport and group navigation interrupt ordinary pending
paths, while a pending mark requires a letter. Invalid suffixes clear their
prefix without executing the suffix as a new root command.

The app observes semantic prefix transitions to capture exact mark and Trim
targets, including absent targets. Compilation and help never capture a target
or mutate a project. A delayed service reply cannot supply a target that was
absent on entry. Prefixes have no timer.

## Counts, repetition and teaching

Count policies preserve the existing distinctions between absent count, zero,
one, a larger count and overflow. Motions, Repeat, frame cuts, whole-beat cuts,
Holds and gain use their declared policies. A positive Repeat count still means
total plays. A second count after an operator is rejected.

Held events run only when the leaf at the current trie position explicitly
allows repetition. Plain frame/beat motion may repeat. Held input cannot complete
or consume a pending `g`, `r`, `d`, comma or mark prefix. In particular, holding
`h` while pressing comma no longer inserts a Hold. Releasing it and pressing
`h` explicitly completes the waiting `,h` once.

Prefix hints derive their next keys and explanations from the declarations.
Counted comma teaching exposes its valid Hold continuation. A count that leaves
no valid completion, such as `2d` or `0r`, displays the actual refusal instead of
advertising an edit. The root zero-count hint explains its unusual motion and
zero-time semantics. Native-control cut protection probes the typed action
without consuming pending input, preserving mark-name authority.

## Compiler and audit

The compiler rejects empty paths, duplicate terminals, and any terminal that is
also a prefix, in either declaration order. Prefix annotations must identify an
existing proper branch, are unique and never create a hidden command. Errors
identify both conflicting labels and complete typed key paths.

Limits are 512 terminal definitions, 128 prefix annotations, 16 keys per path,
4,096 distinct nodes including the root, and 256 UTF-8 bytes per label. Resource
preflight runs before internal allocation or diagnostic key/label cloning.
Shared prefixes count once. Flat indexed nodes avoid recursive ownership and
traversal. These bounds cover the compiler's container; callers still own input
allocation and arbitrary payload limits.

The Kestrel audit enumerates every structural branch in both compiled maps,
including branches without annotations. It tests each with absent, positive,
zero and overflowing counts, alongside text, composition, Visual selection and
the existing modal routers. The live registry digest remains a separate check.
See [shortcut compatibility](KEYBINDING_COMPATIBILITY.md).
The [qualification record](qualification/declarative-bindings-2026-10-01.md)
retains the old-version regression, final checks and their limits.

## Remaining work

User keymap loading, configuration UI, dynamic keycaps throughout the workspace,
and migration of the remaining mode routers are still required. A keymap loader
must validate the entire candidate before installation, reject Kestrel/native
conflicts, update teaching with execution, and cancel captured pending input on
map changes. Keymaps belong outside project content. Named registers, semantic
dot-repeat and atomic bounded macros remain separate DP-06 work.
