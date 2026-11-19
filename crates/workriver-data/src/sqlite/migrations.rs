// The schema of the one SQLite file, as steps applied in order. A released step is never
// edited: a change is a new step.
/// Schema steps, applied in order; `PRAGMA user_version` counts how many ran.
pub(super) const MIGRATIONS: &[&str] = &["
    CREATE TABLE events (
        seq INTEGER PRIMARY KEY,
        at TEXT NOT NULL,
        kind TEXT NOT NULL,
        actor TEXT NOT NULL,
        intent TEXT,
        unit TEXT,
        task TEXT,
        body TEXT NOT NULL CHECK (json_valid(body))
    );
    CREATE INDEX events_unit ON events(unit, seq);
    CREATE TRIGGER events_no_update BEFORE UPDATE ON events
        BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
    CREATE TRIGGER events_no_delete BEFORE DELETE ON events
        BEGIN SELECT RAISE(ABORT, 'events are append-only'); END;
"];
