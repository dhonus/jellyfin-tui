use crate::client::{de_musicbrainz_album_id, ser_musicbrainz_album_id};
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
