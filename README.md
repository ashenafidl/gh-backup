# gh-backup

Backs up all repositories accessible to a GitHub account, grouping them into folders by owner (organization or user) under a common `repos/` directory, then compresses that directory into `repos.tar.gz`.

Each repo is cloned as a `--mirror` (bare) clone, which pulls every branch, tag, and the complete commit history -- not just the default branch. The full GitHub API metadata for each repo (description, stars, topics, visibility, etc.) is also saved as a sibling `.json` file, since that information lives outside the git data itself.

Prompts for your GitHub username and a personal access token at runtime (the token input is hidden). Requires `git`, `curl`, and `tar` to be installed and on PATH.
