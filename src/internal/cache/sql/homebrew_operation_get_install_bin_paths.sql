-- Get the bin path for a Homebrew formula or cask
-- :param1: The name of the Homebrew formula or cask
-- :param2: The version of the Homebrew formula or cask
-- :param3: Whether the formula or cask is a cask
-- :return: A JSON array with the paths where the binaries of the Homebrew formula or cask are installed
SELECT
    bin_paths
FROM
    homebrew_installed
WHERE
    name = ?1
    AND version = ?2
    AND cask = MIN(1, ?3);
