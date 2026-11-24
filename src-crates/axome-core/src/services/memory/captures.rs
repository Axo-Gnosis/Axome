//! The capture log: one row per capture run, one per fact it saved, updated or found known.

use anyhow::{bail, Context, Result};
use axome_protocol::memory::{
    MemoryCaptureQuery, MemoryCaptureRun, MemoryCaptureTrigger, MemoryCapturedFact,
    MemoryFactOutcome,
};
use chrono::Utc;
use rusqlite::{params, Connection, Row};

use crate::services::db::DatabaseService;

pub(crate) struct NewRun {
    pub session_id: String,
    pub folder_path: String,
    pub title: Option<String>,
    pub bank: String,
    pub trigger: MemoryCaptureTrigger,
    pub anchor_entry_id: Option<String>,
}

/// What reverting a saved fact restores.
pub(crate) struct Undo {
    /// The exact line the capture appended.
    pub line: String,
    /// The observation an updated fact closed, when closing changed it.
    pub closed: Option<ClosedLine>,
}

/// A closed observation line: its note, and the line before and after closing.
pub(crate) struct ClosedLine {
    pub path: String,
    pub before: String,
    pub after: String,
}

pub(crate) struct NewFact {
    pub outcome: MemoryFactOutcome,
    pub category: String,
    pub text: String,
    pub previous: Option<String>,
    pub note: String,
    pub path: String,
    /// `None` for known facts.
    pub undo: Option<Undo>,
}

/// What a revert needs beyond the wire fact.
pub(crate) struct FactEdit {
    pub capture_id: i64,
    pub reverted: bool,
    pub path: String,
    pub undo: Option<Undo>,
}

fn trigger_text(trigger: MemoryCaptureTrigger) -> &'static str {
    match trigger {
        MemoryCaptureTrigger::Compaction => "compaction",
        MemoryCaptureTrigger::Close => "close",
    }
}

fn outcome_text(outcome: MemoryFactOutcome) -> &'static str {
    match outcome {
        MemoryFactOutcome::New => "new",
        MemoryFactOutcome::Updated => "updated",
        MemoryFactOutcome::Known => "known",
    }
}

fn invalid(column: usize, text: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(column, text.into(), rusqlite::types::Type::Text)
}

fn trigger(row: &Row, column: usize) -> rusqlite::Result<MemoryCaptureTrigger> {
    match row.get_ref(column)?.as_str()? {
        "compaction" => Ok(MemoryCaptureTrigger::Compaction),
        "close" => Ok(MemoryCaptureTrigger::Close),
        other => Err(invalid(column, other)),
    }
}

fn outcome(row: &Row, column: usize) -> rusqlite::Result<MemoryFactOutcome> {
    match row.get_ref(column)?.as_str()? {
        "new" => Ok(MemoryFactOutcome::New),
        "updated" => Ok(MemoryFactOutcome::Updated),
        "known" => Ok(MemoryFactOutcome::Known),
        other => Err(invalid(column, other)),
    }
}

pub(crate) fn start(db: &DatabaseService, run: NewRun) -> Result<i64> {
    let guard = db.write();
    guard.conn().execute(
        "INSERT INTO memory_captures (session_id, folder_path, title, bank, trigger, anchor_entry_id, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            run.session_id,
            run.folder_path,
            run.title,
            run.bank,
            trigger_text(run.trigger),
            run.anchor_entry_id,
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(guard.conn().last_insert_rowid())
}

pub(crate) fn add_fact(db: &DatabaseService, capture_id: i64, fact: NewFact) -> Result<()> {
    let (line, closed) = fact
        .undo
        .map_or((None, None), |undo| (Some(undo.line), undo.closed));
    let (closed_path, before, after) = closed.map_or((None, None, None), |closed| {
        (Some(closed.path), Some(closed.before), Some(closed.after))
    });
    db.write().conn().execute(
        "INSERT INTO memory_capture_facts (capture_id, outcome, category, text, previous, note, path, line_text, previous_path, previous_before, previous_after)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            capture_id,
            outcome_text(fact.outcome),
            fact.category,
            fact.text,
            fact.previous,
            fact.note,
            fact.path,
            line,
            closed_path,
            before,
            after
        ],
    )?;
    Ok(())
}

