-- Add migration script here

CREATE INDEX IF NOT EXISTS idx_query_timestamp ON historial(query, timestamp);
