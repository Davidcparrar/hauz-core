CREATE TABLE IF NOT EXISTS bills (
    id            TEXT    PRIMARY KEY,
    hash          BLOB    NOT NULL UNIQUE,
    vendor        TEXT,
    amount_minor  INTEGER,
    currency      TEXT,
    period_start  DATE,
    period_end    DATE,
    due           DATE,
    status        TEXT    NOT NULL,
    inserted_at   TEXT    NOT NULL
);
