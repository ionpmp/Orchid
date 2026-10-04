//! One CalDAV collection: pull one-time events, and write those events back.
//!
//! Repeating events (`RRULE`) are left on the server. A timezone name on a
//! start time is stored as the numbers written in the file. A `Z` time is
//! shown in the local offset. Multi-day events show on the first day only.

use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};

use super::config::{format_date, parse_date, CalDavLink, CalendarEvent};

const WINDOW_PAST_DAYS: i64 = 90;
const WINDOW_FUTURE_DAYS: i64 = 365;

/// One VEVENT taken from a calendar collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEvent {
    /// iCalendar UID.
    pub uid: String,
    /// Absolute URL of the event resource.
    pub href: String,
    /// Server `ETag`, including quotes when the server sent them.
    pub etag: String,
    /// Summary.
    pub title: String,
    /// Description.
    pub notes: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Date-only start.
    pub all_day: bool,
    /// Minutes from midnight when `all_day` is false.
    pub start_minutes: u16,
    /// Minutes from midnight when `all_day` is false.
    pub end_minutes: u16,
}

/// Events read from one REPORT response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pull {
    /// One-time events to store.
    pub events: Vec<RemoteEvent>,
    /// UIDs that must stay, including repeating events that were not imported.
    pub keep_uids: Vec<String>,
    /// Repeating events seen and left unchanged.
    pub skipped_repeating: usize,
}

/// Require `http://` or `https://` and a trailing slash on the collection.
pub fn normalize_collection(raw: &str) -> Result<String, &'static str> {
    let raw = raw.trim();
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

/// Download one-time events in the sync window around today.
pub async fn pull(collection: &str, user: &str, password: &str) -> Result<Pull, String> {
    let collection = normalize_collection(collection).map_err(str::to_string)?;
    let today = chrono::Local::now().date_naive();
    let start = today - ChronoDuration::days(WINDOW_PAST_DAYS);
    let end = today + ChronoDuration::days(WINDOW_FUTURE_DAYS);
    let body = calendar_query(&start, &end);
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
    if response.status != 207 && response.status != 200 {
        return Err(format!("failed:HTTP {}", response.status));
    }
    let offset = chrono::Local::now().offset().local_minus_utc();
    Ok(pull_from_xml(&response.body, &collection, offset))
}

/// Create or replace one event. Returns the stored href and etag.
pub async fn put_event(
    collection: &str,
    user: &str,
    password: &str,
    href: Option<&str>,
    etag: Option<&str>,
    event: &CalendarEvent,
    uid: &str,
) -> Result<(String, String), String> {
    let collection = normalize_collection(collection).map_err(str::to_string)?;
    let target = match href {
        Some(href) if !href.is_empty() => href.to_string(),
        _ => event_href(&collection, uid)?,
    };
    let mut headers = vec![("Content-Type", "text/calendar; charset=utf-8")];
    let etag_owned;
    if let Some(etag) = etag.filter(|etag| !etag.is_empty()) {
        etag_owned = etag.to_string();
        headers.push(("If-Match", etag_owned.as_str()));
    }
    let response = request(
        "PUT",
        &target,
        user,
        password,
        None,
        &headers,
        Some(render_ics(event, uid).into_bytes()),
    )
    .await?;
    if response.status == 412 {
        return Err("conflict".into());
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("failed:HTTP {}", response.status));
    }
    let stored_etag = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("etag"))
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    Ok((target, stored_etag))
}

/// Remove one event resource.
pub async fn delete_remote(
    user: &str,
    password: &str,
    href: &str,
    etag: Option<&str>,
) -> Result<(), String> {
    let mut headers = Vec::new();
    let etag_owned;
    if let Some(etag) = etag.filter(|etag| !etag.is_empty()) {
        etag_owned = etag.to_string();
        headers.push(("If-Match", etag_owned.as_str()));
    }
    let response = request("DELETE", href, user, password, None, &headers, None).await?;
    if response.status == 412 {
        return Err("conflict".into());
    }
    if response.status == 404 || (200..300).contains(&response.status) {
        return Ok(());
    }
    Err(format!("failed:HTTP {}", response.status))
}

