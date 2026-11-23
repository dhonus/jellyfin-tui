use crate::client::Album;
use regex::Regex;
use std::{cmp::Ordering, sync::LazyLock};

static ARTICLE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(the |an |a )").unwrap());

pub(crate) fn compare_album_artist(a: &Album, b: &Album) -> Ordering {
    match (a.album_artists.first(), b.album_artists.first()) {
        (Some(a), Some(b)) => compare(&a.name.to_ascii_lowercase(), &b.name.to_ascii_lowercase()),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
    .then_with(|| compare(&a.name.to_ascii_lowercase(), &b.name.to_ascii_lowercase()))
}

pub(crate) fn compare(a: &str, b: &str) -> Ordering {
    fn strip_article(s: &str) -> String {
        let s = s.trim_start();
        let stripped = ARTICLE_RE.replace(s, "");
        stripped.trim_start().to_owned()
    }

    let a = strip_article(a);
    let b = strip_article(b);

    a.cmp(&b)
}

pub(crate) fn strip_article(s: &str) -> String {
    let s = s.trim_start();
    let stripped = ARTICLE_RE.replace(s, "");
    stripped.trim_start().to_owned()
}
