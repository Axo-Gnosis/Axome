// Ported from Basic Memory (https://github.com/basicmachines-co/basic-memory, 7a1ff5f), AGPL-3.0-or-later.
use crate::{format, index::bank_path, links, permalink::permalink, Index, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::UNIX_EPOCH,
};
use walkdir::WalkDir;
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct SyncReport {
    pub added: usize,
    pub changed: usize,
    pub moved: usize,
    pub deleted: usize,
}
impl SyncReport {
    pub fn has_changes(&self) -> bool {
        self.added + self.changed + self.moved + self.deleted > 0
    }
}
struct Old {
    id: i64,
    checksum: String,
    mtime: f64,
    size: u64,
    permalink: String,
}
fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
pub fn sync_bank(index: &mut Index, bank_dir: &Path) -> Result<SyncReport> {
    let bank_id = index.upsert_bank(bank_dir)?;
    let root = bank_path(bank_dir)?;
    let mut old: HashMap<String, Old> = index
        .conn
        .prepare("SELECT file_path,id,checksum,mtime,size,permalink FROM entity WHERE bank_id=?")?
        .query_map([bank_id], |r| {
            Ok((
                r.get(0)?,
                Old {
                    id: r.get(1)?,
                    checksum: r.get(2)?,
                    mtime: r.get(3)?,
                    size: r.get(4)?,
                    permalink: r.get(5)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut paths = vec![];
    let mut protected = vec![];
    // ponytail: scan on every call; add notify if the walk becomes measurable.
    if root.try_exists()? {
        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0
                    || !e.file_type().is_dir()
                    || !e.file_name().to_string_lossy().starts_with('.')
            })
        {
            match entry {
                Ok(e)
                    if e.file_type().is_file()
                        && e.path().extension().is_some_and(|x| x == "md") =>
                {
                    paths.push(e.into_path())
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error=%e,"Memory subtree unreadable");
                    protected.push(e.path().unwrap_or(&root).to_path_buf());
                }
            }
        }
    }
    paths.sort();
    let present: HashSet<String> = paths
        .iter()
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let tx = index.conn.transaction()?;
    let mut report = SyncReport::default();
    for path in paths {
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error=%e,path=%path.display(),"Memory file unreadable");
                continue;
            }
        };
        let mtime = meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        let size = meta.len();
        if old
            .get(&relative)
            .is_some_and(|o| o.size == size && (o.mtime - mtime).abs() < 0.01)
        {
            old.remove(&relative);
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error=%e,path=%path.display(),"Memory file unreadable");
                continue;
            }
        };
        let checksum = hash(&bytes);
        if let Some(o) = old.get(&relative) {
            if o.checksum == checksum {
                tx.execute(
                    "UPDATE entity SET mtime=?,size=? WHERE id=?",
                    params![mtime, size, o.id],
                )?;
                old.remove(&relative);
                continue;
            }
        }
        let text = match std::str::from_utf8(&bytes) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error=%e,path=%path.display(),"Memory note is not UTF-8");
                continue;
            }
        };
        let moved = if old.contains_key(&relative) {
            None
        } else {
            old.iter()
                .filter(|(p, o)| {
                    o.checksum == checksum
                        && !present.contains(*p)
                        && !protected.iter().any(|b| root.join(p).starts_with(b))
                })
                .min_by_key(|(p, _)| *p)
                .map(|(p, _)| p.clone())
        };
        let previous = old.remove(moved.as_ref().unwrap_or(&relative));
        let note = format::parse(text);
        let title = note
            .frontmatter
            .title
            .as_deref()
            .unwrap_or_else(|| path.file_stem().unwrap().to_str().unwrap_or("note"));
        let timestamp = DateTime::<Utc>::from_timestamp(mtime as i64, 0)
            .unwrap_or_default()
            .to_rfc3339();
        let date = |s: Option<&str>| {
            s.and_then(crate::temporal::instant)
                .map(|d| d.to_rfc3339())
                .unwrap_or_else(|| timestamp.clone())
        };
        let created = date(note.frontmatter.created.as_deref());
        let updated = date(note.frontmatter.modified.as_deref());
        let id = if let Some(o) = &previous {
            o.id
        } else {
            tx.execute("INSERT INTO entity(bank_id,title,note_type,permalink,file_path,checksum,mtime,size,tags,created_at,updated_at) VALUES (?,'','note','','','',0,0,'[]','','')",[bank_id])?;
            tx.last_insert_rowid()
        };
        let kept = previous
            .as_ref()
            .filter(|_| moved.is_none())
            .map(|o| o.permalink.as_str());
        let slug = unique_permalink(
            &tx,
            bank_id,
            id,
            note.frontmatter.permalink.as_deref(),
            kept,
            &relative,
            &present,
        )?;
        let mut cached = HashMap::<String, (Option<String>, Option<Vec<u8>>)>::new();
        {
            let mut stmt =
                tx.prepare("SELECT text_hash,model,embedding FROM chunk WHERE entity_id=?")?;
            for row in stmt.query_map([id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))? {
                let (h, v) = row?;
                cached.insert(h, v);
            }
        }
        tx.execute("DELETE FROM search_index WHERE entity_id=?", [id])?;
        tx.execute("DELETE FROM observation WHERE entity_id=?", [id])?;
        tx.execute("DELETE FROM relation WHERE from_id=?", [id])?;
        tx.execute("DELETE FROM chunk WHERE entity_id=?", [id])?;
        tx.execute("UPDATE entity SET title=?,note_type=?,permalink=?,file_path=?,checksum=?,mtime=?,size=?,tags=?,created_at=?,updated_at=? WHERE id=?",params![title,note.frontmatter.note_type.as_deref().unwrap_or("note"),slug,relative,checksum,mtime,size,serde_json::to_string(&note.frontmatter.tags)?,created,updated,id])?;
        let fts = |kind: &str, row: i64, content: &str, category: Option<&str>| -> Result<()> {
            tx.execute("INSERT INTO search_index(title,content,permalink,row_type,row_id,entity_id,bank_id,category) VALUES (?,?,?,?,?,?,?,?)",params![title,content,slug,kind,row,id,bank_id,category])?;
            Ok(())
        };
        let chunk = |kind: &str, row: i64, text: &str| -> Result<()> {
            let h = hash(text.as_bytes());
            let cached = cached.get(&h);
            tx.execute("INSERT INTO chunk(entity_id,row_type,row_id,text,text_hash,model,embedding) VALUES (?,?,?,?,?,?,?)",params![id,kind,row,text,h,cached.and_then(|v|v.0.as_deref()),cached.and_then(|v|v.1.as_deref())])?;
            Ok(())
        };
        fts("entity", id, &note.body, None)?;
        for text in chunks(&note.body) {
            chunk("entity", id, &text)?;
        }
        for obs in &note.observations {
            tx.execute("INSERT INTO observation(entity_id,category,content,context,tags,time_kind,time_lower,time_upper,line) VALUES (?,?,?,?,?,?,?,?,?)",params![id,obs.category,obs.content,obs.context,serde_json::to_string(&obs.tags)?,obs.time.as_ref().map(|t|t.kind.as_str()),obs.time.as_ref().and_then(|t|t.lower.as_deref()),obs.time.as_ref().and_then(|t|t.upper.as_deref()),obs.line as i64])?;
            let row = tx.last_insert_rowid();
            fts("observation", row, &obs.content, Some(&obs.category))?;
            chunk("observation", row, &obs.content)?;
        }
        for rel in &note.relations {
            tx.execute(
                "INSERT INTO relation(from_id,to_name,relation_type,context) VALUES (?,?,?,?)",
                params![id, rel.target, rel.relation_type, rel.context],
            )?;
            fts(
                "relation",
                tx.last_insert_rowid(),
                &format!(
                    "{} {} {}",
                    rel.relation_type,
                    rel.target,
                    rel.context.as_deref().unwrap_or("")
                ),
                None,
            )?;
        }
        if moved.is_some() {
            report.moved += 1;
        } else if previous.is_some() {
            report.changed += 1;
        } else {
            report.added += 1;
        }
    }
    for (path, o) in old {
        if !present.contains(&path) && !protected.iter().any(|p| root.join(&path).starts_with(p)) {
            delete_entity(&tx, o.id)?;
            report.deleted += 1;
        }
    }
    tx.commit()?;
    if report.has_changes() {
        links::resolve_links(index, bank_id)?;
    }
    Ok(report)
}
fn delete_entity(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM search_index WHERE entity_id=?", [id])?;
    conn.execute("DELETE FROM entity WHERE id=?", [id])?;
    Ok(())
}