/// Fold a REPORT body into events. `offset_east_seconds` converts `Z` times.
pub fn pull_from_xml(xml: &str, collection: &str, offset_east_seconds: i32) -> Pull {
    let mut pull = Pull::default();
    for item in multistatus_items(xml) {
        if !item.status_ok {
            continue;
        }
        let href = resolve_href(collection, &item.href);
        for parsed in parse_calendar(&item.calendar_data, &href, &item.etag, offset_east_seconds) {
            if parsed.repeating {
                pull.skipped_repeating += 1;
                if !parsed.uid.is_empty() {
                    pull.keep_uids.push(parsed.uid);
                }
                continue;
            }
            if let Some(event) = parsed.event {
                pull.keep_uids.push(event.uid.clone());
                pull.events.push(event);
            }
        }
    }
    pull
}

/// Apply a pull. Local events with no link stay. Linked events inside the
/// window that the server no longer returned are removed.
pub fn merge(
    events: &mut Vec<CalendarEvent>,
    links: &mut Vec<CalDavLink>,
    pull: &Pull,
    window_start: NaiveDate,
    window_end: NaiveDate,
) {
    for remote in &pull.events {
        let link_index = links.iter().position(|link| {
            (!remote.uid.is_empty() && link.uid == remote.uid)
                || (!remote.href.is_empty() && link.href == remote.href)
        });
        if let Some(index) = link_index {
            let event_id = links[index].event_id.clone();
            links[index].href = remote.href.clone();
            links[index].etag = remote.etag.clone();
            links[index].uid = remote.uid.clone();
            if let Some(event) = events.iter_mut().find(|event| event.id == event_id) {
                event.title = remote.title.clone();
                event.notes = remote.notes.clone();
                event.date = remote.date.clone();
                event.all_day = remote.all_day;
                event.start_minutes = remote.start_minutes;
                event.end_minutes = remote.end_minutes;
            }
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            events.push(CalendarEvent {
                id: id.clone(),
                title: remote.title.clone(),
                date: remote.date.clone(),
                all_day: remote.all_day,
                start_minutes: remote.start_minutes,
                end_minutes: remote.end_minutes,
                notes: remote.notes.clone(),
                color: 0,
            });
            links.push(CalDavLink {
                event_id: id,
                href: remote.href.clone(),
                etag: remote.etag.clone(),
                uid: remote.uid.clone(),
            });
        }
    }
    let remote_uids: Vec<&str> = pull
        .events
        .iter()
        .map(|event| event.uid.as_str())
        .chain(pull.keep_uids.iter().map(String::as_str))
        .collect();
    let mut drop_ids = Vec::new();
    links.retain(|link| {
        let Some(event) = events.iter().find(|event| event.id == link.event_id) else {
            return false;
        };
        let in_window =
            parse_date(&event.date).is_some_and(|date| date >= window_start && date < window_end);
        let seen = remote_uids.iter().any(|uid| *uid == link.uid);
        if in_window && !seen && !link.uid.is_empty() {
            drop_ids.push(link.event_id.clone());
            return false;
        }
        true
    });
    events.retain(|event| !drop_ids.iter().any(|id| id == &event.id));
}

/// Serialize one event as a small iCalendar object.
pub fn render_ics(event: &CalendarEvent, uid: &str) -> String {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//Orchid//Calendar//EN".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{uid}"),
        format!("DTSTAMP:{stamp}"),
    ];
    if event.all_day {
        let start = event.date.replace('-', "");
        let end = parse_date(&event.date)
            .and_then(|date| date.succ_opt())
            .map(|date| format!("{:04}{:02}{:02}", date.year(), date.month(), date.day()))
            .unwrap_or(start.clone());
        lines.push(format!("DTSTART;VALUE=DATE:{start}"));
        lines.push(format!("DTEND;VALUE=DATE:{end}"));
    } else {
        let day = event.date.replace('-', "");
        lines.push(format!("DTSTART:{day}T{}", hhmmss(event.start_minutes)));
        lines.push(format!("DTEND:{day}T{}", hhmmss(event.end_minutes)));
    }
    lines.push(format!("SUMMARY:{}", escape_text(&event.title)));
    if !event.notes.is_empty() {
        lines.push(format!("DESCRIPTION:{}", escape_text(&event.notes)));
    }
    lines.push("END:VEVENT".to_string());
    lines.push("END:VCALENDAR".to_string());
    lines.join("\r\n") + "\r\n"
}

