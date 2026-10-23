#!/usr/bin/env python3
"""Copy a source archive to DEST and apply narrowly anchored DP-24 timers.

Usage: python3 overlay.py --source /path/to/unpacked-archive --dest /tmp/deadpan-first-pause-profile
SOURCE must be a disposable archived checkout, never the shared repository.
"""
from __future__ import annotations
import argparse
from pathlib import Path
import shutil


def replace_once(root: Path, rel: str, old: str, new: str) -> None:
    path = root / rel
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"anchor mismatch {rel}: expected one exact occurrence, got {count}")
    path.write_text(text.replace(old, new, 1))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument('--source', type=Path, required=True)
    ap.add_argument('--dest', type=Path, required=True)
    a = ap.parse_args()
    src = a.source.resolve()
    dest = a.dest.resolve()
    if not (src / 'Cargo.toml').is_file() or src == Path('/Users/michael/Code/deadpan'):
        raise SystemExit('source must be an unpacked archive, never the shared checkout')
    if dest.exists():
        raise SystemExit(f'destination already exists: {dest}')
    def ignore(directory: str, names: list[str]) -> set[str]:
        return {name for name in names if name == '.git' or
                (name == 'target' and Path(directory).resolve() == src)}
    shutil.copytree(src, dest, ignore=ignore)
    # Core reducer: independently time initial whole-layout binding, capture/reanchor,
    # per-owner resolution/resume, binding validation, and leaf insert. These anchors
    # are the ordinary root-sequence path used by perf edit pause.
    rel = 'crates/deadpan-core/src/insert_time.rs'
    replace_once(dest, rel,
        '    let allocation = context.allocation;\n',
        '    let _dp_total = std::time::Instant::now();\n    let allocation = context.allocation;\n')
    replace_once(dest, rel,
        '    let (bindings, phase_layout) = crate::audio_binding_lifecycle::capture_for_insertion_scoped(\n',
        '    let _dp_capture = std::time::Instant::now();\n    let (bindings, phase_layout) = crate::audio_binding_lifecycle::capture_for_insertion_scoped(\n')
    replace_once(dest, rel,
        '        reanchored,\n    )?;\n    let mut working = document.clone();\n',
        '        reanchored,\n    )?;\n    eprintln!("DP24 core_capture_for_insertion_ms={:.3}", _dp_capture.elapsed().as_secs_f64()*1000.0);\n    let mut working = document.clone();\n')
    replace_once(dest, rel,
        '    for (index, shifted) in shifted.iter().enumerate() {\n',
        '    let _dp_resume = std::time::Instant::now();\n    for (index, shifted) in shifted.iter().enumerate() {\n')
    replace_once(dest, rel,
        '    crate::audio_binding_lifecycle::prune(&mut working);\n',
        '    eprintln!("DP24 core_resume_owner_loop_ms={:.3} owners={}", _dp_resume.elapsed().as_secs_f64()*1000.0, shifted.len());\n    crate::audio_binding_lifecycle::prune(&mut working);\n')
    replace_once(dest, rel,
        '    working.validate_bindings_in_scope(&working.audio_bindings)?;\n',
        '    let _dp_validate_bindings = std::time::Instant::now();\n    working.validate_bindings_in_scope(&working.audio_bindings)?;\n    eprintln!("DP24 core_new_binding_validation_ms={:.3}", _dp_validate_bindings.elapsed().as_secs_f64()*1000.0);\n')
    replace_once(dest, rel,
        '    let result = insert_leaf_shared(\n',
        '    let _dp_leaf = std::time::Instant::now();\n    let result = insert_leaf_shared(\n')
    replace_once(dest, rel,
        '    Ok((result, structure))\n',
        '    eprintln!("DP24 core_leaf_insert_ms={:.3} core_reducer_total_ms={:.3}", _dp_leaf.elapsed().as_secs_f64()*1000.0, _dp_total.elapsed().as_secs_f64()*1000.0);\n    Ok((result, structure))\n')
    # Core command boundary distinguishes apply_validated total (reducer + net diff /
    # result validation) while internal reducer timers above split its largest loops.
    rel = 'crates/deadpan-core/src/command.rs'
    replace_once(dest, rel,
        '    document\n        .scope(|| apply_with_durations(document.document(), request, Some(document)))\n        .map(|(edit, result, (durations, bindings))| {\n            (\n                edit,\n                crate::ValidatedDocument::from_validation(result, durations, bindings),\n            )\n        })\n',
        '    let _dp_apply = std::time::Instant::now();\n    let applied = document\n        .scope(|| apply_with_durations(document.document(), request, Some(document)))\n        .map(|(edit, result, (durations, bindings))| {\n            (\n                edit,\n                crate::ValidatedDocument::from_validation(result, durations, bindings),\n            )\n        });\n    eprintln!("DP24 core_apply_validated_total_ms={:.3}", _dp_apply.elapsed().as_secs_f64()*1000.0);\n    applied\n')
    replace_once(dest, rel,
        '    let (edit, validation) = net_transaction_known(\n        document,\n        &result,\n        description(&request.command),\n        previous,\n        known,\n    )?;\n',
        '    let _dp_net = std::time::Instant::now();\n    let (edit, validation) = net_transaction_known(\n        document,\n        &result,\n        description(&request.command),\n        previous,\n        known,\n    )?;\n    eprintln!("DP24 core_net_patch_and_result_validation_ms={:.3}", _dp_net.elapsed().as_secs_f64()*1000.0);\n')
    # Store separately times prepare (which includes core), write plan, SQLite commit/cache.
    rel = 'crates/deadpan-store/src/lib.rs'
    replace_once(dest, rel,
        '        let plan = prepare_command(&transaction, documents, request)?;\n        let (outcome, next) = write_command_plan(\n',
        '        let _dp_prepare = std::time::Instant::now();\n        let plan = prepare_command(&transaction, documents, request)?;\n        eprintln!("DP24 store_prepare_command_ms={:.3}", _dp_prepare.elapsed().as_secs_f64()*1000.0);\n        let _dp_write_plan = std::time::Instant::now();\n        let (outcome, next) = write_command_plan(\n')
    replace_once(dest, rel,
        '            self.context_resolver.as_deref(),\n        )?;\n        transaction.commit()?;\n        documents.insert(next);\n',
        '            self.context_resolver.as_deref(),\n        )?;\n        eprintln!("DP24 store_write_plan_ms={:.3}", _dp_write_plan.elapsed().as_secs_f64()*1000.0);\n        let _dp_sync = std::time::Instant::now();\n        transaction.commit()?;\n        eprintln!("DP24 store_sqlite_commit_ms={:.3}", _dp_sync.elapsed().as_secs_f64()*1000.0);\n        documents.insert(next);\n')
    # Perf harness prepare is command construction; commit includes Store; refresh is the
    # workspace/head + RenderPlan rebuild. Its own existing Samples measure these too.
    rel = 'crates/deadpan-cli/examples/perf/edit.rs'
    replace_once(dest, rel,
        '    samples.prepare.push(ms(started));\n    let committed = Instant::now();\n',
        '    samples.prepare.push(ms(started));\n    eprintln!("DP24 measure_begin");\n    let _dp_total = Instant::now();\n    let committed = Instant::now();\n')
    replace_once(dest, rel,
        '    refresh(store, document, plan)?;\n    samples.refresh.push(ms(refreshed));\n',
        '    refresh(store, document, plan)?;\n    eprintln!("DP24 perf_refresh_ms={:.3} perf_commit_plus_refresh_ms={:.3}", refreshed.elapsed().as_secs_f64()*1000.0, _dp_total.elapsed().as_secs_f64()*1000.0);\n    samples.refresh.push(ms(refreshed));\n    eprintln!("DP24 measure_end");\n')
    print(f'instrumented archive copy: {dest}')
    print('Run on a fresh 10k package: perf make-large <fresh-package> --beats 10000; then perf edit <fresh-package> --cycles 1 --kinds pause; retain stderr and binary/source hashes.')

if __name__ == '__main__':
    main()
