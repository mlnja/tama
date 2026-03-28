use std::sync::{LazyLock, Mutex};

use anyhow::Result;
use duckdb::Connection;

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Records side-effectful actions during an agent span so they can be undone on retry.
///
/// Not tied to any specific tool. On retry, `rollback(span_id)` traverses all recorded
/// actions for that span in reverse order and undoes the ones it knows about.
/// Unknown action types are silently skipped.
pub trait Rollbacker: Send {
    /// Record that a tool call happened. Called BEFORE the write so `old_value` reflects
    /// the state before the mutation.
    fn record_tool_call(
        &mut self,
        span_id: &str,
        tool_name: &str,
        key: &str,
        old_value: Option<&str>,
    );

    /// Traverse all actions recorded for `span_id` in reverse, undoing known mutations.
    fn rollback(&mut self, span_id: &str);

    /// Drop runtime state (counters etc.) — called at the start of each `runtime::run`.
    fn clear(&mut self);
}

// ── NoopRollbacker ────────────────────────────────────────────────────────────

/// Zero-cost. Used in production (`tamad` without a debug hook) where retries never occur.
pub struct NoopRollbacker;

impl Rollbacker for NoopRollbacker {
    fn record_tool_call(&mut self, _: &str, _: &str, _: &str, _: Option<&str>) {}
    fn rollback(&mut self, _: &str) {}
    fn clear(&mut self) {}
}

// ── DuckDbRollbacker ──────────────────────────────────────────────────────────

/// Persists the ordered action log to DuckDB — the single source of truth for rollback.
///
/// Schema: `action_log(seq, span_id, tool_name, key, old_value)`
///
/// On `rollback(span_id)`: reads rows for this span in DESC seq order and applies
/// undo logic for each tool_name it recognises:
///   - `tama_mem_set` → restore key to old_value (or delete if old_value was NULL)
///   - anything else  → skip (no rollback handler registered)
pub struct DuckDbRollbacker {
    conn: Connection,
    seq: i64,
}

impl DuckDbRollbacker {
    pub fn new(db_path: &str) -> Result<Self> {
        std::fs::create_dir_all(
            std::path::Path::new(db_path)
                .parent()
                .unwrap_or(std::path::Path::new(".")),
        )?;
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS action_log (
                seq       INTEGER,
                span_id   TEXT,
                tool_name TEXT,
                key       TEXT,
                old_value TEXT    -- NULL means key did not exist before the write
            );
        ",
        )?;
        Ok(DuckDbRollbacker { conn, seq: 0 })
    }
}

impl Rollbacker for DuckDbRollbacker {
    fn record_tool_call(
        &mut self,
        span_id: &str,
        tool_name: &str,
        key: &str,
        old_value: Option<&str>,
    ) {
        let _ = self.conn.execute(
            "INSERT INTO action_log (seq, span_id, tool_name, key, old_value) VALUES (?,?,?,?,?)",
            duckdb::params![self.seq, span_id, tool_name, key, old_value],
        );
        self.seq += 1;
    }

    fn rollback(&mut self, span_id: &str) {
        // Read the log for this span in reverse order.
        let Ok(mut stmt) = self.conn.prepare(
            "SELECT tool_name, key, old_value FROM action_log WHERE span_id = ? ORDER BY seq DESC",
        ) else {
            return;
        };

        let rows: Vec<(String, String, Option<String>)> = stmt
            .query_map(duckdb::params![span_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .into_iter()
            .flatten()
            .flatten()
            .collect();

        for (tool_name, key, old_value) in rows {
            match tool_name.as_str() {
                "tama_mem_set" => match old_value {
                    Some(ref v) => {
                        eprintln!("  rollback: mem[{key}] ← {v:?}");
                        crate::runtime::tools::inmemory::set(&key, v);
                    }
                    None => {
                        eprintln!("  rollback: mem[{key}] ← (deleted)");
                        crate::runtime::tools::inmemory::delete(&key);
                    }
                },
                other => {
                    eprintln!("  rollback: {other} — no handler, skipping");
                }
            }
        }

        // Remove the rolled-back entries so they don't apply on a second retry.
        let _ = self.conn.execute(
            "DELETE FROM action_log WHERE span_id = ?",
            duckdb::params![span_id],
        );
        eprintln!("  rollback: completed for span {span_id}");
    }

    fn clear(&mut self) {
        self.seq = 0;
        // Historical rows in action_log are kept across runs for audit purposes.
    }
}

// ── Global registry ───────────────────────────────────────────────────────────

static ROLLBACKER: LazyLock<Mutex<Box<dyn Rollbacker + Send>>> =
    LazyLock::new(|| Mutex::new(Box::new(NoopRollbacker)));

/// Replace the active rollbacker. Must be called before any agents run.
pub fn install(r: impl Rollbacker + 'static) {
    *ROLLBACKER.lock().unwrap() = Box::new(r);
}

/// Record a tool call BEFORE the write. No-op when rollbacker is `NoopRollbacker`.
pub fn record_tool_call(span_id: &str, tool_name: &str, key: &str, old_value: Option<&str>) {
    ROLLBACKER
        .lock()
        .unwrap()
        .record_tool_call(span_id, tool_name, key, old_value);
}

/// Undo all recorded actions for `span_id` in reverse. Called on agent retry.
pub fn rollback(span_id: &str) {
    ROLLBACKER.lock().unwrap().rollback(span_id);
}

/// Reset per-run state. Called at the start of each `runtime::run`.
pub fn clear() {
    ROLLBACKER.lock().unwrap().clear();
}
