-- Insert or update the mise plugin last updated timestamp
-- :param ?1 - plugin name
INSERT INTO mise_plugins (
    plugin,
    updated_at
)
VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
ON CONFLICT(plugin) DO UPDATE SET
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE plugin = ?1;