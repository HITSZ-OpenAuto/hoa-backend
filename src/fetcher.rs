//! GitHub repository data fetcher module.
//!
//! This module provides functionality to fetch README.md and worktree.json files
//! from GitHub repositories, replacing the Python-based fetching logic.

use crate::error::{FumaError, Result};
use base64::prelude::*;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use std::path::Path;
use tokio::fs;

/// GitHub API response for file content
#[derive(Debug, Deserialize)]
struct GitHubContent {
    content: String,
    encoding: String,
}

/// GitHub API client for fetching repository data
pub struct GitHubFetcher {
    client: reqwest::Client,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RepoFetchStatus {
    has_readme: bool,
    has_worktree: bool,
}

impl GitHubFetcher {
    /// Create a new GitHub fetcher with authentication token
    pub fn new(token: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("fuma-rs"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );

        let auth_value = format!("Bearer {}", token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value).map_err(|e| {
                FumaError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
            })?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| FumaError::Io(std::io::Error::other(e)))?;

        Ok(Self { client })
    }

    /// Fetch a file from GitHub repository
    async fn fetch_file(
        &self,
        org: &str,
        repo: &str,
        path: &str,
        branch: Option<&str>,
    ) -> Result<String> {
        let mut url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}",
            org, repo, path
        );

        if let Some(ref_name) = branch {
            url.push_str(&format!("?ref={}", ref_name));
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| FumaError::Io(std::io::Error::other(e)))?;

        if !response.status().is_success() {
            return Err(FumaError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("GitHub API returned status: {}", response.status()),
            )));
        }

        let content: GitHubContent = response
            .json()
            .await
            .map_err(|e| FumaError::Io(std::io::Error::other(e)))?;

        // Decode base64 content
        if content.encoding == "base64" {
            let decoded = BASE64_STANDARD
                .decode(content.content.replace('\n', ""))
                .map_err(|e| {
                    FumaError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                })?;

            String::from_utf8(decoded)
                .map_err(|e| FumaError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
        } else {
            Ok(content.content)
        }
    }

    /// Fetch README.md for a repository
    pub async fn fetch_readme(&self, org: &str, repo: &str) -> Result<String> {
        self.fetch_file(org, repo, "README.md", None).await
    }

    /// Fetch worktree.json from worktree branch
    pub async fn fetch_worktree_json(&self, org: &str, repo: &str) -> Result<String> {
        self.fetch_file(org, repo, "worktree.json", Some("worktree"))
            .await
    }

    /// Fetch repository data and save to local files
    async fn fetch_repo_data(
        &self,
        org: &str,
        repo: &str,
        repos_dir: &Path,
    ) -> Result<RepoFetchStatus> {
        let mdx_path = repos_dir.join(format!("{}.mdx", repo));
        let json_path = repos_dir.join(format!("{}.json", repo));
        let mut status = RepoFetchStatus::default();

        // Fetch README if not exists
        if mdx_path.exists() {
            status.has_readme = true;
        } else {
            match self.fetch_readme(org, repo).await {
                Ok(content) => {
                    fs::write(&mdx_path, content).await?;
                    status.has_readme = true;
                }
                Err(e) => {
                    eprintln!("Warning: Failed to fetch README for {}: {}", repo, e);
                }
            }
        }

        // Fetch worktree.json if not exists
        if json_path.exists() {
            status.has_worktree = true;
        } else {
            match self.fetch_worktree_json(org, repo).await {
                Ok(content) => {
                    fs::write(&json_path, content).await?;
                    status.has_worktree = true;
                }
                Err(e) => {
                    eprintln!("Warning: Failed to fetch worktree.json for {}: {}", repo, e);
                }
            }
        }

        Ok(status)
    }
}

/// Fetch all repositories concurrently with semaphore limiting
pub async fn fetch_all_repos(
    token: String,
    org: &str,
    repos_list: &[String],
    repos_dir: &Path,
    concurrency: usize,
) -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    println!("Fetching {} repositories from GitHub...", repos_list.len());

    // Create repos directory if not exists
    if !repos_dir.exists() {
        fs::create_dir_all(repos_dir).await?;
    }

    let fetcher = Arc::new(GitHubFetcher::new(token)?);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    // Create tasks for all repos
    let tasks: Vec<_> = repos_list
        .iter()
        .map(|repo| {
            let fetcher = Arc::clone(&fetcher);
            let semaphore = Arc::clone(&semaphore);
            let org = org.to_string();
            let repo = repo.clone();
            let repos_dir = repos_dir.to_path_buf();

            tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                fetcher.fetch_repo_data(&org, &repo, &repos_dir).await
            })
        })
        .collect();

    // Wait for all tasks to complete
    let results = futures::future::join_all(tasks).await;

    // Count successes and failures
    let mut ready_count = 0;
    let mut missing_readme_count = 0;
    let mut missing_worktree_count = 0;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok(Ok(status)) => {
                if status.has_readme {
                    ready_count += 1;
                } else {
                    missing_readme_count += 1;
                }

                if !status.has_worktree {
                    missing_worktree_count += 1;
                }
            }
            Ok(Err(e)) => {
                error_count += 1;
                eprintln!("Error: {}", e);
            }
            Err(e) => {
                error_count += 1;
                eprintln!("Task error: {}", e);
            }
        }
    }

    println!(
        "Fetch complete: {} with README, {} missing README, {} missing worktree, {} failed",
        ready_count, missing_readme_count, missing_worktree_count, error_count
    );

    Ok(())
}

/// Resolve GitHub token from environment variables
pub fn resolve_github_token() -> Option<String> {
    // Priority order:
    // 1. PERSONAL_ACCESS_TOKEN (explicit)
    // 2. GITHUB_TOKEN (common in GitHub Actions)
    // 3. gh CLI token (local development)

    if let Ok(token) = std::env::var("PERSONAL_ACCESS_TOKEN") {
        return Some(token);
    }

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        return Some(token);
    }

    // Try to get token from gh CLI
    std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}
