CREATE TABLE IF NOT EXISTS bills (
    id            TEXT    PRIMARY KEY,
    hash          BLOB    NOT NULL,
    vendor        TEXT,
    amount_minor  INTEGER,
    currency      TEXT,
    period_start  DATE,
    period_end    DATE,
    due           DATE,
    status        TEXT    NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS bills_hash_idx ON bills (hash);
