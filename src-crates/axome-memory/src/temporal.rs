// Ported from Basic Memory (https://github.com/basicmachines-co/basic-memory, 7a1ff5f), AGPL-3.0-or-later.
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimeKind {
    Effective,
    #[default]
    Valid,
    Occurred,
    Due,
    Mentioned,
}
impl TimeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Effective => "effective",
            Self::Valid => "valid",
            Self::Occurred => "occurred",
            Self::Due => "due",
            Self::Mentioned => "mentioned",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "effective" => Self::Effective,
            "valid" => Self::Valid,
            "occurred" => Self::Occurred,
            "due" => Self::Due,
            "mentioned" => Self::Mentioned,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeRange {
    pub kind: TimeKind,
    pub lower: Option<String>,
    pub upper: Option<String>,
}

pub(crate) fn instant(s: &str) -> Option<DateTime<Utc>> {
    if s.len() == 10 {
        return NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()?
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc());
    }
    let upper = s.to_ascii_uppercase();
    DateTime::parse_from_rfc3339(&upper)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(&upper, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|d| d.and_utc())
        })
}
fn canonical(d: DateTime<Utc>) -> String {
    d.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}
pub fn contains(range: &TimeRange, at: &str) -> bool {
    let Some(at) = instant(at) else { return false };
    range
        .lower
        .as_deref()
        .is_none_or(|s| instant(s).is_some_and(|d| d <= at))
        && range
            .upper
            .as_deref()
            .is_none_or(|s| instant(s).is_some_and(|d| at < d))
}
fn point(s: &str) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let date = match s.len() {
        4 => NaiveDate::from_ymd_opt(s.parse().ok()?, 1, 1)?,
        7 => NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d").ok()?,
        _ => {
            let start = instant(s)?;
            let step = if s.len() == 10 {
                Duration::days(1)
            } else {
                Duration::seconds(1)
            };
            return Some((start, start.checked_add_signed(step)?));
        }
    };
    let end = if s.len() == 4 || date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)?
    };
    Some((
        date.and_hms_opt(0, 0, 0)?.and_utc(),
        end.and_hms_opt(0, 0, 0)?.and_utc(),
    ))
}

pub(crate) fn peel(text: &str) -> Option<(TimeRange, &str)> {
    let tail = text.strip_prefix('@')?;
    if let Some(open) = tail
        .find(['[', '('])
        .filter(|&i| !tail[..i].chars().any(char::is_whitespace))
    {
        let kind = if open == 0 {
            TimeKind::Valid
        } else {
            TimeKind::parse(&tail[..open])?
        };
        let end = tail[open + 1..].find([')', ']'])? + open + 1;
        if tail[end + 1..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace())
        {
            return None;
        }
        let (lo, hi) = tail[open + 1..end].split_once(',')?;
        let (lo, hi) = (lo.trim(), hi.trim());
        if !lo.is_empty() && !hi.is_empty() && (lo.len() == 10) != (hi.len() == 10) {
            return None;
        }
        let bound = |s: &str, shift: bool| -> Option<Option<String>> {
            if s.is_empty() {
                return Some(None);
            }
            let d = instant(s)?;
            let step = if s.len() == 10 {
                Duration::days(1)
            } else {
                Duration::nanoseconds(1)
            };
            Some(Some(canonical(if shift {
                d.checked_add_signed(step)?
            } else {
                d
            })))
        };
        let lower = bound(lo, tail.as_bytes()[open] == b'(')?;
        let upper = bound(hi, tail.as_bytes()[end] == b']')?;
        if let (Some(l), Some(u)) = (&lower, &upper) {
            if instant(l)? > instant(u)? {
                return None;
            }
        }
        return Some((
            TimeRange { kind, lower, upper },
            tail[end + 1..].trim_start(),
        ));
    }
    let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
    let token = &tail[..end];
    let (kind, value) = if token.starts_with(|c: char| c.is_ascii_digit()) {
        (TimeKind::Valid, token)
    } else {
        let (k, v) = token.split_once(':')?;
        (TimeKind::parse(k)?, v)
    };
    let (lo, hi) = point(value)?;
    Some((
        TimeRange {
            kind,
            lower: Some(canonical(lo)),
            upper: Some(canonical(hi)),
        },
        tail[end..].trim_start(),
    ))
}
