-- Insert or update the mise plugin versions
-- :param ?1 - plugin name
-- :param ?2 - JSON array of plugin versions
INSERT INTO mise_plugins (
    plugin_name,
    updated_at,
    versions,
    versions_fetched_at
)
VALUES (
    ?1,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    ?2,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
)
ON CONFLICT(plugin_name) DO UPDATE SET
    versions = ?2,
    versions_fetched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE plugin_name = ?1;
