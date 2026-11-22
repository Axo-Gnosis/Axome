// Ported from Basic Memory (https://github.com/basicmachines-co/basic-memory, 7a1ff5f), AGPL-3.0-or-later.
use crate::{
    index::{EMBED_DIM, EMBED_MODEL},
    temporal, Index, Result,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
#[derive(Default)]
pub struct SearchQuery<'a> {
    pub bank_ids: &'a [i64],
    pub text: &'a str,
    pub query_embedding: Option<&'a [f32]>,
    pub row_types: &'a [&'a str],
    pub note_types: &'a [String],
    pub categories: &'a [String],
    pub tags: &'a [String],
    pub path_prefix: Option<&'a str>,
    pub after: Option<&'a str>,
    pub valid_at: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub row_type: String,
    pub row_id: i64,
    pub entity_id: i64,
    pub bank_id: i64,
    pub title: String,
    pub permalink: String,
    pub file_path: String,
    pub category: Option<String>,
    pub snippet: String,
    pub score: f32,
    pub line: Option<usize>,
}
struct Candidate {
    hit: SearchHit,
    note_type: String,
    tags: Vec<String>,
    updated: String,
    time: Option<temporal::TimeRange>,
}
fn candidate(
    index: &Index,
    kind: String,
    row: i64,
    entity: i64,
    snippet: String,
    score: f32,
) -> Result<Option<Candidate>> {
    let Some((bank_id, title, note_type, permalink, file_path, tags, updated)) = index.conn.query_row(
        "SELECT bank_id,title,note_type,permalink,file_path,tags,updated_at FROM entity WHERE id=?",
        [entity],
        |r| Ok((
            r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        )),
    ).optional()? else {
        return Ok(None);
    };
    let mut tags: Vec<String> = serde_json::from_str(&tags)?;
    let mut category = None;
    let mut line = None;
    let mut time = None;
    if kind == "observation" {
        let Some((obs, _)) = index.observation_with_entity(row)? else {
            return Ok(None);
        };
        category = Some(obs.category);
        line = Some(obs.line);
        tags.extend(obs.tags);
        time = obs.time;
    }
    Ok(Some(Candidate {
        hit: SearchHit {
            row_type: kind,
            row_id: row,
            entity_id: entity,
            bank_id,
            title,
            permalink,
            file_path,
            category,
            snippet,
            score,
            line,
        },
        note_type,
        tags,
        updated,
        time,
    }))
}
fn allowed(c: &Candidate, q: &SearchQuery<'_>) -> bool {
    q.bank_ids.contains(&c.hit.bank_id)
        && (q.row_types.is_empty() || q.row_types.contains(&c.hit.row_type.as_str()))
        && (q.note_types.is_empty() || q.note_types.contains(&c.note_type))
        && (q.categories.is_empty()
            || c.hit
                .category
                .as_ref()
                .is_some_and(|x| q.categories.contains(x)))
        && q.tags.iter().all(|t| c.tags.contains(t))
        && q.path_prefix.is_none_or(|p| c.hit.file_path.starts_with(p))
        && q.after.is_none_or(|a| {
            temporal::instant(a)
                .zip(temporal::instant(&c.updated))
                .is_some_and(|(a, u)| u > a)
        })
        && q.valid_at
            .is_none_or(|a| c.time.as_ref().is_none_or(|t| temporal::contains(t, a)))
}

const RELAXATION_STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "did", "do", "does", "for", "from",
    "had", "has", "have", "how", "i", "in", "is", "it", "of", "on", "or", "our", "that", "the",
    "their", "they", "this", "to", "was", "we", "were", "what", "when", "where", "which", "who",
    "whom", "whose", "why", "will", "with", "you", "your",
];

/// Content words for OR-relaxing a strict query that found nothing, or `None` when
/// relaxing would over-broaden: quoted or boolean queries, fewer than three words,
/// or any bare number (identifier-like queries such as "SPEC 16").
fn relaxed_words(text: &str) -> Option<Vec<String>> {
    let text = text.trim();
    if text.contains('"')
        || [" AND ", " OR ", " NOT "]
            .iter()
            .any(|op| format!(" {text} ").contains(op))
    {
        return None;
    }
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|t| t.trim_matches('\''))
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() < 3 || tokens.iter().any(|t| t.chars().all(char::is_numeric)) {
        return None;
    }
    let pruned: Vec<&str> = tokens
        .iter()
        .copied()
        .filter(|t| !RELAXATION_STOPWORDS.contains(t))
        .collect();
    let mut words: Vec<String> = Vec::new();
    for word in if pruned.is_empty() { &tokens } else { &pruned } {
        if !words.iter().any(|w| w == word) {
            words.push((*word).to_owned());
        }
    }
    Some(words)
}

