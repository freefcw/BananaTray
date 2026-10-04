use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection};

use super::sample::{
    HistoryStatus, HistoryUnit, HistoryValueKind, QuotaHistoryPoint, QuotaHistorySample,
};
use super::store::{
    HistoryError, HistoryPointRow, HistoryRangeQuery, HistoryRow, QuotaHistoryReader,
    QuotaHistoryWriter,
};

pub struct SqliteQuotaHistoryStore {
    conn: Connection,
    path: Option<std::path::PathBuf>,
}

impl SqliteQuotaHistoryStore {
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| HistoryError::new(format!("create history dir: {err}")))?;
        }
        let conn = Connection::open(path).map_err(sqlite_err)?;
        let store = Self {
            conn,
            path: Some(path.to_path_buf()),
        };
        store.configure()?;
        store.migrate()?;
        store.tighten_permissions();
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, HistoryError> {
        let conn = Connection::open_in_memory().map_err(sqlite_err)?;
        let store = Self { conn, path: None };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    fn configure(&self) -> Result<(), HistoryError> {
        self.conn
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(sqlite_err)?;
        self.conn
            .pragma_update(None, "synchronous", "NORMAL")
            .map_err(sqlite_err)?;
        self.conn
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sqlite_err)?;
        self.conn
            .pragma_update(None, "busy_timeout", "2000")
            .map_err(sqlite_err)?;
        Ok(())
    }

    fn migrate(&self) -> Result<(), HistoryError> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(sqlite_err)?;
        if version == 0 {
            self.conn
                .execute_batch(
                    "
                    CREATE TABLE quota_samples (
                      id INTEGER PRIMARY KEY,
                      captured_at_ms INTEGER NOT NULL,
                      provider_id TEXT NOT NULL,
                      status TEXT NOT NULL,
                      error_kind TEXT,
                      failure_reason TEXT,
                      failure_reason_payload TEXT,
                      failure_advice TEXT,
                      detail TEXT,
                      refresh_reason TEXT
                    );
                    CREATE TABLE quota_points (
                      sample_id INTEGER NOT NULL REFERENCES quota_samples(id) ON DELETE CASCADE,
                      quota_key TEXT NOT NULL,
                      quota_type TEXT NOT NULL,
                      label_spec_json TEXT NOT NULL,
                      value_kind TEXT NOT NULL,
                      unit TEXT,
                      used REAL,
                      remaining REAL,
                      limit_value REAL,
                      reset_at_secs INTEGER,
                      PRIMARY KEY (sample_id, quota_key)
                    );
                    CREATE INDEX idx_samples_provider_time
                      ON quota_samples(provider_id, captured_at_ms);
                    PRAGMA user_version = 1;
                    ",
                )
                .map_err(sqlite_err)?;
        }
        Ok(())
    }

    fn tighten_permissions(&self) {
        let Some(path) = &self.path else {
            return;
        };
        tighten(path);
        let mut wal = path.as_os_str().to_owned();
        wal.push("-wal");
        tighten(Path::new(&wal));
        let mut shm = path.as_os_str().to_owned();
        shm.push("-shm");
        tighten(Path::new(&shm));
    }
}

