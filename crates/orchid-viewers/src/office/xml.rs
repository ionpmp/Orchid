// Workbook zip parts and small XML helpers.

fn sheet_entries<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Vec<(String, String)> {
    let book = read_entry(archive, "xl/workbook.xml").unwrap_or_default();
    let rels = read_entry(archive, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let mut out = Vec::new();
    for (name, id) in sheet_refs(&book) {
        let target = rel_target(&rels, &id);
        let path = if target.is_empty() {
            String::new()
        } else {
            normalize_part(&target)
        };
        if !path.is_empty() {
            out.push((name, path));
        }
    }
    if out.is_empty() {
        for name in zip_names(archive) {
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("xl/worksheets/sheet") && lower.ends_with(".xml") {
                out.push((name.clone(), name));
            }
        }
        out.sort_by(|left, right| left.1.cmp(&right.1));
    }
    out
}

fn sheet_refs(xml: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == "sheet" {
                    let name = attr(&event, "name");
                    let id = attr(&event, "r:id");
                    let id = if id.is_empty() {
                        attr_local(&event, "id")
                    } else {
                        id
                    };
                    if !name.is_empty() && !id.is_empty() {
                        out.push((name, id));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn rel_target(xml: &str, id: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == "Relationship" && attr(&event, "Id") == id {
                    return attr(&event, "Target");
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    String::new()
}

fn normalize_part(target: &str) -> String {
    let target = target.trim_start_matches('/');
    if target.starts_with("xl/") {
        target.to_string()
    } else {
        format!("xl/{target}")
    }
}


fn shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    let mut in_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "si" {
                    in_si = true;
                    current.clear();
                } else if in_si && name == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Text(text)) if in_t => {
                current.push_str(&xml_text(text.as_ref()));
            }
            Ok(Event::GeneralRef(entity)) if in_t => {
                current.push_str(entity_text(entity.as_ref()));
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "t" {
                    in_t = false;
                } else if name == "si" {
                    strings.push(std::mem::take(&mut current));
                    in_si = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    strings
}


fn zip_names<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Vec<String> {
    (0..archive.len())
        .filter_map(|index| {
            archive
                .by_index(index)
                .ok()
                .map(|file| file.name().to_string())
        })
        .collect()
}

fn read_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).ok()?;
    Some(xml)
}

fn attr(event: &quick_xml::events::BytesStart<'_>, key: &str) -> String {
    event
        .try_get_attribute(key)
        .ok()
        .flatten()
        .map(|attribute| attribute.value.into_owned())
        .unwrap_or_default()
}

fn attr_local(event: &quick_xml::events::BytesStart<'_>, key: &str) -> String {
    for attribute in event.attributes().flatten() {
        if local_name(attribute.key.as_ref()) == key {
            return attribute.value.into_owned();
        }
    }
    String::new()
}

fn local_name(name: &str) -> String {
    name.rsplit([':', '}']).next().unwrap_or(name).to_string()
}

fn entity_text(raw: &str) -> &'static str {
    match raw {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
}

fn xml_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = after;
            continue;
        };
        let decoded = match &after[..end] {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            _ => "",
        };
        if decoded.is_empty() {
            out.push('&');
            rest = after;
        } else {
            out.push_str(decoded);
            rest = &after[end + 1..];
        }
    }
    out.push_str(rest);
    out
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn is_slide_path(path: &orchid_fs::FsPath) -> bool {
    matches!(
        path.extension()
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("pptx") | Some("pptm") | Some("ppsx")
    )
}
