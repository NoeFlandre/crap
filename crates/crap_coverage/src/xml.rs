//! Cobertura XML reports (`coverage xml`).
//!
//! File names in a report are relative to one of the `<source>` directories.

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::{FileCoverage, LineSet};

pub(crate) struct XmlReport {
    pub(crate) files: BTreeMap<String, FileCoverage>,
    pub(crate) sources: Vec<String>,
}

pub(crate) fn read(contents: &[u8]) -> Result<XmlReport, String> {
    let mut reader = Reader::from_reader(contents);
    let mut buffer = Vec::new();
    let mut files: BTreeMap<String, FileCoverage> = BTreeMap::new();
    let mut sources = Vec::new();
    let mut current: Option<String> = None;
    let mut in_source = false;
    let mut seen_root = false;

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|err| format!("at byte {}: {err}", reader.error_position()))?;
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let name = element.local_name();
                match name.as_ref() {
                    "coverage" => seen_root = true,
                    "source" => in_source = true,
                    "class" => {
                        let filename = attribute(&element, "filename")?
                            .ok_or("`<class>` element without a `filename` attribute")?;
                        files
                            .entry(filename.clone())
                            .or_insert_with(|| FileCoverage {
                                executed: LineSet::new(),
                                statements: Some(LineSet::new()),
                            });
                        current = Some(filename);
                    }
                    "line" => {
                        // `<line>` also appears inside `<method>`; those duplicate class lines.
                        if let Some(filename) = &current {
                            let number = attribute(&element, "number")?
                                .and_then(|number| number.parse::<u32>().ok())
                                .ok_or("`<line>` element without a valid `number`")?;
                            let hits = attribute(&element, "hits")?
                                .and_then(|hits| hits.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            let file = files.entry(filename.clone()).or_default();
                            file.statements.get_or_insert_default().insert(number);
                            if hits > 0.0 {
                                file.executed.insert(number);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(text) if in_source => {
                let text = text.xml10_content();
                let text = text.trim();
                if !text.is_empty() {
                    sources.push(text.to_string());
                }
            }
            Event::End(element) => match element.local_name().as_ref() {
                "source" => in_source = false,
                "class" => current = None,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    if !seen_root {
        return Err("missing `<coverage>` root element".to_string());
    }
    Ok(XmlReport { files, sources })
}

fn attribute(element: &BytesStart<'_>, name: &str) -> Result<Option<String>, String> {
    let Some(attribute) = element
        .try_get_attribute(name)
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let value = attribute
        .normalized_value(XmlVersion::Implicit1_0)
        .map_err(|err| err.to_string())?;
    Ok(Some(value.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report() {
        let contents = br#"<?xml version="1.0" ?>
<coverage version="7.10.0" line-rate="0.75">
    <sources>
        <source>/home/user/project/src</source>
    </sources>
    <packages>
        <package name="pkg">
            <classes>
                <class name="mod.py" filename="pkg/mod.py" line-rate="0.75">
                    <methods/>
                    <lines>
                        <line number="1" hits="1"/>
                        <line number="2" hits="0"/>
                        <line number="4" hits="3" branch="true" condition-coverage="50% (1/2)"/>
                    </lines>
                </class>
            </classes>
        </package>
    </packages>
</coverage>"#;
        let report = read(contents).unwrap();
        assert_eq!(report.sources, ["/home/user/project/src"]);
        let file = &report.files["pkg/mod.py"];
        assert_eq!(file.executed, LineSet::from([1, 4]));
        assert_eq!(file.statements, Some(LineSet::from([1, 2, 4])));
    }

    #[test]
    fn not_cobertura() {
        assert!(read(b"<html></html>").is_err());
    }
}
