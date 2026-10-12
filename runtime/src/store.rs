use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct Store(pub Connection);

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS traces (id TEXT PRIMARY KEY, status TEXT NOT NULL,
            metadata TEXT NOT NULL, created_at TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS steps (id INTEGER PRIMARY KEY, trace_id TEXT NOT NULL
            REFERENCES traces(id) ON DELETE CASCADE, model TEXT NOT NULL, input TEXT NOT NULL,
            output TEXT NOT NULL, cost REAL NOT NULL, latency_ms INTEGER NOT NULL, allowed INTEGER NOT NULL);
          CREATE INDEX IF NOT EXISTS steps_trace ON steps(trace_id);
          CREATE TABLE IF NOT EXISTS spend (month TEXT PRIMARY KEY, total REAL NOT NULL);")?;
        Ok(Self(connection))
    }

    pub fn create(&self, metadata: Value) -> Result<String> {
        let id = format!(
            "trace_{}_{}_{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        );
        self.0.execute(
            "INSERT INTO traces VALUES (?1,'pending',?2,?3)",
            params![id, metadata.to_string(), chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(id)
    }

    pub fn complete(&self, id: &str) -> Result<bool> {
        Ok(self
            .0
            .execute("UPDATE traces SET status='completed' WHERE id=?1", [id])?
            > 0)
    }

    pub fn fail(&self, id: &str, code: &str) -> Result<()> {
        self.0.execute("UPDATE traces SET status='failed',metadata=json_set(metadata,'$.error',?2) WHERE id=?1",params![id,code])?;
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
            Ok(json!({"model":row.get::<_,String>(0)?, "input":serde_json::from_str::<Value>(&input).unwrap_or(Value::Null),
                "output":serde_json::from_str::<Value>(&output).unwrap_or(Value::Null),
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
        let mut statement = self
            .0
            .prepare("SELECT id FROM traces ORDER BY created_at DESC LIMIT 100")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter()
            .map(|id| self.trace(id).map(|v| v.unwrap()))
            .collect()
    }

    pub fn reserve(&mut self, month: &str, amount: f64, limit: Option<f64>) -> Result<bool> {
        let transaction = self
            .0
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        transaction.execute("INSERT OR IGNORE INTO spend VALUES(?1,0)", [month])?;
        let spent: f64 =
            transaction.query_row("SELECT total FROM spend WHERE month=?1", [month], |row| {
                row.get(0)
            })?;
        if limit.is_some_and(|cap| spent + amount > cap) {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE spend SET total=total+?2 WHERE month=?1",
            params![month, amount],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn settle(&self, month: &str, reserved: f64, actual: f64) -> Result<()> {
        self.0.execute(
            "UPDATE spend SET total=MAX(0,total-?2+?3) WHERE month=?1",
            params![month, reserved, actual],
        )?;
        Ok(())
    }

    pub fn expire(&self, days: u32) -> Result<()> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
        self.0
            .execute("DELETE FROM traces WHERE created_at < ?1", [cutoff])?;
        Ok(())
    }
}
