// Ported from Basic Memory (https://github.com/basicmachines-co/basic-memory, 7a1ff5f), AGPL-3.0-or-later.
use unicode_normalization::UnicodeNormalization;
pub fn permalink(path_or_title: &str) -> String {
    let normalized: String = path_or_title.nfc().collect();
    let mut out = String::with_capacity(normalized.len());
    for c in normalized
        .strip_suffix(".md")
        .unwrap_or(&normalized)
        .chars()
        .flat_map(char::to_lowercase)
    {
        let c = if c.is_whitespace() || c == '_' {
            '-'
        } else {
            c
        };
        if (c.is_alphanumeric() || matches!(c, '-' | '/')) && !(c == '-' && out.ends_with('-')) {
            out.push(c);
        }
    }
    out
}
