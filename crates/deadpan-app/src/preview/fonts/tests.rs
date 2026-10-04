use super::*;

fn frame(context: &egui::Context) {
    let mut output = context.run_ui(Default::default(), |_| {});
    // These CPU-only tests inspect glyph selection, not uploaded atlas pixels.
    // Explicitly consume the texture update; epaint rejects silently dropping it.
    output.textures_delta.clear();
}

#[test]
fn real_system_fonts_lead_their_families_and_fallbacks_cover_cjk() {
    let loaded = system();
    assert!(loaded.report.skipped.is_empty(), "{:?}", loaded.report);
    assert_eq!(loaded.report.loaded.len(), 4);
    assert!(
        loaded
            .report
            .loaded
            .iter()
            .map(|font| font.bytes)
            .sum::<usize>()
            <= MAX_TOTAL_BYTES
    );
    for font in &loaded.report.loaded {
        assert_eq!(font.face_index, 0);
        assert!(font.bytes > 0);
        assert_eq!(font.sha256.len(), 64);
    }

    let context = egui::Context::default();
    frame(&context);
    let sample = "Answer 0123456789 :group";
    let builtin: Vec<_> = context.fonts_mut(|fonts| {
        let id = egui::FontId::proportional(13.0);
        sample.chars().map(|c| fonts.glyph_width(&id, c)).collect()
    });
    install(&context);
    frame(&context);
    let defaults = FontDefinitions::default();
    for (family, primary) in [
        (FontFamily::Proportional, "Deadpan SF Pro"),
        (FontFamily::Monospace, "Deadpan SF Mono"),
    ] {
        context.fonts_mut(|fonts| {
            let id = egui::FontId::new(13.0, family.clone());
            // This is egui's actual fallback lookup, not text presence or a
            // nonempty tofu rectangle. Both command and ordinary label families
            // must resolve every sample to a real supported glyph.
            assert!(fonts.has_glyphs(&id, "答え日本語简体中文繁體한글한국어"));
            let installed = &fonts.definitions().families[&family];
            assert_eq!(installed[0], primary);
            assert!(installed[1..].starts_with(&defaults.families[&family]));
            assert_eq!(installed.len(), defaults.families[&family].len() + 3);
        });
    }
    // The system face replaces the built-in Latin metrics.
    let system: Vec<_> = context.fonts_mut(|fonts| {
        let id = egui::FontId::proportional(13.0);
        sample.chars().map(|c| fonts.glyph_width(&id, c)).collect()
    });
    assert_ne!(system, builtin);
    let report = context
        .data(|data| data.get_temp::<Report>(egui::Id::new(REPORT_KEY)))
        .unwrap();
    assert_eq!(
        serde_json::to_value(report).unwrap(),
        serde_json::to_value(&loaded.report).unwrap()
    );
}

#[test]
fn missing_oversized_directory_and_invalid_fonts_keep_builtins_with_bounded_diagnostics() {
    let scratch = tempfile::tempdir().unwrap();
    let missing = scratch.path().join("missing.ttf");
    let oversized = scratch.path().join("oversized.ttf");
    File::create(&oversized).unwrap().set_len(9).unwrap();
    let invalid = scratch.path().join("invalid.ttf");
    std::fs::write(&invalid, b"not-font").unwrap();
    let loaded = load(
        &[
            Candidate {
                name: "missing",
                path: &missing,
                index: 0,
                required: "答",
                role: Role::Fallback,
            },
            Candidate {
                name: "oversized",
                path: &oversized,
                index: 0,
                required: "答",
                role: Role::Fallback,
            },
            Candidate {
                name: "directory",
                path: scratch.path(),
                index: 0,
                required: "答",
                role: Role::Fallback,
            },
            Candidate {
                name: "invalid",
                path: &invalid,
                index: 0,
                required: "答",
                role: Role::Fallback,
            },
        ],
        8,
    );
    assert!(loaded.fonts.is_empty());
    assert!(loaded.report.loaded.is_empty());
    assert_eq!(loaded.report.skipped.len(), 4);
    assert!(loaded.report.skipped[0].reason.contains("cannot open"));
    assert!(loaded.report.skipped[1].reason.contains("byte budget"));
    assert!(loaded.report.skipped[2].reason.contains("regular file"));
    assert!(
        loaded.report.skipped[3]
            .reason
            .contains("invalid installed font face")
    );
    assert!(
        loaded
            .report
            .skipped
            .iter()
            .all(|failure| failure.reason.chars().count() <= MAX_DIAGNOSTIC_CHARS)
    );
    assert!(read_bounded(std::io::repeat(0), 8).is_err());
    assert_eq!(read_bounded(&b"12345678"[..], 8).unwrap(), b"12345678");

    let context = egui::Context::default();
    install_loaded(&context, &loaded);
    frame(&context);
    context.fonts(|fonts| assert_eq!(fonts.definitions(), &FontDefinitions::default()));
}

#[test]
fn face_index_and_required_glyphs_are_checked_before_egui_and_total_budget_is_enforced() {
    let loaded = system();
    let (_, _, data) = &loaded.fonts[2];
    assert!(validate(data.font.as_ref(), u32::MAX, "答").is_err());
    assert!(validate(data.font.as_ref(), 0, "\u{10ffff}").is_err());
    let budget = data.font.len();
    let path = Path::new(&loaded.report.loaded[2].path);
    let limited = load(
        &[
            Candidate {
                name: "first",
                path,
                index: 0,
                required: "答え",
                role: Role::Fallback,
            },
            Candidate {
                name: "second",
                path,
                index: 0,
                required: "答え",
                role: Role::Fallback,
            },
        ],
        budget,
    );
    assert_eq!(limited.fonts.len(), 1);
    assert_eq!(limited.report.loaded[0].bytes, budget);
    assert_eq!(limited.report.skipped.len(), 1);
    assert_eq!(
        limited.report.skipped[0].reason,
        "system font byte budget exhausted"
    );
}

#[test]
fn missing_primary_still_admits_the_next_installed_fallback() {
    let scratch = tempfile::tempdir().unwrap();
    let missing = scratch.path().join("missing-primary.ttc");
    let fallback = &system().report.loaded[3];
    let loaded = load(
        &[
            Candidate {
                name: "missing primary",
                path: &missing,
                index: 0,
                required: "答え",
                role: Role::Fallback,
            },
            Candidate {
                name: "remaining fallback",
                path: Path::new(&fallback.path),
                index: fallback.face_index,
                required: "答え한글",
                role: Role::Fallback,
            },
        ],
        MAX_TOTAL_BYTES,
    );
    assert_eq!(loaded.report.skipped.len(), 1);
    assert_eq!(loaded.report.loaded.len(), 1);
    assert_eq!(loaded.report.loaded[0].sha256, fallback.sha256);
    let context = egui::Context::default();
    install_loaded(&context, &loaded);
    frame(&context);
    context.fonts_mut(|fonts| {
        assert!(fonts.has_glyphs(&egui::FontId::proportional(13.0), "答え한글"));
        assert!(fonts.has_glyphs(&egui::FontId::monospace(13.0), "答え한글"));
    });
}
