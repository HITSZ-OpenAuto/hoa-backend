/// Semester mapping from Chinese names to folder names and display titles
pub const SEMESTER_MAPPING: &[(&str, &str, &str)] = &[
    ("第一学年秋季", "fresh-autumn", "大一·秋"),
    ("第一学年春季", "fresh-spring", "大一·春"),
    ("第二学年秋季", "sophomore-autumn", "大二·秋"),
    ("第二学年春季", "sophomore-spring", "大二·春"),
    ("第三学年秋季", "junior-autumn", "大三·秋"),
    ("第三学年春季", "junior-spring", "大三·春"),
    ("第四学年秋季", "senior-autumn", "大四·秋"),
    ("第四学年春季", "senior-spring", "大四·春"),
];

/// Get semester folder and title from Chinese semester name
pub fn get_semester_folder(recommended: &str) -> Option<(&'static str, &'static str)> {
    SEMESTER_MAPPING
        .iter()
        .find(|&&(key, _, _)| key == recommended)
        .map(|&(_, folder, title)| (folder, title))
}

// ============================================================================
// File Exclusion Rules
// ============================================================================

/// Files to exclude from the file tree
pub const EXCLUDED_PATTERNS: &[&str] = &[".gitkeep", "README.md", "LICENSE", "tag.txt"];

/// File extensions to exclude
pub const EXCLUDED_EXTENSIONS: &[&str] = &[".toml"];

/// Directory prefixes to exclude
pub const EXCLUDED_PREFIXES: &[&str] = &[".github/"];

/// Check if a file path should be included in the file tree
pub fn should_include_file(path: &str) -> bool {
    let filename = path.split('/').last().unwrap_or("");

    // Check exact matches
    if EXCLUDED_PATTERNS.contains(&filename) {
        return false;
    }

    // Check extensions
    if EXCLUDED_EXTENSIONS
        .iter()
        .any(|ext| filename.ends_with(ext))
    {
        return false;
    }

    // Check prefixes
    if EXCLUDED_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        return false;
    }

    true
}
