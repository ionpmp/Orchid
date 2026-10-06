//! One CardDAV collection: pull vCards and write them back.
//!
//! Basic authentication only. A card keeps `FN`, two `EMAIL` values, two
//! `TEL` values, `NOTE`, and `UID`. Other properties are ignored. At most
//! 500 cards are kept.

use std::time::Duration;

use super::config::Contact;

const MAX_CARDS: usize = 500;

/// Cards read from one address book REPORT.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pull {
    /// Cards in the REPORT, at most 500.
    pub cards: Vec<Contact>,
    /// True when the REPORT had more cards than this build keeps.
    pub truncated: bool,
}

/// Require one `http://` or `https://` collection URL with a trailing slash.
pub fn normalize_collection(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
    if raw.split_whitespace().count() != 1 {
        return Err("one-url");
    }
    if !(raw.starts_with("https://") || raw.starts_with("http://")) {
        return Err("bad-url");
    }
    let mut url = url::Url::parse(raw).map_err(|_| "bad-url")?;
    if url.host_str().is_none() {
        return Err("bad-url");
    }
    let path = url.path();
    if !path.ends_with('/') {
        let joined = format!("{path}/");
        url.set_path(&joined);
    }
    Ok(url.to_string())
}

/// Download every vCard in the collection.
pub async fn pull(collection: &str, user: &str, password: &str) -> Result<Pull, String> {
    let collection = normalize_collection(collection).map_err(str::to_string)?;
    let body = addressbook_query();
    let response = request(
        "REPORT",
        &collection,
        user,
        password,
        Some("application/xml; charset=utf-8"),
        &[("Depth", "1")],
        Some(body.into_bytes()),
    )
    .await?;
    if response.status == 401 || response.status == 403 {
        return Err("auth".into());
    }
    if !(200..300).contains(&response.status) && response.status != 207 {
        return Err(format!("failed:HTTP {}", response.status));
    }
    Ok(pull_from_xml(&response.body, &collection))
}

/// Write one card. Returns the stored href and etag.
pub async fn put_card(
    collection: &str,
    user: &str,
    password: &str,
    card: &Contact,
) -> Result<(String, String), String> {
    let collection = normalize_collection(collection).map_err(str::to_string)?;
    let href = if card.href.trim().is_empty() {
        card_href(&collection, &card.id)?
    } else {
        card.href.clone()
    };
    let mut headers = Vec::new();
    if !card.etag.is_empty() {
        headers.push(("If-Match", card.etag.as_str()));
    }
    let response = request(
        "PUT",
        &href,
        user,
        password,
        Some("text/vcard; charset=utf-8"),
        &headers,
        Some(render_vcard(card).into_bytes()),
    )
    .await?;
    if response.status == 412 {
        return Err("conflict".into());
    }
    if response.status == 401 || response.status == 403 {
        return Err("auth".into());
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("failed:HTTP {}", response.status));
    }
    let etag = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("etag"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    Ok((href, etag))
}

/// Remove one remote card.
pub async fn delete_card(
    user: &str,
    password: &str,
    href: &str,
    etag: Option<&str>,
) -> Result<(), String> {
    let mut headers = Vec::new();
    if let Some(etag) = etag.filter(|value| !value.is_empty()) {
        headers.push(("If-Match", etag));
    }
    let response = request("DELETE", href, user, password, None, &headers, None).await?;
    if response.status == 404 {
        return Ok(());
    }
    if response.status == 412 {
        return Err("conflict".into());
    }
    if response.status == 401 || response.status == 403 {
        return Err("auth".into());
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("failed:HTTP {}", response.status));
    }
    Ok(())
}

/// Fold a REPORT body into cards. Dates and photos are not read.
pub fn pull_from_xml(xml: &str, collection: &str) -> Pull {
    let mut cards = Vec::new();
    let mut truncated = false;
    for item in response_blocks(xml) {
        if cards.len() >= MAX_CARDS {
            truncated = true;
            break;
        }
        let Some(vcard) = tag_text(&item, "address-data") else {
            continue;
        };
        let href = tag_text(&item, "href").unwrap_or_default();
        let href = resolve_href(collection, &href);
        let etag = tag_text(&item, "getetag").unwrap_or_default();
        if let Some(mut card) = parse_vcard(&vcard) {
            card.href = href;
            card.etag = etag;
            cards.push(card);
        }
    }
    Pull { cards, truncated }
}