fn tighten(path: &Path) {
    #[cfg(unix)]
    if path.exists() {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

fn sqlite_err(err: rusqlite::Error) -> HistoryError {
    HistoryError::new(err.to_string())
}

fn history_point(row: &rusqlite::Row<'_>, quota_key: String) -> HistoryPointRow {
    let value_kind: String = row.get(13).unwrap_or_default();
    let unit: Option<String> = row.get(14).ok().flatten();
    HistoryPointRow {
        quota_key,
        quota_type: row.get(11).unwrap_or_default(),
        label_spec_json: row.get(12).unwrap_or_default(),
        value_kind: HistoryValueKind::parse(&value_kind).unwrap_or(HistoryValueKind::NonNumeric),
        unit: unit.as_deref().and_then(HistoryUnit::parse),
        used: row.get(15).ok().flatten(),
        remaining: row.get(16).ok().flatten(),
        limit_value: row.get(17).ok().flatten(),
        reset_at_secs: row.get(18).ok().flatten(),
    }
}

impl QuotaHistoryWriter for SqliteQuotaHistoryStore {
    fn append(&mut self, sample: &QuotaHistorySample) -> Result<(), HistoryError> {
        let tx = self.conn.transaction().map_err(sqlite_err)?;
        tx.execute(
            "INSERT INTO quota_samples (
                captured_at_ms, provider_id, status, error_kind, failure_reason,
                failure_reason_payload, failure_advice, detail, refresh_reason
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                sample.captured_at_ms,
                sample.provider_id,
                sample.status.as_str(),
                sample.error_kind,
                sample.failure_reason,
                sample.failure_reason_payload,
                sample.failure_advice_json,
                sample.detail,
                sample.refresh_reason,
            ],
        )
        .map_err(sqlite_err)?;
        let sample_id = tx.last_insert_rowid();
        let mut kept: BTreeMap<&str, &QuotaHistoryPoint> = BTreeMap::new();
        for point in &sample.points {
            if kept.insert(point.quota_key.as_str(), point).is_some() {
                log::warn!(
                    target: "history",
                    "duplicate quota_key {} for provider {}",
                    point.quota_key,
                    sample.provider_id
                );
            }
        }
        for point in kept.values() {
            tx.execute(
                "INSERT INTO quota_points (
                    sample_id, quota_key, quota_type, label_spec_json, value_kind,
                    unit, used, remaining, limit_value, reset_at_secs
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    sample_id,
                    point.quota_key,
                    point.quota_type,
                    point.label_spec_json,
                    point.value_kind.as_str(),
                    point.unit.map(super::sample::HistoryUnit::as_str),
                    point.used,
                    point.remaining,
                    point.limit_value,
                    point.reset_at_secs,
                ],
            )
            .map_err(sqlite_err)?;
        }
        tx.commit().map_err(sqlite_err)?;
        self.tighten_permissions();
        Ok(())
    }

    fn purge_provider(&mut self, provider_id: &str) -> Result<u64, HistoryError> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM quota_samples WHERE provider_id = ?1",
                params![provider_id],
            )
            .map_err(sqlite_err)?;
        Ok(changed as u64)
    }

    fn purge_provider_before(
        &mut self,
        provider_id: &str,
        captured_before_ms: i64,
    ) -> Result<u64, HistoryError> {
        let changed = self
            .conn
            .execute(
                "DELETE FROM quota_samples WHERE provider_id = ?1 AND captured_at_ms < ?2",
                params![provider_id, captured_before_ms],
            )
            .map_err(sqlite_err)?;
        Ok(changed as u64)
    }

    fn purge_all(&mut self) -> Result<u64, HistoryError> {
        let changed = self
            .conn
            .execute("DELETE FROM quota_samples", [])
            .map_err(sqlite_err)?;
        Ok(changed as u64)
    }
}

impl QuotaHistoryReader for SqliteQuotaHistoryStore {
    fn load_rows(&mut self, query: &HistoryRangeQuery) -> Result<Vec<HistoryRow>, HistoryError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT s.id, s.captured_at_ms, s.provider_id, s.status, s.error_kind,
                        s.failure_reason, s.failure_reason_payload, s.failure_advice,
                        s.detail, s.refresh_reason,
                        p.quota_key, p.quota_type, p.label_spec_json, p.value_kind, p.unit,
                        p.used, p.remaining, p.limit_value, p.reset_at_secs
                 FROM quota_samples s
                 LEFT JOIN quota_points p ON p.sample_id = s.id
                 WHERE s.provider_id = ?1
                   AND s.captured_at_ms >= ?2
                   AND s.captured_at_ms < ?3
                   AND (?4 OR s.status = 'success')
                 ORDER BY s.captured_at_ms, s.id",
            )
            .map_err(sqlite_err)?;
        let mut rows = statement
            .query(params![
                query.provider_id,
                query.captured_from_ms,
                query.captured_to_ms,
                query.include_non_success
            ])
            .map_err(sqlite_err)?;
        let mut grouped: Vec<HistoryRow> = Vec::new();
        while let Some(row) = rows.next().map_err(sqlite_err)? {
            let sample_id: i64 = row.get(0).map_err(sqlite_err)?;
            let point = row
                .get::<_, Option<String>>(10)
                .map_err(sqlite_err)?
                .map(|quota_key| history_point(row, quota_key));
            if grouped
                .last()
                .is_some_and(|sample| sample.sample_id == sample_id)
            {
                if let Some(point) = point {
                    grouped.last_mut().unwrap().points.push(point);
                }
                continue;
            }
            let status = HistoryStatus::parse(&row.get::<_, String>(3).map_err(sqlite_err)?)
                .ok_or_else(|| HistoryError::new("unknown history status"))?;
            grouped.push(HistoryRow {
                sample_id,
                captured_at_ms: row.get(1).map_err(sqlite_err)?,
                provider_id: row.get(2).map_err(sqlite_err)?,
                status,
                error_kind: row.get(4).map_err(sqlite_err)?,
                failure_reason: row.get(5).map_err(sqlite_err)?,
                failure_reason_payload: row.get(6).map_err(sqlite_err)?,
                failure_advice_json: row.get(7).map_err(sqlite_err)?,
                detail: row.get(8).map_err(sqlite_err)?,
                refresh_reason: row.get(9).map_err(sqlite_err)?,
                points: point.into_iter().collect(),
            });
        }
        Ok(grouped)
    }
}

