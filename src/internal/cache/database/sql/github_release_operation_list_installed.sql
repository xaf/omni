-- List all the installed github releases
SELECT
    repository,
    version,
    prerelease,
    immutable
FROM
    github_release_installed;
