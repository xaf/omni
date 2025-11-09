-- Get distinct workdir_ids for stale open entries that need existence check
-- ?1: retention_stale (seconds)
-- Check against the most recent of used_from_date or last_seen_at
SELECT DISTINCT workdir_id
FROM env_history
WHERE used_until_date IS NULL
AND CAST(strftime('%s', MAX(used_from_date, last_seen_at)) AS INTEGER) < CAST(strftime('%s', 'now') AS INTEGER) - ?1
ORDER BY workdir_id;
