-- Add a new github release
-- :param: ?1 repository - the repository name
-- :param: ?2 version - the version of the release
-- :param: ?3 prerelease - whether the release is a prerelease
-- :param: ?4 immutable - whether the release is immutable
INSERT INTO github_release_installed (
    repository,
    version,
    prerelease,
    immutable,
    last_required_at
)
VALUES (
    ?1,
    ?2,
    ?3,
    ?4,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
)
ON CONFLICT (repository, version) DO UPDATE
SET
    prerelease = ?3,
    immutable = ?4,
    last_required_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE
    repository = ?1
    AND version = ?2;
