// Ported from Basic Memory (https://github.com/basicmachines-co/basic-memory, 7a1ff5f), AGPL-3.0-or-later.
use crate::{
    format::{self, Frontmatter},
    temporal::{TimeKind, TimeRange},
    Error, Result,
};
pub fn new_note(title: &str, note_type: &str, tags: &[String], body: &str) -> String {
    format!(
        "{}\n{body}",
        format::render_frontmatter(&Frontmatter {
            title: Some(title.into()),
            note_type: Some(note_type.into()),
            tags: tags.to_vec(),
            ..Default::default()
        })
    )
}
fn writable(text: &str) -> Result<usize> {
    format::header(text)
        .map(|h| h.map_or(0, |(_, end)| end))
        .map_err(|()| Error::Invalid("Cannot edit malformed frontmatter".into()))
}
fn join(a: &str, b: &str) -> String {
    format!(
        "{a}{}{b}",
        if a.is_empty() || a.ends_with('\n') {
            ""
        } else {
            "\n"
        }
    )
}
pub(crate) fn heading(line: &str) -> Option<(usize, &str)> {
    let s = line.trim_start();
    let n = s.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&n) || !s[n..].starts_with(char::is_whitespace) {
        return None;
    }
    Some((n, s[n..].trim().trim_end_matches('#').trim_end()))
}
pub(crate) fn section_range(text: &str, name: &str) -> Result<std::ops::Range<usize>> {
    let body = writable(text)?;
    let mut offset = body;
    let mut found = None;
    let mut active = None;
    let mut count = 0;
    let visible: std::collections::HashSet<usize> = format::visible_lines(&text[body..])
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    for (i, line) in text[body..].split_inclusive('\n').enumerate() {
        if visible.contains(&i) {
            if let Some((level, title)) = heading(line) {
                if let Some((start, l)) = active {
                    if level <= l {
                        found = Some(start..offset);
                        active = None;
                    }
                }
                if title == name {
                    count += 1;
                    active = Some((offset + line.len(), level));
                }
            }
        }
        offset += line.len();
    }
    if let Some((start, _)) = active {
        found = Some(start..text.len());
    }
    if count != 1 {
        return Err(Error::Invalid(format!(
            "Section {name:?} is absent or ambiguous"
        )));
    }
    Ok(found.unwrap())
}
pub fn append_observation(text: &str, line: &str) -> String {
    if writable(text).is_err() {
        return text.into();
    }
    if let Ok(r) = section_range(text, "Observations") {
        let mut end = r.start;
        let mut offset = r.start;
        let visible: std::collections::HashSet<_> = format::visible_lines(&text[r.clone()])
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        for (i, l) in text[r.clone()].split_inclusive('\n').enumerate() {
            offset += l.len();
            if visible.contains(&i) && format::item(l).is_some() {
                end = offset;
            }
        }
        return format!(
            "{}{}{}",
            join(&text[..end], line),
            if line.ends_with('\n') { "" } else { "\n" },
            &text[end..]
        );
    }
    join(text, &format!("\n## Observations\n{line}\n"))
}
pub fn close_observation(text: &str, line_no: usize, until: &str) -> String {
    if writable(text).is_err() || crate::temporal::instant(until).is_none() {
        return text.into();
    }
    let note = format::parse(text);
    let Some(obs) = note.observations.iter().find(|o| o.line == line_no) else {
        return text.into();
    };
    let mut offset = 0;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        if i + 1 == line_no {
            if let Some(end) = line.find(']') {
                let insertion =
                    end + 1 + line[end + 1..].len() - line[end + 1..].trim_start().len();
                match &obs.time {
                    None => {
                        if line[insertion..].starts_with('@') {
                            return text.into();
                        }
                        return format!(
                            "{}@(,{until}) {}",
                            &text[..offset + insertion],
                            &text[offset + insertion..]
                        );
                    }
                    Some(TimeRange {
                        kind: TimeKind::Valid,
                        lower: Some(_),
                        upper: None,
                    }) => {
                        let rest = &line[insertion..];
                        let Some(body) = rest.strip_prefix("@[") else {
                            return text.into();
                        };
                        let Some(comma) = body.find(",)") else {
                            return text.into();
                        };
                        let lower = &body[..comma];
                        let (Some(from), Some(to)) = (
                            crate::temporal::instant(lower),
                            crate::temporal::instant(until),
                        ) else {
                            return text.into();
                        };
                        // `peel` rejects a range mixing a date with an instant, and one that ends before it starts.
                        if (lower.len() == 10) != (until.len() == 10) || from > to {
                            return text.into();
                        }
                        let start = offset + insertion;
                        return format!(
                            "{}@[{lower},{until}){}",
                            &text[..start],
                            &text[start + 2 + comma + 2..]
                        );
                    }
                    Some(_) => return text.into(),
                }
            }
        }
        offset += line.len();
    }
    text.into()
}