fn hhmmss(minutes: u16) -> String {
    format!("{:02}{:02}00", minutes / 60, minutes % 60)
}

fn escape_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
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

fn calendar_query(start: &NaiveDate, end: &NaiveDate) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag/>
    <C:calendar-data/>
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VEVENT">
        <C:time-range start="{start}" end="{end}"/>
      </C:comp-filter>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>"#,
        start = format!("{}T000000Z", start.format("%Y%m%d")),
        end = format!("{}T000000Z", end.format("%Y%m%d")),
    )
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

fn event_href(collection: &str, uid: &str) -> Result<String, String> {
    let mut url = url::Url::parse(collection).map_err(|_| "bad-url".to_string())?;
    let file = format!("{}.ics", safe_uid(uid));
    url.path_segments_mut()
        .map_err(|_| "bad-url".to_string())?
        .pop_if_empty()
        .push(&file);
    Ok(url.to_string())
}

fn safe_uid(uid: &str) -> String {
    let mut out = String::new();
    for ch in uid.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "event".into()
    } else {
        out
    }
}

struct StatusItem {
    href: String,
    etag: String,
    calendar_data: String,
    status_ok: bool,
}

fn multistatus_items(xml: &str) -> Vec<StatusItem> {
    split_elements(xml, "response")
        .into_iter()
        .filter_map(|block| {
            let href = element_text(&block, "href")?;
            let status = element_text(&block, "status").unwrap_or_default();
            let status_ok =
                status.is_empty() || status.contains(" 200 ") || status.contains(" 207 ");
            Some(StatusItem {
                href,
                etag: element_text(&block, "getetag").unwrap_or_default(),
                calendar_data: element_text(&block, "calendar-data").unwrap_or_default(),
                status_ok,
            })
        })
        .collect()
}

fn split_elements(xml: &str, local: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = xml.to_ascii_lowercase();
    let mut from = 0;
    while let Some(start) = find_open_tag(&lower, from, local) {
        let Some(open_end) = xml[start..].find('>') else {
            break;
        };
        let content_at = start + open_end + 1;
        if xml[start..content_at].ends_with("/>") {
            from = content_at;
            continue;
        }
        let Some(close_at) = find_close_tag(&lower, content_at, local) else {
            break;
        };
        out.push(xml[content_at..close_at].to_string());
        from = close_at + 2;
    }
    out
}

fn find_open_tag(lower: &str, from: usize, local: &str) -> Option<usize> {
    let mut search = from;
    while search < lower.len() {
        let rel = lower[search..].find('<')?;
        let at = search + rel;
        if tag_name_is(&lower[at + 1..], local) {
            return Some(at);
        }
        search = at + 1;
    }
    None
}

fn find_close_tag(lower: &str, from: usize, local: &str) -> Option<usize> {
    let mut search = from;
    while search < lower.len() {
        let rel = lower[search..].find("</")?;
        let at = search + rel;
        if tag_name_is(&lower[at + 2..], local) {
            return Some(at);
        }
        search = at + 2;
    }
    None
}

fn tag_name_is(after_bracket: &str, local: &str) -> bool {
    let name = match after_bracket.find(':') {
        Some(colon)
            if after_bracket[..colon]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
        {
            &after_bracket[colon + 1..]
        }
        _ => after_bracket,
    };
    let Some(rest) = name.strip_prefix(local) else {
        return false;
    };
    rest.is_empty()
        || rest.starts_with('>')
        || rest.starts_with(' ')
        || rest.starts_with('\t')
        || rest.starts_with('/')
}

fn element_text(block: &str, local: &str) -> Option<String> {
    let lower = block.to_ascii_lowercase();
    let patterns = [format!("<{local}"), format!(":{local}")];
    for pattern in patterns {
        let Some(rel) = lower.find(&pattern) else {
            continue;
        };
        let start = if pattern.starts_with(':') {
            block[..rel].rfind('<')?
        } else {
            rel
        };
        let open_end = start + block[start..].find('>')?;
        let content_at = open_end + 1;
        if block[start..=open_end].ends_with("/>") {
            return Some(String::new());
        }
        if block[content_at..].starts_with("<![CDATA[") {
            let data_at = content_at + "<![CDATA[".len();
            let end = block[data_at..].find("]]>")? + data_at;
            return Some(block[data_at..end].to_string());
        }
        let rest = &block[content_at..];
        let end = rest.find('<')?;
        return Some(xml_text(&rest[..end]));
    }
    None
}

