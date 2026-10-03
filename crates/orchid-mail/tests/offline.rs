//! Offline tests for mail autodiscover and MIME helpers.

use orchid_mail::account::{AuthKind, ComposeMessage, TlsMode};
use orchid_mail::autodiscover::ispdb::parse_ispdb_xml;
use orchid_mail::autodiscover::profiles::builtin_profile;
use orchid_mail::mime_util::{build_rfc822, header_fields, html_to_text, parse_rfc822};
use uuid::Uuid;

#[test]
fn gmail_and_yandex_profiles() {
    let g = builtin_profile("a@gmail.com", "gmail.com").expect("gmail");
    assert_eq!(g.imap.host, "imap.gmail.com");
    assert_eq!(g.auth, AuthKind::Oauth2);
    let y = builtin_profile("a@yandex.ru", "yandex.ru").expect("yandex");
    assert_eq!(y.smtp.port, 465);
    assert_eq!(y.auth, AuthKind::Password);
}

#[test]
fn ispdb_sample() {
    let xml = r#"
    <clientConfig version="1.1">
      <emailProvider id="example.com">
        <incomingServer type="imap">
          <hostname>imap.example.com</hostname>
          <port>993</port>
          <socketType>SSL</socketType>
          <username>%EMAILADDRESS%</username>
        </incomingServer>
        <outgoingServer type="smtp">
          <hostname>smtp.example.com</hostname>
          <port>587</port>
          <socketType>STARTTLS</socketType>
          <username>%EMAILADDRESS%</username>
        </outgoingServer>
      </emailProvider>
    </clientConfig>
    "#;
    let s = parse_ispdb_xml(xml).expect("parse");
    assert_eq!(s.imap.host, "imap.example.com");
    assert_eq!(s.imap.tls, TlsMode::Implicit);
    assert_eq!(s.smtp.tls, TlsMode::StartTls);
}

#[test]
fn mime_roundtrip_and_html_strip() {
    let compose = ComposeMessage {
        to: "to@example.com".into(),
        subject: "Subject line".into(),
        body: "Body text".into(),
        ..Default::default()
    };
    let raw = build_rfc822("Sender", "from@example.com", &compose).expect("build");
    let body = parse_rfc822(Uuid::nil(), "INBOX", 7, &raw);
    assert!(body.text.contains("Body text"));
    let (from, _to, subject, _date, _unix, _mid, _att) = header_fields(&raw);
    assert!(from.contains("Sender") || from.contains("from@"));
    assert_eq!(subject, "Subject line");
    assert_eq!(html_to_text("<p>Hi <b>there</b></p>"), "Hi there");
}