/// Render the stored fields as a vCard 3.0 body.
///
/// A second email or phone is written only when it is not empty.
pub fn render_vcard(card: &Contact) -> String {
    let mut lines = vec![
        "BEGIN:VCARD".to_string(),
        "VERSION:3.0".to_string(),
        format!("UID:{}", escape_text(&card.id)),
        format!("FN:{}", escape_text(&card.name)),
        format!("EMAIL:{}", escape_text(&card.email)),
    ];
    if !card.email2.trim().is_empty() {
        lines.push(format!("EMAIL:{}", escape_text(&card.email2)));
    }
    lines.push(format!("TEL:{}", escape_text(&card.phone)));
    if !card.phone2.trim().is_empty() {
        lines.push(format!("TEL:{}", escape_text(&card.phone2)));
    }
    lines.push(format!("NOTE:{}", escape_text(&card.notes)));
    lines.push("END:VCARD".to_string());
    let mut body = lines.join("\r\n");
    body.push_str("\r\n");
    body
}

/// Read `UID`, `FN`, two `EMAIL` values, two `TEL` values, and `NOTE`.
/// Further properties are dropped.
pub fn parse_vcard(raw: &str) -> Option<Contact> {
    let text = unfold(raw);
    let mut uid = String::new();
    let mut name = String::new();
    let mut email = String::new();
    let mut email2 = String::new();
    let mut phone = String::new();
    let mut phone2 = String::new();
    let mut notes = String::new();
    let mut in_card = false;
    for line in text.lines() {
        if line.eq_ignore_ascii_case("BEGIN:VCARD") {
            in_card = true;
            continue;
        }
        if line.eq_ignore_ascii_case("END:VCARD") {
            break;
        }
        if !in_card {
            continue;
        }
        let Some((prop, value)) = split_prop(line) else {
            continue;
        };
        match prop.as_str() {
            "UID" if uid.is_empty() => uid = value,
            "FN" if name.is_empty() => name = value,
            "EMAIL" if email.is_empty() => email = value,
            "EMAIL" if email2.is_empty() => email2 = value,
            "TEL" if phone.is_empty() => phone = value,
            "TEL" if phone2.is_empty() => phone2 = value,
            "NOTE" if notes.is_empty() => notes = value,
            _ => {}
        }
    }
    if uid.is_empty() && name.is_empty() && email.is_empty() {
        return None;
    }
    if uid.is_empty() {
        uid = uuid::Uuid::new_v4().to_string();
    }
    Some(Contact {
        id: uid,
        name,
        email,
        email2,
        phone,
        phone2,
        notes,
        href: String::new(),
        etag: String::new(),
    })
}

fn addressbook_query() -> String {
    r#"<?xml version="1.0" encoding="utf-8" ?>
<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <D:getetag/>
    <C:address-data/>
  </D:prop>
</C:addressbook-query>"#
        .to_string()
}

fn card_href(collection: &str, uid: &str) -> Result<String, String> {
    let mut url = url::Url::parse(collection).map_err(|_| "bad-url".to_string())?;
    let file = format!("{}.vcf", safe_uid(uid));
    url.path_segments_mut()
        .map_err(|_| "bad-url".to_string())?
        .pop_if_empty()
        .push(&file);
    Ok(url.to_string())
}

fn safe_uid(uid: &str) -> String {
    let cleaned: String = uid
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "contact".into()
    } else {
        cleaned
    }
}

fn resolve_href(collection: &str, href: &str) -> String {
    let href = href.trim();
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    url::Url::parse(collection)
        .ok()
        .and_then(|base| base.join(href).ok())
        .map(|url| url.to_string())
        .unwrap_or_else(|| href.to_string())
}

fn response_blocks(xml: &str) -> Vec<String> {
    element_bodies(xml, "response")
        .into_iter()
        .map(|body| body.to_string())
        .collect()
}

fn tag_text(block: &str, tag: &str) -> Option<String> {
    let raw = element_bodies(block, tag).into_iter().next()?;
    let text = if let Some(inner) = raw.trim().strip_prefix("<![CDATA[") {
        inner.trim_end_matches("]]>").to_string()
    } else {
        unescape_xml(raw)
    };
    Some(text.trim().to_string())
}

fn element_bodies<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let lower = xml.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut search = 0;
    while search < lower.len() {
        let Some(rel) = lower[search..].find('<') else {
            break;
        };
        let at = search + rel;
        if is_open_element(&lower[at..], tag) {
            let Some(gt) = lower[at..].find('>') else {
                break;
            };
            let body_start = at + gt + 1;
            let Some(close_at) = find_close(&lower[body_start..], tag) else {
                break;
            };
            let close_at = body_start + close_at;
            out.push(&xml[body_start..close_at]);
            search = close_at;
        } else {
            search = at + 1;
        }
    }
    out
}

fn is_open_element(lower_from_lt: &str, tag: &str) -> bool {
    if lower_from_lt.starts_with("</") {
        return false;
    }
    let rest = lower_from_lt.trim_start_matches('<');
    let name_end = rest
        .find([' ', '>', '/', '\t', '\n', '\r'])
        .unwrap_or(rest.len());
    let name = &rest[..name_end];
    name.rsplit(':').next() == Some(tag)
}