fn xml_text(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = after;
            break;
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
    out.trim().to_string()
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

struct ParsedIcs {
    repeating: bool,
    uid: String,
    event: Option<RemoteEvent>,
}

fn parse_ics(ics: &str, href: &str, etag: &str, offset_east_seconds: i32) -> Option<ParsedIcs> {
    parse_calendar(ics, href, etag, offset_east_seconds)
        .into_iter()
        .next()
}

fn parse_calendar(ics: &str, href: &str, etag: &str, offset_east_seconds: i32) -> Vec<ParsedIcs> {
    if ics.trim().is_empty() {
        return Vec::new();
    }
    let text = unfold(ics);
    let mut out = Vec::new();
    let mut in_event = false;
    let mut uid = String::new();
    let mut title = String::new();
    let mut notes = String::new();
    let mut start: Option<When> = None;
    let mut end: Option<When> = None;
    let mut repeating = false;
    let mut cancelled = false;
    for line in text.lines() {
        let line = line.trim();
        if line.eq_ignore_ascii_case("BEGIN:VEVENT") {
            in_event = true;
            uid.clear();
            title.clear();
            notes.clear();
            start = None;
            end = None;
            repeating = false;
            cancelled = false;
            continue;
        }
        if line.eq_ignore_ascii_case("END:VEVENT") {
            if let Some(parsed) = finish_event(
                href, etag, &uid, &title, &notes, start, end, repeating, cancelled,
            ) {
                out.push(parsed);
            }
            in_event = false;
            continue;
        }
        if !in_event {
            continue;
        }
        let Some((name, params, value)) = split_prop(line) else {
            continue;
        };
        match name.as_str() {
            "UID" => uid = value,
            "SUMMARY" => title = value,
            "DESCRIPTION" => notes = value,
            "RRULE" => repeating = true,
            "STATUS" if value.eq_ignore_ascii_case("CANCELLED") => cancelled = true,
            "DTSTART" => start = parse_when(&params, &value, offset_east_seconds),
            "DTEND" => end = parse_when(&params, &value, offset_east_seconds),
            _ => {}
        }
    }
    out
}

fn finish_event(
    href: &str,
    etag: &str,
    uid: &str,
    title: &str,
    notes: &str,
    start: Option<When>,
    end: Option<When>,
    repeating: bool,
    cancelled: bool,
) -> Option<ParsedIcs> {
    if uid.is_empty() && title.is_empty() {
        return None;
    }
    if repeating {
        return Some(ParsedIcs {
            repeating: true,
            uid: uid.to_string(),
            event: None,
        });
    }
    if cancelled {
        return Some(ParsedIcs {
            repeating: false,
            uid: uid.to_string(),
            event: None,
        });
    }
    let start = start?;
    let (date, all_day, start_minutes, end_minutes) = match start {
        When::Date(date) => (format_date(date), true, 0, 0),
        When::DateTime {
            date,
            minutes: start_minutes,
        } => {
            let end_minutes = match end {
                Some(When::DateTime {
                    date: end_date,
                    minutes,
                }) if end_date == date => minutes.max(start_minutes),
                _ => start_minutes.saturating_add(60).min(23 * 60 + 59),
            };
            (format_date(date), false, start_minutes, end_minutes)
        }
    };
    Some(ParsedIcs {
        repeating: false,
        uid: uid.to_string(),
        event: Some(RemoteEvent {
            uid: uid.to_string(),
            href: href.to_string(),
            etag: etag.to_string(),
            title: title.to_string(),
            notes: notes.to_string(),
            date,
            all_day,
            start_minutes,
            end_minutes,
        }),
    })
}

fn unfold(input: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\r' {
            index += 1;
            continue;
        }
        if ch == '\n' {
            if matches!(chars.get(index + 1), Some(' ' | '\t')) {
                index += 2;
                continue;
            }
            out.push('\n');
            index += 1;
            continue;
        }
        out.push(ch);
        index += 1;
    }
    out
}

fn split_prop(line: &str) -> Option<(String, Vec<(String, String)>, String)> {
    let (head, value) = line.split_once(':')?;
    let mut parts = head.split(';');
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((
                key.to_ascii_uppercase(),
                value.trim_matches('"').to_string(),
            ))
        })
        .collect();
    Some((name, params, unescape_text(value)))
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

