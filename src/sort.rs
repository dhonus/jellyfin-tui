use regex::Regex;
use std::{cmp::Ordering, sync::LazyLock};

static ARTICLE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(the |an |a )").unwrap());

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

