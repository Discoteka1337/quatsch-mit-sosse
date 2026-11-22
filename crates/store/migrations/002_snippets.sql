ALTER TABLE snippets RENAME TO snippets_v1;

CREATE TABLE snippets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trigger TEXT NOT NULL CHECK (length(trim(trigger)) > 0),
    trigger_key TEXT NOT NULL,
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Version 1 allowed different spellings of the same spoken trigger. Preserve
-- every existing template. New writes check this index within a transaction.
CREATE INDEX snippets_trigger_key ON snippets (trigger_key);
INSERT INTO snippets (id, trigger, trigger_key, text, created_at)
    SELECT rowid, trigger, qms_trigger_key(trigger), text, created_at FROM snippets_v1;
DROP TABLE snippets_v1;

WITH defaults(trigger, text) AS (VALUES
    ('meine E-Mail-Adresse', ''),
    ('Gedanken ordnen Prompt', 'Ordne diese unstrukturierten Gedanken zu einer klaren, gut lesbaren Fassung, ohne Inhalte hinzuzufügen oder wegzulassen.')
)
INSERT INTO snippets (trigger, trigger_key, text)
    SELECT trigger, qms_trigger_key(trigger), text FROM defaults
    WHERE NOT EXISTS (SELECT 1 FROM snippets WHERE trigger_key = qms_trigger_key(defaults.trigger));
