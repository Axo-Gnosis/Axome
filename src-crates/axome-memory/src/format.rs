// Ported from Basic Memory (https://github.com/basicmachines-co/basic-memory, 7a1ff5f), AGPL-3.0-or-later.
use crate::temporal::{self, TimeRange};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_norway::{Mapping, Value};
use std::sync::LazyLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub note_type: Option<String>,
    pub permalink: Option<String>,
    pub tags: Vec<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub extra: Mapping,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub category: String,
    pub content: String,
    pub tags: Vec<String>,
    pub context: Option<String>,
    pub time: Option<TimeRange>,
    pub line: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub relation_type: String,
    pub target: String,
    pub context: Option<String>,
    pub line: usize,
}
#[derive(Debug, Clone)]
pub struct Note {
    pub frontmatter: Frontmatter,
    pub body: String,
    pub observations: Vec<Observation>,
    pub relations: Vec<Relation>,
}
static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*[-*+]\s+(.*)$").unwrap());
static OBS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[([^\[\]()]+)\]\s+(.+)").unwrap());
static CLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{1,3}:\d{2}(?::\d{2})?(?:[.,]\d{1,3})?( - .*)?$").unwrap());
pub(crate) fn item(line: &str) -> Option<&str> {
    ITEM.captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

// An invalid header is distinguishable from an absent header so editors refuse it.
pub(crate) fn header(text: &str) -> Result<Option<(Mapping, usize)>, ()> {
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return Ok(None);
    }
    let start = text.find('\n').ok_or(())? + 1;
    let mut end = start;
    for line in lines {
        if line.trim_end() == "---" {
            let value: Value = serde_norway::from_str(&text[start..end]).map_err(|_| ())?;
            return match value {
                Value::Mapping(m) => Ok(Some((m, end + line.len()))),
                _ => Err(()),
            };
        }
        end += line.len();
    }
    Err(())
}
pub fn render_frontmatter(f: &Frontmatter) -> String {
    let mut m = f.extra.clone();
    for (k, v) in [
        ("title", &f.title),
        ("type", &f.note_type),
        ("permalink", &f.permalink),
        ("created", &f.created),
        ("modified", &f.modified),
    ] {
        if let Some(v) = v {
            m.insert(Value::String(k.into()), Value::String(v.clone()));
        }
    }
    if !f.tags.is_empty() {
        m.insert(
            Value::String("tags".into()),
            Value::Sequence(f.tags.iter().cloned().map(Value::String).collect()),
        );
    }
    format!(
        "---\n{}---\n",
        serde_norway::to_string(&m).expect("YAML values are serializable")
    )
}
pub fn parse(text: &str) -> Note {
    let (frontmatter, offset) = match header(text) {
        Ok(Some((mut m, end))) => {
            let mut take = |k: &str| {
                m.remove(Value::String(k.into()))
                    .and_then(|v| v.as_str().map(str::to_owned))
            };
            let title = take("title");
            let note_type = take("type");
            let permalink = take("permalink");
            let created = take("created");
            let modified = take("modified");
            let tags = match m.remove(Value::String("tags".into())) {
                Some(Value::Sequence(v)) => v
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
                Some(Value::String(v)) => vec![v],
                _ => vec![],
            };
            (
                Frontmatter {
                    title,
                    note_type,
                    permalink,
                    tags,
                    created,
                    modified,
                    extra: m,
                },
                end,
            )
        }
        _ => (Frontmatter::default(), 0),
    };
    let mut note = Note {
        frontmatter,
        body: text[offset..].to_owned(),
        observations: vec![],
        relations: vec![],
    };
    let first_line = text[..offset].bytes().filter(|b| *b == b'\n').count();
    for (idx, line) in visible_lines(&text[offset..]) {
        let number = first_line + idx + 1;
        if let Some(body) = item(line) {
            if let Some(c) = OBS.captures(body) {
                let category = c[1].trim();
                if !matches!(category, "" | "x" | "X" | "-") && !CLOCK.is_match(category) {
                    let raw = c.get(2).unwrap().as_str().trim();
                    let (time, content) =
                        temporal::peel(raw).map_or((None, raw), |(t, s)| (Some(t), s));
                    let (content, context) = peel_context(content);
                    if !content.is_empty() {
                        note.observations.push(Observation {
                            category: category.into(),
                            content: content.into(),
                            tags: content
                                .split_whitespace()
                                .filter(|s| s.starts_with('#'))
                                .flat_map(|s| {
                                    s.split('#').filter(|s| !s.is_empty()).map(str::to_owned)
                                })
                                .collect(),
                            context: context.map(str::to_owned),
                            time,
                            line: number,
                        });
                    }
                }
            }
        }
        let masked = mask_code(line);
        let links = wikilinks(&masked);
        let explicit = item(&masked).and_then(|body| explicit_relation(body, number));
        if let Some(r) = explicit {
            note.relations.push(r);
        } else {
            note.relations
                .extend(links.into_iter().map(|target| Relation {
                    relation_type: "links_to".into(),
                    target: target.into(),
                    context: None,
                    line: number,
                }));
        }
    }
    note
}
pub(crate) fn peel_context(text: &str) -> (&str, Option<&str>) {
    if let Some(prefix) = text.strip_suffix(')') {
        if let Some((body, context)) = prefix.rsplit_once('(') {
            return (body.trim_end(), Some(context.trim()));
        }
    }
    (text, None)
}
pub(crate) fn visible_lines(text: &str) -> Vec<(usize, &str)> {
    let mut fence: Option<(char, usize)> = None;
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let s = line.trim_start();
            if let Some((c, n)) = fence {
                if s.chars().take_while(|x| *x == c).count() >= n
                    && s.trim_matches(c).trim().is_empty()
                {
                    fence = None;
                }
                return false;
            }
            let c = s.chars().next().unwrap_or(' ');
            let n = s.chars().take_while(|x| *x == c).count();
            if matches!(c, '`' | '~') && n >= 3 {
                fence = Some((c, n));
                return false;
            }
            !s.starts_with('>')
        })
        .collect()
}
fn escaped(s: &str, i: usize) -> bool {
    s[..i].bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
}
fn mask_code(s: &str) -> String {
    let mut bytes = s.as_bytes().to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' || escaped(s, i) {
            i += 1;
            continue;
        }
        let n = bytes[i..].iter().take_while(|b| **b == b'`').count();
        let mut j = i + n;
        let mut end = None;
        while j < bytes.len() {
            if bytes[j] == b'`' {
                let run = bytes[j..].iter().take_while(|b| **b == b'`').count();
                if run == n {
                    end = Some(j + n);
                    break;
                }
                j += run;
            } else {
                j += 1;
            }
        }
        if let Some(end) = end {
            bytes[i..end].fill(b' ');
            i = end;
        } else {
            i += n;
        }
    }
    String::from_utf8(bytes).expect("code spans replaced as whole UTF-8 slices")
}
fn wikilinks(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut i = 0;
    while let Some(p) = s[i..].find("[[") {
        let start = i + p;
        i = start + 2;
        if escaped(s, start) {
            continue;
        }
        let mut depth = 1;
        let mut j = i;
        while j + 1 < s.len() {
            let pair = &s.as_bytes()[j..j + 2];
            if pair == b"[[" && !escaped(s, j) {
                depth += 1;
                j += 2;
            } else if pair == b"]]" && !escaped(s, j) {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                j += 2;
            } else {
                j += 1;
            }
        }
        if depth != 0 {
            break;
        }
        let target = s[i..j].trim();
        if !target.is_empty() {
            out.push(target);
        }
        i = j + 2;
    }
    out
}
fn explicit_relation(body: &str, line: usize) -> Option<Relation> {
    let start = body.find("[[")?;
    if escaped(body, start) {
        return None;
    }
    let end = body[start + 2..].find("]]")? + start + 2;
    let label = body[..start].trim();
    let first = label.chars().next()?;
    let label = if matches!(first, '\'' | '"') && label.ends_with(first) && label.len() >= 2 {
        label[1..label.len() - 1].trim()
    } else {
        if label.chars().any(char::is_whitespace) {
            return None;
        }
        label
    };
    let target = body[start + 2..end].trim();
    if label.is_empty() || target.is_empty() {
        return None;
    }
    let tail = body[end + 2..].trim();
    let context = if tail.is_empty() {
        None
    } else {
        let mut depth = 0;
        for (i, c) in tail.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && i + 1 != tail.len() {
                        return None;
                    }
                }
                _ => {}
            }
            if depth < 0 {
                return None;
            }
        }
        if !tail.starts_with('(') || !tail.ends_with(')') || depth != 0 {
            return None;
        }
        Some(tail[1..tail.len() - 1].trim().into())
    };
    Some(Relation {
        relation_type: label.into(),
        target: target.into(),
        context,
        line,
    })
}
