use super::*;

const MARKER: &str = "<key>DeadpanPackaging</key><string>xtask-bundle-1</string>";

fn plist(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict>{body}</dict></plist>"
    )
}

#[test]
fn actual_top_dictionary_marker_survives_other_metadata_and_ordinary_xml_text() {
    let body = format!(
        "<key>CFBundleExecutable</key><string>deadpan-app</string><!-- ignored -->{MARKER}<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLSchemes</key><array><string>deadpan</string></array></dict></array><key>Note</key><string>A &amp; B</string><key>Flag</key><true/>"
    );
    assert!(declared(&plist(&body)));
    let standard_dtd = plist(MARKER).replace("<plist", "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist");
    assert!(declared(&standard_dtd));
    assert!(declared(&plist(
        "<key>Deadpan&#80;ackaging</key><string>xtask-bundle-&#49;</string>"
    )));
    assert!(declared(&plist(
        "<key>DeadpanPack<![CDATA[aging]]></key><string>xtask-bundle-1</string>"
    )));
}

#[test]
fn removed_marker_cannot_be_recovered_from_comments_cdata_or_the_wrong_dictionary() {
    for body in [
        format!("<!-- {MARKER} -->"),
        format!("<key>Other</key><string><![CDATA[{MARKER}]]></string>"),
        format!("<key>Wrapped</key><dict>{MARKER}</dict>"),
        format!("<key>Wrapped</key><array><dict>{MARKER}</dict></array>"),
        format!("{MARKER}<key>Wrapped</key><dict>{MARKER}</dict>"),
    ] {
        assert!(!declared(&plist(&body)), "{body}");
    }
    assert!(!declared(MARKER));
    assert!(!declared(&format!("<dict>{MARKER}</dict>")));
    assert!(!declared(&format!(
        "<plist><array><dict>{MARKER}</dict></array></plist>"
    )));
}

#[test]
fn duplicate_wrong_type_nested_and_incomplete_marker_values_refuse() {
    for body in [
        format!("{MARKER}{MARKER}"),
        format!("{MARKER}<key>DeadpanPackaging</key><string>other</string>"),
        "<key>DeadpanPackaging</key><integer>1</integer>".into(),
        "<key>DeadpanPackaging</key><true/>".into(),
        "<key>DeadpanPackaging</key><array><string>xtask-bundle-1</string></array>".into(),
        "<key>DeadpanPackaging</key><string>other</string>".into(),
        "<key>DeadpanPackaging</key><string> xtask-bundle-1</string>".into(),
        "<key>DeadpanPackaging</key><string><string>xtask-bundle-1</string></string>".into(),
        "<key>Deadpan<key>Packaging</key></key><string>xtask-bundle-1</string>".into(),
        "<key>DeadpanPackaging</key>".into(),
        format!("{MARKER}<key>Dangling</key>"),
        format!("{MARKER}<string>unkeyed</string>"),
    ] {
        assert!(!declared(&plist(&body)), "{body}");
    }
}

#[test]
fn malformed_xml_namespaces_external_entities_and_trailing_roots_refuse() {
    for source in [
        plist(MARKER).replace("</string>", "</key>"),
        plist(MARKER).replace("</plist>", ""),
        format!("{}<plist><dict/></plist>", plist(MARKER)),
        plist(MARKER).replace("<plist version=\"1.0\">", "<plist xmlns=\"foreign\">"),
        plist(MARKER).replace(
            "<plist version=\"1.0\">",
            "<plist version=\"1.0\" version=\"1.0\">",
        ),
        plist(MARKER).replace("<key>DeadpanPackaging", "<foreign:key>DeadpanPackaging"),
        plist(MARKER).replace("</dict>", "<!-- malformed -- comment --></dict>"),
        plist(MARKER).replace("xtask-bundle-1", "&remote;"),
        plist(MARKER).replace(
            "<plist",
            "<!DOCTYPE plist [<!ENTITY remote SYSTEM \"file:///must-not-open\">]><plist",
        ),
        plist(MARKER).replace("</dict>", "\u{1}</dict>"),
        format!("<!-- before declaration -->{}", plist(MARKER)),
        format!("<![CDATA[ ]]> {}", plist(MARKER)),
        format!("{}<?xml version=\"1.0\"?>", plist(MARKER)),
    ] {
        assert!(!declared(&source), "{source}");
    }
}

#[test]
fn parsing_is_bounded_by_the_payload_and_element_depth() {
    assert!(!declared(
        &" ".repeat(usize::try_from(INFO_LIMIT).unwrap() + 1)
    ));
    for (arrays, expected) in [(MAX_DEPTH - 2, true), (MAX_DEPTH - 1, false)] {
        let source = plist(&format!(
            "{MARKER}<key>Other</key>{}{}",
            "<array>".repeat(arrays),
            "</array>".repeat(arrays)
        ));
        assert_eq!(declared(&source), expected, "depth {}", arrays + 2);
    }
}