/// Upstream's `resolve_permalink`: a frontmatter permalink nobody else holds wins,
/// then the permalink this note already has, then the desired one with `-1`, `-2`, ….
/// Rows whose file vanished in this scan are about to move or be deleted, so they
/// hold nothing.
fn unique_permalink(
    conn: &Connection,
    bank_id: i64,
    id: i64,
    frontmatter: Option<&str>,
    kept: Option<&str>,
    relative: &str,
    present: &HashSet<String>,
) -> Result<String> {
    let mut owners =
        conn.prepare("SELECT file_path FROM entity WHERE bank_id=? AND permalink=? AND id!=?")?;
    let mut taken = |slug: &str| -> Result<bool> {
        for path in owners.query_map(params![bank_id, slug, id], |r| r.get::<_, String>(0))? {
            if present.contains(&path?) {
                return Ok(true);
            }
        }
        Ok(false)
    };
    let frontmatter = frontmatter.filter(|s| !s.is_empty());
    if let Some(slug) = frontmatter {
        if !taken(slug)? {
            return Ok(slug.to_owned());
        }
    }
    if let Some(slug) = kept.filter(|s| !s.is_empty()) {
        return Ok(slug.to_owned());
    }
    let desired = frontmatter.map_or_else(|| permalink(relative), str::to_owned);
    let mut slug = desired.clone();
    for suffix in 1.. {
        if !taken(&slug)? {
            break;
        }
        slug = format!("{desired}-{suffix}");
    }
    Ok(slug)
}
fn chunks(body: &str) -> Vec<String> {
    let mut out = vec![];
    let mut current = String::new();
    let mut count = 0;
    for line in body.split_inclusive('\n') {
        let n = line.chars().count();
        if !current.is_empty() && (crate::edit::heading(line).is_some() || count + n > 900) {
            out.push(std::mem::take(&mut current));
            count = 0;
        }
        for c in line.chars() {
            if count == 900 {
                out.push(std::mem::take(&mut current));
                count = 0;
            }
            current.push(c);
            count += 1;
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}
