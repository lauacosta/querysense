-- Add migration script here

CREATE TABLE IF NOT EXISTS historial (
    id INTEGER PRIMARY KEY,
    query TEXT NOT NULL UNIQUE,
    result TEXT NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
)
