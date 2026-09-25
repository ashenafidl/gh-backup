use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, exit},
};

use serde_json::Value;

const API_URL: &str = "https://api.github.com/user/repos";
const PER_PAGE: u32 = 100;
const REPOS_DIR: &str = "repos";
const ARCHIVE_NAME: &str = "repos.tar.gz";

fn prompt_credentials() -> (String, String) {
    print!("GitHub username: ");
    io::stdout().flush().ok();

    let mut username = String::new();
    io::stdin()
        .read_line(&mut username)
        .expect("Failed to read username");
    let username = username.trim().to_string();

    if username.is_empty() {
        eprintln!("Error: username cannot be empty.");
        exit(1);
    }

    let token =
        rpassword::prompt_password("GitHub personal access token: ").expect("Failed to read token");
    let token = token.trim().to_string();
    if token.is_empty() {
        eprintln!("Error: token cannot be empty.");
        exit(1);
    }

    (username, token)
}

/// Percent-encodes a string for safe use inside a URL's userinfo component
/// (i.e. the `user:pass` part before the `@`).
fn percent_encode_userinfo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Fetches one page of repos by shelling out to `curl`, so we don't need a
/// heavyweight HTTP/TLS crate as a dependency. Returns the raw JSON values
/// so the full metadata GitHub sends back can be preserved.
fn fetch_page(username: &str, token: &str, page: u32) -> Vec<Value> {
    let url = format!("{API_URL}?per_page={PER_PAGE}&page={page}");
    let userpass = format!("{username}:{token}");

    let output = Command::new("curl")
        .args([
            "-fsS",
            "-u",
            &userpass,
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2026-03-10",
            &url,
        ])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to run curl: {e}");
            exit(1);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Error fetching page {page}: {stderr}");
        exit(1);
    }

    match serde_json::from_slice::<Vec<Value>>(&output.stdout) {
        Ok(repos) => repos,
        Err(e) => {
            eprintln!("Error parsing response for page {page}: {e}");
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
            exit(1);
        }
    }
}

/// Builds an HTTPS clone URL with the username/token embedded, so `git`
/// never has to prompt for credentials interactively.
fn authenticated_clone_url(clone_url: &str, username: &str, token: &str) -> String {
    let rest = clone_url
        .strip_prefix("https://")
        .unwrap_or(clone_url.trim_start_matches("http://"));
    let user_enc = percent_encode_userinfo(username);
    let token_enc = percent_encode_userinfo(token);
    format!("https://{user_enc}:{token_enc}@{rest}")
}

/// Bare `--mirror` clone: every branch, every tag, every ref, and the full
/// commit history. Existing mirrors are refreshed with `remote update`
/// rather than re-cloned from scratch.
fn clone_or_update_mirror(auth_url: &str, plain_url: &str, dest_dir: &Path) {
    let already_exists = dest_dir.join("HEAD").is_file();

    if already_exists {
        println!("Updating existing mirror: {}", dest_dir.display());
        let status = Command::new("git")
            .args([
                "--git-dir",
                dest_dir.to_str().expect("non-UTF-8 path"),
                "remote",
                "update",
            ])
            .status();

        match status {
            Ok(s) if s.success() => return,
            Ok(s) => {
                eprintln!("git remote update failed with status: {s}, will re-clone");
            }
            Err(e) => {
                eprintln!("Failed to run git remote update: {e}, will re-clone");
            }
        }
    }

    println!("Mirror-cloning {plain_url} into {}", dest_dir.display());

    if let Some(parent) = dest_dir.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Failed to create directory {}: {e}", parent.display());
            exit(1);
        }
    }

    let status = Command::new("git")
        .args([
            "clone",
            "--mirror",
            auth_url,
            dest_dir.to_str().expect("non-UTF-8 path"),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("git clone --mirror failed with status: {s}");
            exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run git clone --mirror: {e}");
            exit(1);
        }
    }
}

/// Writes the full GitHub API JSON for a repo to a sibling `.json` file so
/// metadata (description, stars, topics, visibility, etc.) is preserved
/// alongside the mirrored git data.
fn write_metadata(repo: &Value, metadata_path: &Path) {
    let pretty = serde_json::to_string_pretty(repo).unwrap_or_else(|_| repo.to_string());
    if let Err(e) = fs::write(metadata_path, pretty) {
        eprintln!(
            "Failed to write metadata to {}: {e}",
            metadata_path.display()
        );
    }
}

/// Compresses the repos directory into a tar.gz archive next to it.
fn compress_repos_dir() {
    println!("Compressing {REPOS_DIR}/ into {ARCHIVE_NAME}...");

    let status = Command::new("tar")
        .args(["-czf", ARCHIVE_NAME, REPOS_DIR])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("Done. Archive written to {ARCHIVE_NAME}");
        }
        Ok(s) => {
            eprintln!("tar failed with status: {s}");
            exit(1);
        }
        Err(e) => {
            eprintln!("Failed to run tar: {e}");
            exit(1);
        }
    }
}

fn main() {
    let (username, token) = prompt_credentials();

    let mut page: u32 = 1;

    loop {
        println!("Fetching page {page}...");
        let repos = fetch_page(&username, &token, page);

        if repos.is_empty() {
            println!("No more repositories found.");
            break;
        }

        for repo in &repos {
            let clone_url = match repo.get("clone_url").and_then(Value::as_str) {
                Some(u) => u.to_string(),
                None => continue,
            };
            let owner_login = match repo
                .get("owner")
                .and_then(|o| o.get("login"))
                .and_then(Value::as_str)
            {
                Some(l) => l.to_string(),
                None => continue,
            };

            let repo_name = clone_url
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".git");

            if repo_name.is_empty() {
                continue;
            }

            let owner_dir = Path::new(REPOS_DIR).join(&owner_login);
            let dest_dir: PathBuf = owner_dir.join(format!("{repo_name}.git"));
            let metadata_path = owner_dir.join(format!("{repo_name}.json"));

            let auth_url = authenticated_clone_url(&clone_url, &username, &token);
            clone_or_update_mirror(&auth_url, &clone_url, &dest_dir);

            if let Err(e) = fs::create_dir_all(&owner_dir) {
                eprintln!("Failed to create directory {}: {e}", owner_dir.display());
                exit(1);
            }
            write_metadata(repo, &metadata_path);
        }

        page += 1;
    }

    if Path::new(REPOS_DIR).is_dir() {
        compress_repos_dir();
    } else {
        println!("No repos were cloned; skipping compression.");
    }
}