fn find_close(lower_from_body: &str, tag: &str) -> Option<usize> {
    let mut search = 0;
    while let Some(rel) = lower_from_body[search..].find("</") {
        let at = search + rel;
        if is_close_element(&lower_from_body[at..], tag) {
            return Some(at);
        }
        search = at + 2;
    }
    None
}

fn is_close_element(lower_from_lt: &str, tag: &str) -> bool {
    let rest = lower_from_lt
        .trim_start_matches('<')
        .trim_start_matches('/');
    let name_end = rest
        .find([' ', '>', '\t', '\n', '\r'])
        .unwrap_or(rest.len());
    rest[..name_end].rsplit(':').next() == Some(tag)
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn unfold(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.split(['\n', '\r']) {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            out.push_str(line.trim_start());
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

fn split_prop(line: &str) -> Option<(String, String)> {
    let (head, value) = line.split_once(':')?;
    let name = head
        .split(';')
        .next()?
        .split('.')
        .next()?
        .to_ascii_uppercase();
    Some((name, unescape_text(value)))
}

fn escape_text(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(ch),
        }
    }
    out
}

fn unescape_text(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

async fn request(
    method: &str,
    url: &str,
    user: &str,
    password: &str,
    content_type: Option<&str>,
    headers: &[(&str, &str)],
    body: Option<Vec<u8>>,
) -> Result<HttpResponse, String> {
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|err| err.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|err| err.to_string())?;
    let mut builder = client.request(method, url).basic_auth(user, Some(password));
    if let Some(content_type) = content_type {
        builder = builder.header("Content-Type", content_type);
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }
    let response = builder.send().await.map_err(|err| err.to_string())?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let body = response.text().await.map_err(|err| err.to_string())?;
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vcard_round_trip_keeps_the_four_fields() {
        let card = Contact {
            id: "uid-1".into(),
            name: "Lily & Rose".into(),
            email: "lily@example.com".into(),
            email2: String::new(),
            phone: "+1 2".into(),
            phone2: String::new(),
            notes: "line\ntwo".into(),
            href: String::new(),
            etag: String::new(),
        };
        let parsed = parse_vcard(&render_vcard(&card)).unwrap();
        assert_eq!(parsed.name, "Lily & Rose");
        assert_eq!(parsed.email, "lily@example.com");
        assert_eq!(parsed.phone, "+1 2");
        assert_eq!(parsed.notes, "line\ntwo");
        assert_eq!(parsed.id, "uid-1");
        assert!(parsed.email2.is_empty());
    }

    #[test]
    fn vcard_keeps_a_second_email_and_phone_and_drops_a_third() {
        let raw = "BEGIN:VCARD\r\nUID:u\r\nFN:Ada\r\nEMAIL:a@example.com\r\nEMAIL:b@example.com\r\nEMAIL:c@example.com\r\nTEL:1\r\nTEL:2\r\nTEL:3\r\nEND:VCARD\r\n";
        let card = parse_vcard(raw).unwrap();
        assert_eq!(card.email, "a@example.com");
        assert_eq!(card.email2, "b@example.com");
        assert_eq!(card.phone, "1");
        assert_eq!(card.phone2, "2");
        let again = parse_vcard(&render_vcard(&card)).unwrap();
        assert_eq!(again.email2, "b@example.com");
        assert_eq!(again.phone2, "2");
    }

    #[test]
    fn report_keeps_two_cards_and_drops_a_photo_only_card() {
        let xml = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
<D:response><D:href>/book/a.vcf</D:href><D:propstat><D:prop><D:getetag>"e1"</D:getetag>
<C:address-data><![CDATA[BEGIN:VCARD
UID:a
FN:Ann
EMAIL:a@example.com
TEL:1
NOTE:n
PHOTO:abc
END:VCARD]]></C:address-data></D:prop></D:propstat></D:response>
<D:response><D:href>/book/b.vcf</D:href><D:propstat><D:prop><D:getetag>"e2"</D:getetag>
<C:address-data>BEGIN:VCARD
UID:b
FN:Bob
END:VCARD</C:address-data></D:prop></D:propstat></D:response>
</D:multistatus>"#;
        let pull = pull_from_xml(xml, "https://card.example/book/");
        assert_eq!(pull.cards.len(), 2);
        assert_eq!(pull.cards[0].name, "Ann");
        assert!(pull.cards[0].href.ends_with("/book/a.vcf"));
        assert_eq!(pull.cards[0].etag, "\"e1\"");
        assert_eq!(pull.cards[1].name, "Bob");
        assert!(normalize_collection("https://card.example/book")
            .unwrap()
            .ends_with("/book/"));
        assert!(normalize_collection("https://a.example/one https://b.example/two").is_err());
    }
}
