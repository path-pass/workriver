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
", "
    CREATE TABLE format (events TEXT NOT NULL);
    INSERT INTO format (events) VALUES ('virtual-workspaces');
"];

/// The step that marks the form the events are written in: workspaces of workriver's own
/// (`ws1`), not the runtime's groups. It only ever runs on a store that holds no events: one
/// that has events and not this step was written in the earlier form, which this version
/// does not read (`Store::open` refuses it, and leaves it as it is).
pub(super) const FORMAT_STEP: usize = 1;
