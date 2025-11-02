-- Upgrade from version 3 to version 4
BEGIN TRANSACTION;

-- Add columns to github_release_installed table to track release properties
-- These are used to check if an installed version matches filter requirements

-- Track whether the installed release was a prerelease
ALTER TABLE github_release_installed ADD COLUMN prerelease BOOLEAN NOT NULL DEFAULT 0;

-- Track whether the installed release was marked as immutable by GitHub
ALTER TABLE github_release_installed ADD COLUMN immutable BOOLEAN NOT NULL DEFAULT 0;

-- Update the user_version to 4
PRAGMA user_version = 4;

-- Commit the transaction
COMMIT;
