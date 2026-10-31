use crate::client::{
    de_musicbrainz_album_id, sanitize_device_name, ser_musicbrainz_album_id, Client,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Mbid {
    #[serde(
        rename = "ProviderIds",
        default,
        deserialize_with = "de_musicbrainz_album_id",
        serialize_with = "ser_musicbrainz_album_id"
    )]
    id: Option<String>,
}

fn parse(json: &str) -> Option<String> {
    serde_json::from_str::<Mbid>(json).unwrap().id
}

#[test]
fn musicbrainz_album_id_round_trips() {
    let original = Mbid { id: Some("7a85cc75".into()) };
    let json = serde_json::to_string(&original).unwrap();
    assert_eq!(json, r#"{"ProviderIds":{"MusicBrainzAlbum":"7a85cc75"}}"#);
    assert_eq!(parse(&json).as_deref(), Some("7a85cc75"));
    assert_eq!(parse(&serde_json::to_string(&Mbid { id: None }).unwrap()), None);
}

#[test]
fn musicbrainz_album_id_reads_legacy_rows() {
    assert_eq!(parse(r#"{"ProviderIds":"7a85cc75"}"#).as_deref(), Some("7a85cc75"));
    assert_eq!(parse(r#"{"ProviderIds":null}"#), None);
    assert_eq!(parse(r#"{"ProviderIds":{"MusicBrainzAlbum":""}}"#), None);
    assert_eq!(parse(r#"{}"#), None);
}

#[test]
fn device_name_keeps_ordinary_hostnames() {
    assert_eq!(sanitize_device_name("think-pad_2.local"), "think-pad_2.local");
}

#[test]
fn device_name_cannot_break_out_of_the_quoted_header() {
    assert_eq!(sanitize_device_name("a\"b, Token=\\x"), "abTokenx");
    assert_eq!(sanitize_device_name("poč\u{ed}tač"), "pota");
}

#[test]
fn device_name_falls_back_when_nothing_usable_is_left() {
    assert_eq!(sanitize_device_name(""), "jellyfin-tui");
    assert_eq!(sanitize_device_name("\"\""), "jellyfin-tui");
}

#[test]
fn authorization_header_names_the_app_and_the_machine_separately() {
    let (_, header) = Client::generate_authorization_header(&"id".to_string(), "token");
    assert!(header.contains("Client=\"jellyfin-tui\""), "{header}");
    assert!(header.contains(&format!("Device=\"{}\"", crate::client::device_name())), "{header}");
}
