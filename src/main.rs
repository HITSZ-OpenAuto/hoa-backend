//! Fuma - Fast Course Page Generator for HITSZ-OpenAuto
//!
//! This binary replaces the Python-based page generation system with a high-performance
//! Rust implementation that avoids the N+1 query problem by loading all data upfront.

mod constants;
mod error;
mod fetcher;
mod formatter;
mod generator;
mod loader;
mod models;
mod tree;

use error::Result;
use std::path::Path;
use std::{env, fs};

/// Main entry point for the Fuma course page generator.
///
/// This program:
/// 1. (Optional) Fetches repos data from GitHub
/// 2. Loads all training plans from TOML files (avoiding N+1 queries)
/// 3. Filters courses based on repos_list.txt
/// 4. Generates course pages with YAML frontmatter
/// 5. Builds file trees from worktree.json data
/// 6. Formats MDX files for Fumadocs compatibility
#[tokio::main]
async fn main() -> Result<()> {
    // Check for --fetch flag
    let args: Vec<String> = env::args().collect();
    let should_fetch = args.contains(&"--fetch".to_string());

    let repo_root = Path::new(".").to_path_buf();

    println!("Repository root: {}", repo_root.display());

    let repos_dir = repo_root.join("repos");

    // Fetch repos from GitHub if --fetch flag is provided
    if should_fetch {
        println!("\n=== Fetching repos from GitHub ===");

        let token = fetcher::resolve_github_token();
        if token.is_none() {
            eprintln!("Error: No GitHub token found!");
            eprintln!(
                "Please set PERSONAL_ACCESS_TOKEN, GITHUB_TOKEN, or login via `gh auth login`"
            );
            std::process::exit(1);
        }

        // Load repos list
        let repos_list_path = repo_root.join("repos_list.txt");
        if !repos_list_path.exists() {
            eprintln!("Error: repos_list.txt not found!");
            std::process::exit(1);
        }

        let repos_content = fs::read_to_string(&repos_list_path)?;
        let repos_list: Vec<String> = repos_content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        println!("Found {} repositories in repos_list.txt", repos_list.len());

        // Fetch repos (20 concurrent requests)
        fetcher::fetch_all_repos(
            token.unwrap(),
            "HITSZ-OpenAuto",
            &repos_list,
            &repos_dir,
            20,
        )
        .await?;

        println!("✓ Repos fetched successfully\n");
    }

    // Check if repos directory exists
    if !repos_dir.exists() {
        eprintln!("\nError: 'repos' directory not found!");
        eprintln!("This tool requires the repos directory to be populated first.");
        eprintln!("Please run with --fetch flag or ensure repos have been fetched.");
        eprintln!("\nExpected directory: {}", repos_dir.display());
        std::process::exit(1);
    }

    // Load repos list (optional filter)
    let repos_set = loader::load_repos_list(&repo_root)?;
    if repos_set.is_empty() {
        println!("No repos_list.txt found - will process all available courses");
    } else {
        println!(
            "Loaded {} repositories from repos_list.txt",
            repos_set.len()
        );
    }

    // Load all training plans from TOML files
    let data_dir = repo_root.join("hoa-major-data");
    let plans = loader::load_all_plans(&data_dir)?;
    println!("Loaded {} training plans", plans.len());

    // Filter courses by repos_set (if repos_list.txt exists)
    let filtered_plans: Vec<_> = if repos_set.is_empty() {
        plans
    } else {
        plans
            .into_iter()
            .map(|mut plan| {
                plan.courses.retain(|c| repos_set.contains(&c.code));
                plan
            })
            .collect()
    };

    let total_courses: usize = filtered_plans.iter().map(|p| p.courses.len()).sum();
    println!("Total courses to process: {}", total_courses);

    // Generate course pages
    let docs_dir = repo_root.join("content/docs");
    if !docs_dir.exists() {
        println!("Creating output directory: {}", docs_dir.display());
        fs::create_dir_all(&docs_dir)?;
    }

    println!("Generating course pages...");
    generator::generate_course_pages(&filtered_plans, &repos_dir, &docs_dir, &repos_set).await?;
    println!("Course pages generated successfully");

    // Format MDX files
    println!("Formatting MDX files...");
    let modified_count = formatter::format_all_mdx_files(&docs_dir)?;
    println!("Formatted {} MDX files", modified_count);

    println!("\n✓ Done! All pages generated and formatted.");

    Ok(())
}