#[derive(Clone, Copy)]
enum When {
    Date(NaiveDate),
    DateTime { date: NaiveDate, minutes: u16 },
}

fn parse_when(params: &[(String, String)], value: &str, offset_east_seconds: i32) -> Option<When> {
    let value = value.trim();
    let date_only = params
        .iter()
        .any(|(key, val)| key == "VALUE" && val.eq_ignore_ascii_case("DATE"))
        || (value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()));
    if date_only {
        return Some(When::Date(ymd(&value[..8])?));
    }
    if value.len() < 15 || value.as_bytes().get(8) != Some(&b'T') {
        return None;
    }
    let date = ymd(&value[..8])?;
    let hour: u32 = value[9..11].parse().ok()?;
    let minute: u32 = value[11..13].parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    if value.ends_with('Z') {
        let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
        let utc = date.and_time(time);
        let shifted = Utc
            .from_utc_datetime(&utc)
            .checked_add_signed(ChronoDuration::seconds(i64::from(offset_east_seconds)))?;
        let local_date = shifted.date_naive();
        let minutes = u16::try_from(shifted.hour() * 60 + shifted.minute()).ok()?;
        return Some(When::DateTime {
            date: local_date,
            minutes,
        });
    }
    Some(When::DateTime {
        date,
        minutes: u16::try_from(hour * 60 + minute).ok()?,
    })
}

