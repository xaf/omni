-- Close open entries for a workdir that no longer exists
-- ?1: workdir_id
UPDATE env_history
SET used_until_date = strftime('%Y-%m-%dT%H:%M:%f', 'now')
WHERE workdir_id = ?1
AND used_until_date IS NULL;
