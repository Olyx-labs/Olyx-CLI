use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct Store(pub Connection, Option<Session>);

struct Session {
    path: std::path::PathBuf,
    lock: Option<std::fs::File>,
}

impl Drop for Session {
    fn drop(&mut self) {
        drop(self.lock.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        #[cfg(unix)]
        if path != ":memory:" {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            for suffix in ["-wal", "-shm"] {
                let journal = format!("{path}{suffix}");
                if std::path::Path::new(&journal).exists() {
                    std::fs::set_permissions(journal, std::fs::Permissions::from_mode(0o600))?;
                }
            }
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS traces (id TEXT PRIMARY KEY, status TEXT NOT NULL,
            metadata TEXT NOT NULL, created_at TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS steps (id INTEGER PRIMARY KEY, trace_id TEXT NOT NULL
            REFERENCES traces(id) ON DELETE CASCADE, model TEXT NOT NULL, input TEXT NOT NULL,
            output TEXT NOT NULL, cost REAL NOT NULL, latency_ms INTEGER NOT NULL, allowed INTEGER NOT NULL);
          CREATE INDEX IF NOT EXISTS steps_trace ON steps(trace_id);
          CREATE TABLE IF NOT EXISTS trace_owners (trace_id TEXT PRIMARY KEY
            REFERENCES traces(id) ON DELETE CASCADE, owner TEXT NOT NULL);")?;
        let session = if path == ":memory:" {
            None
        } else {
            let database = std::fs::canonicalize(path)?;
            let mut directory = database.into_os_string();
            directory.push(".sessions");
            let directory = std::path::PathBuf::from(directory);
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.recursive(true).create(&directory)?;
            let owner = format!(
                "{}_{}_{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            );
            let lock_path = directory.join(owner);
            let mut options = std::fs::OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let lock = options.open(&lock_path)?;
            fs2::FileExt::lock_exclusive(&lock)?;
            Some(Session {
                path: lock_path,
                lock: Some(lock),
            })
        };
        let store = Self(connection, session);
        store.recover_interrupted()?;
        Ok(store)
    }

    pub fn create(&self, metadata: Value) -> Result<String> {
        let id = format!(
            "trace_{}_{}_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        );
        let transaction = self.0.unchecked_transaction()?;
        self.0.execute(
            "INSERT INTO traces VALUES (?1,'pending',?2,?3)",
            params![id, metadata.to_string(), chrono::Utc::now().to_rfc3339()],
        )?;
        if let Some(session) = &self.1 {
            let owner = session.path.file_name().unwrap().to_string_lossy();
            self.0.execute(
                "INSERT INTO trace_owners VALUES (?1,?2)",
                params![id, owner],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    fn recover_interrupted(&self) -> Result<()> {
        let Some(session) = &self.1 else {
            return Ok(());
        };
        let mut statement = self.0.prepare(
            "SELECT traces.id,trace_owners.owner FROM traces JOIN trace_owners ON traces.id=trace_owners.trace_id WHERE traces.status='pending'")?;
        let pending = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, owner) in pending {
            // Owners are generated locally; never follow paths stored in a modified database.
            if owner.is_empty()
                || !owner
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'_')
            {
                anyhow::bail!("Invalid trace owner");
            }
            let path = session.path.parent().unwrap().join(owner);
            let lock = match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
            {
                Ok(file) => Some(file),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            if let Some(file) = &lock {
                match fs2::FileExt::try_lock_exclusive(file) {
                    Ok(()) => (),
                    Err(error)
                        if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            self.fail_pending(&id, &json!({"code":"request_interrupted", "trace_id":id,
                "message":"The CLI process ended before the request finished.",
                "hint":"Inspect saved attempts before retrying; the provider may have processed the request. Recovery does not resend it.",
                "retryable":false}))?;
            drop(lock);
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn complete(&self, id: &str) -> Result<bool> {
        Ok(self
            .0
            .execute("UPDATE traces SET status='completed' WHERE id=?1", [id])?
            > 0)
    }

    pub fn fail_details(&self, id: &str, diagnostic: &Value) -> Result<()> {
        self.0.execute(
            "UPDATE traces SET status='failed',metadata=json_set(metadata,'$.error',?2,'$.diagnostic',json(?3)) WHERE id=?1",
            params![id, diagnostic["code"].as_str().unwrap_or("unknown_error"), diagnostic.to_string()],
        )?;
        Ok(())
    }

    pub fn fail_pending(&self, id: &str, diagnostic: &Value) -> Result<()> {
        self.0.execute(
            "UPDATE traces SET status='failed',metadata=json_set(metadata,'$.error',?2,'$.diagnostic',json(?3)) WHERE id=?1 AND status='pending'",
            params![id, diagnostic["code"].as_str().unwrap_or("request_cancelled"), diagnostic.to_string()],
        )?;
        Ok(())
    }

    pub fn record(
        &self,
        id: &str,
        model: &str,
        input: &Value,
        output: &Value,
        metrics: (f64, u128, bool),
    ) -> Result<()> {
        let (cost, latency, allowed) = metrics;
        self.0.execute(
            "INSERT INTO steps(trace_id,model,input,output,cost,latency_ms,allowed)
            VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                id,
                model,
                input.to_string(),
                output.to_string(),
                cost,
                latency as i64,
                allowed
            ],
        )?;
        Ok(())
    }

    pub fn trace(&self, id: &str) -> Result<Option<Value>> {
        let mut statement = self
            .0
            .prepare("SELECT status,metadata,created_at FROM traces WHERE id=?1")?;
        let mut rows = statement.query([id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let status: String = row.get(0)?;
        let metadata: String = row.get(1)?;
        let created: String = row.get(2)?;
        let mut steps = self.0.prepare("SELECT model,input,output,cost,latency_ms,allowed FROM steps WHERE trace_id=?1 ORDER BY id")?;
        let values = steps.query_map([id], |row| {
            let input: String = row.get(1)?;
            let output: String = row.get(2)?;
            Ok(json!({"model":row.get::<_,String>(0)?, "input":parse_json_column(&input, 1)?,
                "output":parse_json_column(&output, 2)?,
                "cost":row.get::<_,f64>(3)?,"latency_ms":row.get::<_,i64>(4)?,"allowed":row.get::<_,bool>(5)?}))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        let latency: i64 = values
            .iter()
            .filter_map(|value| value["latency_ms"].as_i64())
            .sum();
        let cost: f64 = values.iter().filter_map(|v| v["cost"].as_f64()).sum();
        Ok(Some(
            json!({"external_id":id,"status":status,"metadata":serde_json::from_str::<Value>(&metadata)?,
            "created_at":created,"steps":values,"total_cost":cost,"total_latency_ms":latency}),
        ))
    }

    pub fn list(&self) -> Result<Vec<Value>> {
        let transaction = self.0.unchecked_transaction()?;
        let mut statement = self
            .0
            .prepare("SELECT id FROM traces ORDER BY created_at DESC LIMIT 100")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let traces = ids
            .iter()
            .map(|id| {
                self.trace(id)?
                    .ok_or_else(|| anyhow::anyhow!("Trace disappeared during snapshot read"))
            })
            .collect::<Result<Vec<_>>>()?;
        drop(statement);
        transaction.commit()?;
        Ok(traces)
    }

    pub fn expire(&self, days: u32) -> Result<()> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        self.0
            .execute("DELETE FROM traces WHERE created_at < ?1", [cutoff])?;
        Ok(())
    }
}

fn parse_json_column(text: &str, index: usize) -> rusqlite::Result<Value> {
    serde_json::from_str(text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_details_are_atomic_when_update_is_rejected() {
        let store = Store::open(":memory:").unwrap();
        let id = store.create(json!({"source":"storage-test"})).unwrap();
        store
            .0
            .execute_batch(
                "CREATE TRIGGER reject_diagnostic BEFORE UPDATE ON traces
          WHEN json_type(NEW.metadata,'$.diagnostic') IS NOT NULL
          BEGIN SELECT RAISE(ABORT,'simulated write failure'); END;",
            )
            .unwrap();
        assert!(
            store
                .fail_details(&id, &json!({"code":"provider_unavailable"}))
                .is_err()
        );
        let trace = store.trace(&id).unwrap().unwrap();
        assert_eq!(trace["status"], "pending");
        assert!(trace["metadata"].get("error").is_none());
        assert!(trace["metadata"].get("diagnostic").is_none());
    }

    #[test]
    fn corrupt_step_json_is_reported_instead_of_replaced_with_null() {
        for column in ["input", "output"] {
            let store = Store::open(":memory:").unwrap();
            let id = store.create(json!({})).unwrap();
            store
                .record(
                    &id,
                    "fixture",
                    &json!({"request":"saved"}),
                    &json!({"response":"saved"}),
                    (0.0, 1, true),
                )
                .unwrap();
            store
                .0
                .execute(
                    &format!("UPDATE steps SET {column}='invalid json' WHERE trace_id=?1"),
                    [&id],
                )
                .unwrap();
            assert!(store.trace(&id).is_err(), "corrupt {column} was hidden");
        }
    }

    #[test]
    fn trace_listing_keeps_a_snapshot_during_concurrent_retention() {
        let path = std::env::temp_dir().join(format!(
            "olyx-storage-snapshot-{}-{}.db",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let reader = Store::open(path.to_str().unwrap()).unwrap();
        for _ in 0..50 {
            reader.create(json!({})).unwrap();
        }
        let writer_path = path.clone();
        let writer = std::thread::spawn(move || {
            let store = Store::open(writer_path.to_str().unwrap()).unwrap();
            for _ in 0..100 {
                let id = store.create(json!({})).unwrap();
                store
                    .0
                    .execute(
                        "UPDATE traces SET created_at='2000-01-01T00:00:00+00:00' WHERE id=?1",
                        [&id],
                    )
                    .unwrap();
                store.expire(1).unwrap();
            }
        });
        for _ in 0..100 {
            let traces = reader.list().unwrap();
            assert!(traces.iter().all(|trace| trace["external_id"].is_string()));
        }
        writer.join().unwrap();
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
}
