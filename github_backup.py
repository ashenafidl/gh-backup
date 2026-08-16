"""
Backs up all repositories accessible to a GitHub account, grouping them
into folders by owner (organization or user).

Prompts for your GitHub username and a personal access token at runtime
(the token input is hidden). Requires `git` to be installed and on PATH.
"""

import getpass
import json
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

API_URL = "https://api.github.com/user/repos"
PER_PAGE = 100


def prompt_credentials() -> tuple[str, str]:
    username = input("GitHub username: ").strip()
    if not username:
        sys.exit("Error: username cannot be empty.")

    token = getpass.getpass("GitHub personal access token: ").strip()
    if not token:
        sys.exit("Error: token cannot be empty.")

    return username, token


def fetch_page(username: str, token: str, page: int) -> list[dict]:
    url = f"{API_URL}?per_page={PER_PAGE}&page={page}"
    request = urllib.request.Request(url)
    request.add_header("Accept", "application/vnd.github+json")
    request.add_header("X-GitHub-Api-Version", "2022-11-28")

    credentials = f"{username}:{token}"
    import base64

    encoded = base64.b64encode(credentials.encode()).decode()
    request.add_header("Authorization", f"Basic {encoded}")

    try:
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read().decode())
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")
        sys.exit(f"Error fetching page {page}: HTTP {e.code}\n{body}")
    except urllib.error.URLError as e:
        sys.exit(f"Error fetching page {page}: {e.reason}")


def clone_repo(clone_url: str, dest_dir: Path) -> None:
    if (dest_dir / ".git").is_dir():
        print(f"Skipping existing repo: {dest_dir}")
        return

    print(f"Cloning {clone_url} into {dest_dir}")
    dest_dir.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["git", "clone", "--depth", "1", "--single-branch", clone_url, str(dest_dir)],
        check=True,
    )


def main() -> None:
    username, token = prompt_credentials()

    page = 1
    try:
        while True:
            print(f"Fetching page {page}...")
            repos = fetch_page(username, token, page)

            if not repos:
                print("No more repositories found.")
                break

            for repo in repos:
                clone_url = repo.get("clone_url")
                owner = repo.get("owner", {}).get("login")
                if not clone_url or not owner:
                    continue

                repo_name = clone_url.rstrip("/").split("/")[-1]
                repo_name = repo_name.removesuffix(".git")

                dest_dir = Path(owner) / repo_name
                clone_repo(clone_url, dest_dir)

            page += 1
    except KeyboardInterrupt:
        sys.exit("\nInterrupted.")
    except subprocess.CalledProcessError as e:
        sys.exit(f"git clone failed: {e}")


if __name__ == "__main__":
    main()