fn ymd(text: &str) -> Option<NaiveDate> {
    if text.len() < 8 {
        return None;
    }
    NaiveDate::from_ymd_opt(
        text[0..4].parse().ok()?,
        text[4..6].parse().ok()?,
        text[6..8].parse().ok()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> CalendarEvent {
        CalendarEvent {
            id: "local".into(),
            title: "Lily & Rose".into(),
            date: "2026-10-03".into(),
            all_day: false,
            start_minutes: 9 * 60,
            end_minutes: 10 * 60,
            notes: "line\ntwo".into(),
            color: 2,
        }
    }

    #[test]
    fn ics_round_trip_keeps_text_and_time() {
        let event = sample_event();
        let ics = render_ics(&event, "uid-1");
        let parsed = parse_ics(&ics, "https://cal.example/uid-1.ics", "\"e1\"", 0).unwrap();
        let remote = parsed.event.unwrap();
        assert_eq!(remote.title, "Lily & Rose");
        assert_eq!(remote.notes, "line\ntwo");
        assert_eq!(remote.date, "2026-10-03");
        assert_eq!(remote.start_minutes, 9 * 60);
        assert_eq!(remote.end_minutes, 10 * 60);
        assert!(!remote.all_day);
    }

    #[test]
    fn zulu_time_uses_the_given_offset_and_repeating_is_skipped() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:utc\r\nDTSTART:20261003T020000Z\r\nDTEND:20261003T030000Z\r\nSUMMARY:UTC\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:rep\r\nRRULE:FREQ=DAILY\r\nDTSTART;VALUE=DATE:20261004\r\nSUMMARY:Daily\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let xml = format!(
            r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:response><D:href>/cal/utc.ics</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:getetag>"e"</D:getetag><C:calendar-data><![CDATA[{ics}]]></C:calendar-data></D:prop></D:propstat></D:response></D:multistatus>"#
        );
        let pull = pull_from_xml(&xml, "https://cal.example/cal/", 7 * 3600);
        assert_eq!(pull.skipped_repeating, 1);
        assert_eq!(pull.events.len(), 1);
        assert_eq!(pull.events[0].date, "2026-10-03");
        assert_eq!(pull.events[0].start_minutes, 9 * 60);
        assert_eq!(pull.events[0].href, "https://cal.example/cal/utc.ics");
        assert!(pull.keep_uids.iter().any(|uid| uid == "rep"));
    }

    #[test]
    fn merge_updates_a_linked_event_and_drops_one_missing_inside_the_window() {
        let mut events = vec![
            CalendarEvent {
                id: "keep".into(),
                title: "Old".into(),
                date: "2026-10-03".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 4,
            },
            CalendarEvent {
                id: "gone".into(),
                title: "Gone".into(),
                date: "2026-10-04".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 1,
            },
            CalendarEvent {
                id: "local".into(),
                title: "Local".into(),
                date: "2026-10-05".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 2,
            },
        ];
        let mut links = vec![
            CalDavLink {
                event_id: "keep".into(),
                href: "https://cal.example/keep.ics".into(),
                etag: "\"old\"".into(),
                uid: "keep-uid".into(),
            },
            CalDavLink {
                event_id: "gone".into(),
                href: "https://cal.example/gone.ics".into(),
                etag: "\"g\"".into(),
                uid: "gone-uid".into(),
            },
        ];
        let pull = Pull {
            events: vec![RemoteEvent {
                uid: "keep-uid".into(),
                href: "https://cal.example/keep.ics".into(),
                etag: "\"new\"".into(),
                title: "New title".into(),
                notes: String::new(),
                date: "2026-10-03".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
            }],
            keep_uids: vec!["keep-uid".into()],
            skipped_repeating: 0,
        };
        let start = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        merge(&mut events, &mut links, &pull, start, end);
        assert_eq!(
            events
                .iter()
                .find(|event| event.id == "keep")
                .unwrap()
                .title,
            "New title"
        );
        assert_eq!(
            events
                .iter()
                .find(|event| event.id == "keep")
                .unwrap()
                .color,
            4
        );
        assert!(events.iter().all(|event| event.id != "gone"));
        assert!(events.iter().any(|event| event.id == "local"));
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].etag, "\"new\"");
    }

    #[tokio::test]
    async fn put_and_report_round_trip_against_a_local_server() {
        let ics = render_ics(&sample_event(), "uid-1");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let ics_for_server = ics.clone();
        std::thread::spawn(move || {
            use std::io::{ErrorKind, Read, Write};
            listener.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
            for _ in 0..2 {
                let mut sock = loop {
                    match listener.accept() {
                        Ok((sock, _)) => break sock,
                        Err(err) if err.kind() == ErrorKind::WouldBlock => {
                            if std::time::Instant::now() > deadline {
                                return;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                        Err(_) => return,
                    }
                };
                sock.set_nonblocking(false).unwrap();
                let _ = sock.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                let mut buf = Vec::new();
                let mut tmp = [0u8; 2048];
                loop {
                    let n = sock.read(&mut tmp).unwrap();
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        let header_end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                        let header = String::from_utf8_lossy(&buf[..header_end]).to_string();
                        let length = header
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        while buf.len() < header_end + length {
                            let n = sock.read(&mut tmp).unwrap();
                            if n == 0 {
                                break;
                            }
                            buf.extend_from_slice(&tmp[..n]);
                        }
                        let is_put = header.starts_with("PUT ");
                        let body = if is_put {
                            String::new()
                        } else {
                            format!(
                                r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/cal/uid-1.ics</D:href><D:propstat><D:status>HTTP/1.1 200 OK</D:status><D:prop><D:getetag>"e1"</D:getetag><D:calendar-data>{ics}</D:calendar-data></D:prop></D:propstat></D:response></D:multistatus>"#,
                                ics = xml_escape(&ics_for_server)
                            )
                        };
                        let status = if is_put {
                            "201 Created"
                        } else {
                            "207 Multi-Status"
                        };
                        let extra = if is_put { "ETag: \"e1\"\r\n" } else { "" };
                        let resp = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
                            body.len()
                        );
                        sock.write_all(resp.as_bytes()).unwrap();
                        break;
                    }
                }
            }
        });
        let collection = format!("http://{addr}/cal");
        let url = normalize_collection(&collection).unwrap();
        let response = request(
            "REPORT",
            &url,
            "ada",
            "secret",
            Some("application/xml; charset=utf-8"),
            &[("Depth", "1")],
            Some(b"<q/>".to_vec()),
        )
        .await
        .unwrap();
        let pull = pull_from_xml(&response.body, &url, 0);
        assert_eq!(
            pull.events.len(),
            1,
            "status {} body {}",
            response.status,
            response.body
        );
        assert_eq!(pull.events[0].title, "Lily & Rose");
        let (href, etag) = put_event(
            &collection,
            "ada",
            "secret",
            None,
            None,
            &sample_event(),
            "uid-1",
        )
        .await
        .unwrap();
        assert!(href.ends_with("/uid-1.ics"), "{href}");
        assert_eq!(etag, "\"e1\"");
    }

    fn xml_escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}
