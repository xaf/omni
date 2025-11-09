-- Update last_seen_at for open entries of a workdir
-- ?1: workdir_id
UPDATE env_history
SET last_seen_at = strftime('%Y-%m-%dT%H:%M:%f', 'now')
WHERE workdir_id = ?1
AND used_until_date IS NULL;
