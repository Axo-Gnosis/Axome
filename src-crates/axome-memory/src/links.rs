// Ported from Basic Memory (https://github.com/basicmachines-co/basic-memory, 7a1ff5f), AGPL-3.0-or-later.
use crate::{Index, Result};
use rusqlite::params;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Outgoing,
    Incoming,
}

#[derive(Debug, Serialize)]
pub struct Neighbor {
    pub entity_id: i64,
    pub relation_type: String,
    pub direction: Direction,
}

/// The notes `entity_id` links to and the notes linking to it, each once.
/// `[[bank::Target]]` links stay unresolved in SQLite; `resolve_cross(bank, target)`
/// resolves them in both directions.
pub fn neighbors(
    index: &Index,
    entity_id: i64,
    resolve_cross: &dyn Fn(&str, &str) -> Result<Option<i64>>,
) -> Result<Vec<Neighbor>> {
    let cross = |name: &str| match name.split_once("::") {
        Some((bank, target)) => resolve_cross(bank, target),
        None => Ok(None),
    };
    let mut edges = vec![];
    let mut stmt = index.conn.prepare(
        "SELECT from_id,to_id,to_name,relation_type FROM relation WHERE from_id=?1 OR to_id=?1 ORDER BY id",
    )?;
    for row in stmt.query_map([entity_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (from, to, name, kind) = row?;
        if from == entity_id {
            if let Some(to) = match to {
                Some(to) => Some(to),
                None => cross(&name)?,
            } {
                edges.push((to, kind, Direction::Outgoing));
            }
        } else {
            edges.push((from, kind, Direction::Incoming));
        }
    }
    let mut stmt = index.conn.prepare(
        "SELECT from_id,to_name,relation_type FROM relation WHERE to_id IS NULL AND instr(to_name,'::')>0 ORDER BY id",
    )?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (from, name, kind) = row?;
        if cross(&name)? == Some(entity_id) {
            edges.push((from, kind, Direction::Incoming));
        }
    }
    let mut seen = HashSet::from([entity_id]);
    Ok(edges
        .into_iter()
        .filter(|(id, _, _)| seen.insert(*id))
        .map(|(entity_id, relation_type, direction)| Neighbor {
            entity_id,
            relation_type,
            direction,
        })
        .collect())
}

/// A bank's notes, loaded once, for resolving many link targets.
pub struct Resolver {
    rows: Vec<(i64, String, String, String)>, // id, title, permalink, file_path
}

impl Resolver {
    pub fn load(index: &Index, bank_id: i64) -> Result<Self> {
        let mut stmt = index.conn.prepare(
            "SELECT id,title,permalink,file_path FROM entity WHERE bank_id=? ORDER BY id",
        )?;
        let rows = stmt
            .query_map([bank_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Self { rows })
    }

    // ponytail: linear scans per target; build title/permalink/path maps if a bank's link count shows up in profiles.
    pub fn resolve(&self, target: &str) -> Option<i64> {
        if target.contains("::") {
            return None;
        }
        let rows = &self.rows;
        if let Some(r) = rows.iter().find(|r| r.2 == target) {
            return Some(r.0);
        }
        let mut titles = rows.iter().filter(|r| r.1 == target);
        if let (Some(r), None) = (titles.next(), titles.next()) {
            return Some(r.0);
        }
        if let Some(r) = rows.iter().find(|r| r.3 == target) {
            return Some(r.0);
        }
        let md = format!("{target}.md");
        if let Some(r) = rows.iter().find(|r| r.3 == md) {
            return Some(r.0);
        }
        let normalize = |s: &str| s.replace('_', "-").to_lowercase();
        let target = normalize(target);
        let md = normalize(&md);
        let mut matches = rows.iter().filter(|r| {
            normalize(&r.1) == target
                || normalize(&r.2) == target
                || normalize(&r.3) == target
                || normalize(&r.3) == md
        });
        match (matches.next(), matches.next()) {
            (Some(r), None) => Some(r.0),
            _ => None,
        }
    }
}

pub fn resolve(index: &Index, bank_id: i64, target: &str) -> Result<Option<i64>> {
    Ok(Resolver::load(index, bank_id)?.resolve(target))
}
pub fn resolve_links(index: &Index, bank_id: i64) -> Result<()> {
    let resolver = Resolver::load(index, bank_id)?;
    let mut stmt=index.conn.prepare("SELECT r.id,r.to_name,r.to_id FROM relation r JOIN entity e ON e.id=r.from_id WHERE e.bank_id=?")?;
    let rows = stmt
        .query_map([bank_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, target, current) in rows {
        if target.contains("::") {
            continue;
        }
        let to = resolver.resolve(&target);
        if to != current {
            index
                .conn
                .execute("UPDATE relation SET to_id=? WHERE id=?", params![to, id])?;
        }
    }
    Ok(())
}
