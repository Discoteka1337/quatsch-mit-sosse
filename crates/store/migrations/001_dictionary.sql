CREATE TABLE dictionary (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    term TEXT NOT NULL CHECK (length(trim(term)) > 0),
    term_key TEXT NOT NULL UNIQUE,
    replacement TEXT,
    aliases TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(aliases) AND json_type(aliases) = 'array'),
    source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'auto')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE TABLE snippets (
    trigger TEXT PRIMARY KEY NOT NULL CHECK (length(trim(trigger)) > 0),
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

INSERT INTO dictionary (term, term_key, aliases) VALUES
    ('Codex', 'codex', '["kodex"]'),
    ('Claude', 'claude', '[]'),
    ('ChatGPT', 'chatgpt', '["Chat GPT", "Chat G P T", "Chat Ge Pe Te"]'),
    ('Wispr Flow', 'wispr flow', '["Whisper Flow"]'),
    ('Denis', 'denis', '[]'),
    ('Anton-Mall-Stadion', 'anton-mall-stadion', '["Anton Mall Stadion"]'),
    ('BGV', 'bgv', '["B G V", "Be Ge Vau"]'),
    ('Lia', 'lia', '[]'),
    ('Saatci', 'saatci', '[]'),
    ('Baar', 'baar', '[]');
