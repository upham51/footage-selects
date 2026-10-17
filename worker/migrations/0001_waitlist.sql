-- Waitlist signups from the landing page. Only the Worker can reach this
-- database (through its D1 binding); there is no public access to it.
-- Safe to run more than once.
CREATE TABLE IF NOT EXISTS waitlist (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  email TEXT NOT NULL CHECK (length(email) BETWEEN 3 AND 254 AND email LIKE '%_@_%._%'),
  email_normalized TEXT GENERATED ALWAYS AS (lower(trim(email))) STORED,
  editor TEXT NOT NULL CHECK (editor IN ('resolve', 'premiere', 'final_cut', 'other', 'none')),
  shoots TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(shoots) AND json_type(shoots) = 'array'),
  shoots_other TEXT CHECK (shoots_other IS NULL OR length(shoots_other) <= 100),
  hours_per_month TEXT NOT NULL CHECK (hours_per_month IN ('under_2', '2_to_5', '5_to_15', '15_to_40', 'over_40'))
);
CREATE UNIQUE INDEX IF NOT EXISTS waitlist_email_normalized ON waitlist (email_normalized);
CREATE INDEX IF NOT EXISTS waitlist_created_at ON waitlist (created_at);
