//! Extract only the real top-level packaging marker from bounded XML plists.
//! This does not deserialize application metadata or resolve external entities.

use quick_xml::{Reader, events::Event};

use super::{INFO_LIMIT, PACKAGING_KEY, PACKAGING_LAYOUT};

const MAX_DEPTH: usize = 64;

fn legal_xml_char(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

enum Element {
    Plist { dictionary: bool },
    Dict { key: Option<String> },
    Array,
    Key(String),
    String { marker: bool, text: String },
    Scalar,
}

#[derive(Default)]
struct Marker {
    stack: Vec<Element>,
    root_seen: bool,
    root_closed: bool,
    marker_seen: bool,
    marker_valid: bool,
}

impl Marker {
    fn start(&mut self, name: &[u8]) -> Option<()> {
        if self.root_closed || self.stack.len() >= MAX_DEPTH {
            return None;
        }
        let marker = match self.stack.last_mut() {
            None if name == b"plist" && !self.root_seen => {
                self.root_seen = true;
                false
            }
            Some(Element::Plist { dictionary }) if name == b"dict" && !*dictionary => {
                *dictionary = true;
                false
            }
            Some(Element::Dict { key }) if name == b"key" && key.is_none() => false,
            Some(Element::Dict { key }) if name != b"key" => key.take()? == PACKAGING_KEY,
            Some(Element::Array) if name != b"key" && name != b"plist" => false,
            _ => return None,
        };
        if marker && name != b"string" {
            return None;
        }
        self.stack.push(match name {
            b"plist" if self.stack.is_empty() => Element::Plist { dictionary: false },
            b"dict" => Element::Dict { key: None },
            b"array" => Element::Array,
            b"key" => Element::Key(String::new()),
            b"string" => Element::String {
                marker,
                text: String::new(),
            },
            b"integer" | b"real" | b"date" | b"data" | b"true" | b"false" => Element::Scalar,
            _ => return None,
        });
        Some(())
    }

    fn end(&mut self) -> Option<()> {
        match self.stack.pop()? {
            Element::Plist { dictionary: true } => self.root_closed = true,
            Element::Plist { dictionary: false } | Element::Dict { key: Some(_) } => return None,
            Element::Key(text) => {
                if text == PACKAGING_KEY {
                    if self.stack.len() != 2 || self.marker_seen {
                        return None;
                    }
                    self.marker_seen = true;
                }
                let Element::Dict { key } = self.stack.last_mut()? else {
                    return None;
                };
                *key = Some(text);
            }
            Element::String { marker: true, text } => {
                if text != PACKAGING_LAYOUT {
                    return None;
                }
                self.marker_valid = true;
            }
            _ => {}
        }
        Some(())
    }

    fn text(&mut self, text: &str) -> Option<()> {
        if !text.chars().all(legal_xml_char) {
            return None;
        }
        match self.stack.last_mut() {
            Some(Element::Key(value)) | Some(Element::String { text: value, .. }) => {
                value.push_str(text)
            }
            Some(Element::Scalar) => {}
            _ if text
                .bytes()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) => {}
            _ => return None,
        }
        Some(())
    }
}

fn parse(info: &str) -> Option<bool> {
    if info.len() as u64 > INFO_LIMIT || !info.chars().all(legal_xml_char) {
        return None;
    }
    let mut reader = Reader::from_str(info);
    reader.config_mut().check_comments = true;
    reader.config_mut().expand_empty_elements = true;
    let mut marker = Marker::default();
    let mut first_event = true;
    let mut doctype = false;
    loop {
        let event = reader.read_event().ok()?;
        let initial = first_event;
        first_event = false;
        match event {
            Event::Start(start) => {
                for attribute in start.attributes() {
                    let attribute = attribute.ok()?;
                    if start.name().as_ref() != b"plist"
                        || attribute.key.as_ref() != b"version"
                        || attribute.value.as_ref() != b"1.0"
                    {
                        return None;
                    }
                }
                marker.start(start.name().as_ref())?;
            }
            Event::End(_) => marker.end()?,
            Event::Text(text) => marker.text(&text.xml10_content().ok()?)?,
            Event::CData(text) if !marker.stack.is_empty() => {
                marker.text(&text.xml10_content().ok()?)?
            }
            Event::GeneralRef(reference) => {
                if let Some(ch) = reference.resolve_char_ref().ok()? {
                    marker.text(ch.encode_utf8(&mut [0; 4]))?;
                } else {
                    marker.text(quick_xml::escape::resolve_xml_entity(
                        &reference.decode().ok()?,
                    )?)?;
                }
            }
            Event::Decl(value) if initial => {
                if value.version().ok()?.as_ref() != b"1.0" {
                    return None;
                }
                if let Some(encoding) = value.encoding()
                    && !encoding.ok()?.as_ref().eq_ignore_ascii_case(b"utf-8")
                {
                    return None;
                }
            }
            Event::DocType(value) if !marker.root_seen && !doctype => {
                let value = value.decode().ok()?;
                if !value.starts_with("plist ") || value.contains(['[', ']']) {
                    return None;
                }
                doctype = true;
            }
            Event::Comment(_) => {}
            Event::Eof => {
                return Some(marker.root_closed && marker.stack.is_empty() && marker.marker_valid);
            }
            _ => return None,
        }
    }
}

pub(super) fn declared(info: &str) -> bool {
    parse(info).unwrap_or(false)
}

#[cfg(test)]
mod tests;
