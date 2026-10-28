# Database and storage setup

No database server is needed. The CLI bundles SQLite and creates the database
and missing parent directories when a local runtime command opens it.

Start with `examples/olyx.json`. Its `database` path is resolved relative to that
configuration file, so `../data/traces.db` creates the archive's `data` directory.
Provider credentials stay in environment variables rather than the configuration.

For durable storage, change `database` to a file in your user directory or on a
persistent local volume. Pass that same configuration to route, debug, and replay:

```sh
oy check examples/olyx.json
oy route --config examples/olyx.json --input examples/request.json
oy debug --config examples/olyx.json
oy replay TRACE_ID --config examples/olyx.json
```

`OLYX_CONFIG` can replace `--config`. The CLI creates traces during routing and
creates new traces during replay; it preserves the source trace.

Omit `database` to use `data/traces.db` beside your configuration file.
No database server or dashboard is needed.

## Storage options

| Setting | Behavior |
| --- | --- |
| A relative file path | Persistent SQLite file, relative to the configuration |
| An absolute file path | Persistent SQLite file at that location |
| `":memory:"` | Ephemeral database for this CLI invocation |

A new CLI process cannot debug or replay another process's in-memory traces.
PostgreSQL, MySQL, and object storage are not SQLite backends for this release.
Trace storage is local and is not synchronized to a hosted service.

## Retention and access

`policy.retention_days` defaults to 30. Expiration runs when a local runtime
command opens the database; it is not a background cleanup service.

Traces contain original requests and responses, including native provider details.
Routing does not scan or redact them. On Unix, newly created trace directories
use mode 0700 and database/journal files use mode 0600.

## Backup and restore

SQLite uses WAL mode with full synchronous commits. Completed writes survive
process restarts. After an abrupt termination, the next database open marks
abandoned traces as failed with `request_interrupted`. Process locks protect requests still running
in other CLI processes. Saved steps are preserved; recovery never resends a
request. Inspect them before retrying, since upstream usage may have occurred.
Keep the adjacent `.sessions` directory with the database while processes run,
and use a local filesystem with working file locks. Pending traces created by
older CLI versions lack ownership records and cannot be safely recovered
automatically. Corrupt stored JSON is reported as a
storage error rather than silently replaced with an empty value.

Use SQLite's backup API to take a consistent snapshot
instead of copying the main file while the CLI is writing to it:

```sh
python3 - <<'PY'
import sqlite3
with sqlite3.connect("data/traces.db") as source:
    with sqlite3.connect("data/traces.backup.db") as destination:
        source.backup(destination)
PY
```

Use paths appropriate to your configuration. Keep the backup in a private
location. To restore, stop CLI processes using the original database and point
`database` at the backup file. You can then inspect it with `oy debug`.
