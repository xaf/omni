-- Upgrade from version 4 to version 5
BEGIN TRANSACTION;

-- Add last_seen_at column to env_history table
-- This is used to track when we last saw this workdir being used (during omni up)
-- Initialize with used_from_date for existing entries
ALTER TABLE env_history ADD COLUMN last_seen_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00.000Z';

-- Set last_seen_at to used_from_date for all existing entries
UPDATE env_history SET last_seen_at = used_from_date;

-- Update the user_version to 5
PRAGMA user_version = 5;

-- Commit the transaction
COMMIT;