pub(crate) fn finish(
    db: &DatabaseService,
    capture_id: i64,
    error: Option<&str>,
    no_model: bool,
) -> Result<()> {
    db.write().conn().execute(
        "UPDATE memory_captures SET ended_at = ?2, error = ?3, no_model = ?4 WHERE id = ?1",
        params![capture_id, Utc::now().to_rfc3339(), error, no_model],
    )?;
    Ok(())
}

const RUN_COLUMNS: &str = "id, session_id, folder_path, title, bank, trigger, anchor_entry_id, started_at, ended_at, error, no_model";

fn run(row: &Row) -> rusqlite::Result<MemoryCaptureRun> {
    Ok(MemoryCaptureRun {
        id: row.get(0)?,
        session_id: row.get(1)?,
        folder_path: row.get(2)?,
        title: row.get(3)?,
        bank: row.get(4)?,
        trigger: trigger(row, 5)?,
        anchor_entry_id: row.get(6)?,
        started_at: row.get(7)?,
        ended_at: row.get(8)?,
        error: row.get(9)?,
        no_model: row.get(10)?,
        facts: Vec::new(),
    })
}

fn with_facts(conn: &Connection, mut run: MemoryCaptureRun) -> Result<MemoryCaptureRun> {
    let mut statement = conn.prepare_cached(
        "SELECT id, outcome, category, text, previous, note, path, reverted
         FROM memory_capture_facts WHERE capture_id = ?1 ORDER BY id",
    )?;
    run.facts = statement
        .query_map([run.id], |row| {
            Ok(MemoryCapturedFact {
                id: row.get(0)?,
                outcome: outcome(row, 1)?,
                category: row.get(2)?,
                text: row.get(3)?,
                previous: row.get(4)?,
                note: row.get(5)?,
                path: row.get(6)?,
                reverted: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(run)
}

pub(crate) fn get(db: &DatabaseService, capture_id: i64) -> Result<MemoryCaptureRun> {
    let guard = db.read();
    let run = guard
        .conn()
        .query_row(
            &format!("SELECT {RUN_COLUMNS} FROM memory_captures WHERE id = ?1"),
            [capture_id],
            run,
        )
        .context("Memory capture not found")?;
    with_facts(guard.conn(), run)
}

pub(crate) fn list(
    db: &DatabaseService,
    query: &MemoryCaptureQuery,
) -> Result<Vec<MemoryCaptureRun>> {
    let guard = db.read();
    let runs = guard
        .conn()
        .prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM memory_captures
             WHERE (?1 IS NULL OR session_id = ?1) AND (?2 IS NULL OR bank = ?2)
             ORDER BY id DESC LIMIT ?3"
        ))?
        .query_map(
            params![query.session_id, query.bank, query.limit.clamp(1, 500)],
            run,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    runs.into_iter()
        .map(|run| with_facts(guard.conn(), run))
        .collect()
}

pub(crate) fn fact_edit(db: &DatabaseService, fact_id: i64) -> Result<FactEdit> {
    type Columns = (
        i64,
        bool,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let (capture_id, reverted, path, line, closed_path, before, after): Columns = db
        .read()
        .conn()
        .query_row(
            "SELECT capture_id, reverted, path, line_text, previous_path, previous_before, previous_after
             FROM memory_capture_facts WHERE id = ?1",
            [fact_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .context("Memory fact not found")?;
    let closed = match (closed_path, before, after) {
        (Some(path), Some(before), Some(after)) => Some(ClosedLine {
            path,
            before,
            after,
        }),
        (None, None, None) => None,
        _ => bail!("Memory fact undo data is incomplete"),
    };
    Ok(FactEdit {
        capture_id,
        reverted,
        path,
        undo: line.map(|line| Undo { line, closed }),
    })
}

pub(crate) fn mark_reverted(db: &DatabaseService, fact_id: i64) -> Result<()> {
    db.write().conn().execute(
        "UPDATE memory_capture_facts SET reverted = 1 WHERE id = ?1",
        [fact_id],
    )?;
    Ok(())
}

/// Runs a previous process left open can never finish.
pub(crate) fn interrupt_unfinished(db: &DatabaseService) -> Result<()> {
    db.write().conn().execute(
        "UPDATE memory_captures SET ended_at = ?1, error = 'Axome closed before this capture finished.' WHERE ended_at IS NULL",
        [Utc::now().to_rfc3339()],
    )?;
    Ok(())
}