fn keyword(index: &Index, q: &SearchQuery<'_>, query: &str, banks: &str) -> Result<Vec<SearchHit>> {
    let mut fts = vec![];
    if query.is_empty() {
        return Ok(fts);
    }
    let mut stmt = index.conn.prepare(&format!("SELECT row_type,row_id,entity_id,snippet(search_index,1,'','',' … ',40),-bm25(search_index) FROM search_index WHERE search_index MATCH ? AND bank_id IN ({banks}) ORDER BY bm25(search_index)"))?;
    let rows = stmt.query_map([query], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })?;
    for row in rows {
        let (kind, id, entity, snippet, score) = row?;
        if let Some(c) = candidate(index, kind, id, entity, snippet, score)? {
            if allowed(&c, q) {
                fts.push(c.hit);
                if fts.len() == 100 {
                    break;
                }
            }
        }
    }
    Ok(fts)
}
pub fn search(index: &Index, q: &SearchQuery<'_>) -> Result<Vec<SearchHit>> {
    if q.bank_ids.is_empty() || q.limit == 0 {
        return Ok(vec![]);
    }
    let banks = q
        .bank_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    // Upstream's prepare_single_term: every word is a prefix match ("auth" finds
    // "authentication"), except a note path, which must match exactly.
    let quote = |t: &str| {
        let quoted = format!("\"{}\"", t.replace('"', "\"\""));
        if t.contains('/') && t.ends_with(".md") {
            quoted
        } else {
            quoted + "*"
        }
    };
    let strict = q
        .text
        .split_whitespace()
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ");
    let mut fts = keyword(index, q, &strict, &banks)?;
    // Upstream's relaxed fallback (`search_query.py` relaxed_query_words): OR-ing every
    // term lets a lone stopword hit normalize to 1.0 and outrank the vector result.
    // ponytail: CJK and format-character tokenization rules are not ported.
    if fts.is_empty() {
        if let Some(words) = relaxed_words(q.text) {
            let relaxed = words
                .iter()
                .map(|w| quote(w))
                .collect::<Vec<_>>()
                .join(" OR ");
            fts = keyword(index, q, &relaxed, &banks)?;
        }
    }
    let max = fts.iter().map(|h| h.score).fold(0.0_f32, f32::max);
    if max > 0.0 {
        for h in &mut fts {
            h.score /= max;
        }
    }
    let mut vectors: HashMap<(String, i64), SearchHit> = HashMap::new();
    if let Some(query) = q.query_embedding.filter(|v| v.len() == EMBED_DIM) {
        // ponytail: brute-force cosine; move to sqlite-vec if this shows in profiles.
        let mut stmt=index.conn.prepare(&format!("SELECT c.row_type,c.row_id,c.entity_id,c.text,c.embedding FROM chunk c JOIN entity e ON e.id=c.entity_id WHERE c.model=? AND c.embedding IS NOT NULL AND e.bank_id IN ({banks})"))?;
        let mut rows = stmt.query(params![EMBED_MODEL])?;
        while let Some(row) = rows.next()? {
            let blob = row.get_ref(4)?.as_blob().map_err(rusqlite::Error::from)?;
            if blob.len() != EMBED_DIM * 4 {
                continue;
            }
            let score = blob
                .chunks_exact(4)
                .zip(query)
                .map(|(b, q)| f32::from_le_bytes(b.try_into().unwrap()) * q)
                .sum::<f32>();
            if !score.is_finite() || score < 0.55 {
                continue;
            }
            let kind: String = row.get(0)?;
            let id: i64 = row.get(1)?;
            let key = (kind.clone(), id);
            if vectors.get(&key).is_some_and(|h| h.score >= score) {
                continue;
            }
            let text = row.get_ref(3)?.as_str().map_err(rusqlite::Error::from)?;
            let snippet = text.chars().take(200).collect();
            if let Some(c) = candidate(index, kind, id, row.get(2)?, snippet, score)? {
                if allowed(&c, q) {
                    vectors.insert(key, c.hit);
                }
            }
        }
    }
    let mut vectors: Vec<_> = vectors.into_values().collect();
    sort(&mut vectors);
    vectors.truncate(100);
    let mut hits = fuse(fts, vectors);
    sort(&mut hits);
    Ok(hits.into_iter().skip(q.offset).take(q.limit).collect())
}
fn sort(hits: &mut [SearchHit]) {
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.row_type.cmp(&b.row_type))
            .then_with(|| a.row_id.cmp(&b.row_id))
    });
}
fn fuse(fts: Vec<SearchHit>, vectors: Vec<SearchHit>) -> Vec<SearchHit> {
    let mut out: HashMap<_, _> = fts
        .into_iter()
        .map(|h| ((h.row_type.clone(), h.row_id), h))
        .collect();
    for h in vectors {
        let key = (h.row_type.clone(), h.row_id);
        if let Some(f) = out.get_mut(&key) {
            f.score = f.score.max(h.score) + 0.3 * f.score.min(h.score);
        } else {
            out.insert(key, h);
        }
    }
    out.into_values().collect()
}