#[cfg(test)]
mod tests {
    use super::super::sample::{
        HistoryStatus, HistoryUnit, HistoryValueKind, QuotaHistoryPoint, QuotaHistorySample,
    };
    use super::*;

    fn sample(provider: &str, captured: i64, points: Vec<QuotaHistoryPoint>) -> QuotaHistorySample {
        QuotaHistorySample {
            captured_at_ms: captured,
            provider_id: provider.to_string(),
            status: HistoryStatus::Success,
            error_kind: None,
            failure_reason: None,
            failure_reason_payload: None,
            failure_advice_json: None,
            detail: None,
            refresh_reason: Some("periodic".to_string()),
            points,
        }
    }

    #[test]
    fn failed_sample_round_trips_with_no_points() {
        let mut store = SqliteQuotaHistoryStore::open_in_memory().unwrap();
        store
            .append(&QuotaHistorySample {
                captured_at_ms: 5,
                provider_id: "codex".to_string(),
                status: HistoryStatus::Failed,
                error_kind: Some("unknown".to_string()),
                failure_reason: Some("no_data".to_string()),
                failure_reason_payload: None,
                failure_advice_json: None,
                detail: None,
                refresh_reason: None,
                points: Vec::new(),
            })
            .unwrap();
        let rows = store
            .load_rows(&HistoryRangeQuery {
                provider_id: "codex".to_string(),
                captured_from_ms: 0,
                captured_to_ms: 10,
                include_non_success: true,
            })
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].points.is_empty());
        assert_eq!(rows[0].status, HistoryStatus::Failed);
    }

    #[test]
    fn duplicate_quota_key_keeps_the_last_point() {
        let mut store = SqliteQuotaHistoryStore::open_in_memory().unwrap();
        store
            .append(&sample(
                "codex",
                5,
                vec![point("weekly", 1.0), point("weekly", 9.0)],
            ))
            .unwrap();
        let rows = store
            .load_rows(&HistoryRangeQuery {
                provider_id: "codex".to_string(),
                captured_from_ms: 0,
                captured_to_ms: 10,
                include_non_success: true,
            })
            .unwrap();
        assert_eq!(rows[0].points.len(), 1);
        assert_eq!(rows[0].points[0].used, Some(9.0));
    }

    #[test]
    fn purge_before_does_not_cross_providers() {
        let mut store = SqliteQuotaHistoryStore::open_in_memory().unwrap();
        store
            .append(&sample("codex", 1, vec![point("session", 1.0)]))
            .unwrap();
        store
            .append(&sample("codex", 100, vec![point("session", 2.0)]))
            .unwrap();
        store
            .append(&sample("claude", 1, vec![point("session", 3.0)]))
            .unwrap();
        let removed = store.purge_provider_before("codex", 50).unwrap();
        assert_eq!(removed, 1);
        let codex = store
            .load_rows(&HistoryRangeQuery {
                provider_id: "codex".to_string(),
                captured_from_ms: 0,
                captured_to_ms: 1_000,
                include_non_success: true,
            })
            .unwrap();
        let claude = store
            .load_rows(&HistoryRangeQuery {
                provider_id: "claude".to_string(),
                captured_from_ms: 0,
                captured_to_ms: 1_000,
                include_non_success: true,
            })
            .unwrap();
        assert_eq!(codex.len(), 1);
        assert_eq!(claude.len(), 1);
    }

    fn point(key: &str, used: f64) -> QuotaHistoryPoint {
        QuotaHistoryPoint {
            quota_key: key.to_string(),
            quota_type: "general".to_string(),
            label_spec_json: "{}".to_string(),
            value_kind: HistoryValueKind::Metered,
            unit: Some(HistoryUnit::Percentage),
            used: Some(used),
            remaining: None,
            limit_value: Some(100.0),
            reset_at_secs: None,
        }
    }
}
