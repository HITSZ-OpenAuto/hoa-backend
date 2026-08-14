//! Data loading utilities for training plans and course information.
//!
//! This module provides functions to load all training plan data from TOML files
//! and enrich it with grade details from grades_summary.json. By loading all data
//! upfront, we avoid the N+1 query problem that plagued the Python implementation.

use crate::error::{FumaError, Result};
use crate::models::{Course, GradeDetail, Plan, SharedCategory, TomlPlan};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
struct TomlSharedCategories {
    categories: Vec<TomlSharedCategory>,
    #[serde(default)]
    no_course_info_repo_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TomlSharedCategory {
    id: String,
    title: String,
    repo_ids: Vec<String>,
    #[serde(default)]
    study_levels: Vec<String>,
}

/// Grades summary data structure mapping repository IDs to grade details per plan variant
pub type GradesSummary = HashMap<String, HashMap<String, Vec<GradeDetail>>>;
/// Lookup table mapping course code to repo ID with optional plan-specific overrides
type LookupTable = HashMap<String, HashMap<String, String>>;

/// Load grades_summary.json if present.
///
/// Returns an empty HashMap if the file doesn't exist or can't be parsed.
pub fn load_grades_summary(data_dir: &Path) -> GradesSummary {
    let path = data_dir.join("grades_summary.json");

    if !path.exists() {
        return HashMap::new();
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|_| HashMap::new()),
        Err(_) => HashMap::new(),
    }
}

/// Load lookup_table.toml if present.
///
/// Returns an empty HashMap if the file doesn't exist or can't be parsed.
fn load_lookup_table(data_dir: &Path) -> LookupTable {
    let path = data_dir.join("lookup_table.toml");

    if !path.exists() {
        return HashMap::new();
    }

    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|_| HashMap::new()),
        Err(_) => HashMap::new(),
    }
}

/// Resolve repository ID for a course code by lookup table rules.
///
/// Priority:
/// 1. Exact match by `plan_id`
/// 2. `DEFAULT` fallback
/// 3. Original `course_code` (identity mapping)
fn resolve_repo_id(lookup_table: &LookupTable, course_code: &str, plan_id: &str) -> String {
    lookup_table
        .get(course_code)
        .and_then(|mapping| {
            mapping
                .get(plan_id)
                .or_else(|| mapping.get("DEFAULT"))
                .or_else(|| mapping.get("default"))
        })
        .map(|repo_id| repo_id.trim())
        .filter(|repo_id| !repo_id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| course_code.to_string())
}

/// Select grade details for a course based on hierarchical matching rules.
///
/// Match priority order:
/// 1. `{year}_{major_code}` or `{year}_{major_name}` (most specific)
/// 2. `{year}_default` (year-specific default)
/// 3. `default` (global default)
///
/// Returns None if no matching grade details are found.
fn select_grade_details(
    grades_summary: &GradesSummary,
    repo_id: &str,
    year: &str,
    major_code: &str,
    major_name: &str,
) -> Option<Vec<GradeDetail>> {
    let entry = grades_summary.get(repo_id)?;

    // Try year_major keys (both code and name)
    let year_major_keys = vec![
        format!("{}_{}", year, major_code),
        format!("{}_{}", year, major_name),
    ];

    for key in &year_major_keys {
        if let Some(details) = entry.get(key) {
            if !details.is_empty() {
                return Some(details.clone());
            }
        }
    }

    // Try year_default
    let year_default_key = format!("{}_default", year);
    if let Some(details) = entry.get(&year_default_key) {
        if !details.is_empty() {
            return Some(details.clone());
        }
    }

    // Try default
    if let Some(details) = entry.get("default") {
        if !details.is_empty() {
            return Some(details.clone());
        }
    }

    None
}

/// Load all training plans from TOML files with grade details enrichment.
///
/// This function loads all plan data in a single pass, avoiding the N+1 query problem
/// that occurred in the Python implementation where each course required a separate
/// CLI invocation to fetch grade details.
///
/// # Arguments
/// * `data_dir` - Path to the hoa-majors data directory containing plans/ subdirectory
///
/// # Returns
/// * `Ok(Vec<Plan>)` - All loaded and enriched training plans
/// * `Err(FumaError)` - If the plans directory is missing or files can't be read
pub fn load_all_plans(data_dir: &Path) -> Result<Vec<Plan>> {
    let plans_dir = data_dir.join("plans");

    if !plans_dir.exists() {
        return Err(FumaError::MissingDirectory(plans_dir));
    }

    // Load grades summary once for all plans
    let grades_summary = load_grades_summary(data_dir);
    // Load course_code -> repo_id lookup table once for all plans
    let lookup_table = load_lookup_table(data_dir);

    let mut plans = Vec::new();

    for entry in WalkDir::new(&plans_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
    {
        let content = fs::read_to_string(entry.path())?;
        let toml_plan: TomlPlan = toml::from_str(&content)?;

        // Enrich courses with grade_details from grades_summary.json
        let courses: Vec<Course> = toml_plan
            .courses
            .into_iter()
            .map(|c| {
                let repo_id =
                    resolve_repo_id(&lookup_table, &c.course_code, &toml_plan.info.plan_id);

                // Select grade details if not already in TOML.
                // NOTE: We look up grades_summary by repository ID, not by course_code.
                let grade_details = c.grade_details.or_else(|| {
                    select_grade_details(
                        &grades_summary,
                        &repo_id,
                        &toml_plan.info.year,
                        &toml_plan.info.major_code,
                        &toml_plan.info.major_name,
                    )
                });

                Course {
                    repo_id,
                    name: c.course_name,
                    credit: c.credit,
                    assessment_method: c.assessment_method,
                    course_nature: c.course_nature,
                    recommended_semester: c.recommended_year_semester,
                    hours: c.hours,
                    grade_details,
                }
            })
            .collect();

        if courses.is_empty() {
            eprintln!(
                "Warning: skipping empty plan {} {} ({})",
                toml_plan.info.year, toml_plan.info.major_name, toml_plan.info.major_code
            );
            continue;
        }

        plans.push(Plan {
            year: toml_plan.info.year,
            major_code: toml_plan.info.major_code,
            major_name: toml_plan.info.major_name,
            study_level: toml_plan.info.study_level,
            courses,
        });
    }

    // Sort plans by year and major_code for deterministic processing
    plans.sort_by(|a, b| a.year.cmp(&b.year).then(a.major_code.cmp(&b.major_code)));

    Ok(plans)
}

/// Config for shared categories and which repo IDs are index pages (no CourseInfo).
pub struct SharedCategoriesConfig {
    pub categories: Vec<SharedCategory>,
    pub no_course_info_repo_ids: HashSet<String>,
}

/// Load shared_categories.toml if present.
///
/// Returns default (empty categories, empty no_course_info set) if file doesn't exist or can't be parsed.
pub fn load_shared_categories(data_dir: &Path) -> SharedCategoriesConfig {
    let path = data_dir.join("shared_categories.toml");

    if !path.exists() {
        return SharedCategoriesConfig {
            categories: Vec::new(),
            no_course_info_repo_ids: HashSet::new(),
        };
    }

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return SharedCategoriesConfig {
                categories: Vec::new(),
                no_course_info_repo_ids: HashSet::new(),
            };
        }
    };

    let toml: TomlSharedCategories = match toml::from_str(&content) {
        Ok(t) => t,
        Err(_) => {
            return SharedCategoriesConfig {
                categories: Vec::new(),
                no_course_info_repo_ids: HashSet::new(),
            };
        }
    };

    SharedCategoriesConfig {
        categories: toml
            .categories
            .into_iter()
            .map(|c| SharedCategory {
                id: c.id,
                title: c.title,
                repo_ids: c.repo_ids,
                study_levels: c.study_levels,
            })
            .collect(),
        no_course_info_repo_ids: toml.no_course_info_repo_ids.into_iter().collect(),
    }
}

/// Load repos_list.txt to filter available courses.
///
/// # Returns
/// * Empty HashSet if repos_list.txt doesn't exist (process all courses)
/// * HashSet of repository codes if the file exists
pub fn load_repos_list(repo_root: &Path) -> Result<HashSet<String>> {
    let path = repo_root.join("repos_list.txt");

    if !path.exists() {
        eprintln!("Warning: repos_list.txt not found, will process all available courses");
        return Ok(HashSet::new());
    }

    let content = fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_grade_detail(name: &str, percent: &str) -> GradeDetail {
        GradeDetail {
            name: name.to_string(),
            percent: Some(percent.to_string()),
        }
    }

    #[test]
    fn test_select_grade_details_year_major_code() {
        let mut grades_summary = HashMap::new();
        let mut course_entry = HashMap::new();

        course_entry.insert(
            "2023_CS".to_string(),
            vec![create_test_grade_detail("Exam", "70%")],
        );
        course_entry.insert(
            "2023_default".to_string(),
            vec![create_test_grade_detail("Exam", "60%")],
        );
        course_entry.insert(
            "default".to_string(),
            vec![create_test_grade_detail("Exam", "50%")],
        );

        grades_summary.insert("MATH101".to_string(), course_entry);

        let result =
            select_grade_details(&grades_summary, "MATH101", "2023", "CS", "Computer Science");

        assert!(result.is_some());
        let details = result.unwrap();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].name, "Exam");
        assert_eq!(details[0].percent, Some("70%".to_string()));
    }

    #[test]
    fn test_select_grade_details_year_major_name() {
        let mut grades_summary = HashMap::new();
        let mut course_entry = HashMap::new();

        course_entry.insert(
            "2023_Computer Science".to_string(),
            vec![create_test_grade_detail("Project", "80%")],
        );
        course_entry.insert(
            "default".to_string(),
            vec![create_test_grade_detail("Exam", "50%")],
        );

        grades_summary.insert("PROG202".to_string(), course_entry);

        let result =
            select_grade_details(&grades_summary, "PROG202", "2023", "CS", "Computer Science");

        assert!(result.is_some());
        let details = result.unwrap();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].name, "Project");
        assert_eq!(details[0].percent, Some("80%".to_string()));
    }

    #[test]
    fn test_select_grade_details_year_default() {
        let mut grades_summary = HashMap::new();
        let mut course_entry = HashMap::new();

        course_entry.insert(
            "2023_default".to_string(),
            vec![create_test_grade_detail("Midterm", "40%")],
        );
        course_entry.insert(
            "default".to_string(),
            vec![create_test_grade_detail("Exam", "50%")],
        );

        grades_summary.insert("PHYS101".to_string(), course_entry);

        let result = select_grade_details(
            &grades_summary,
            "PHYS101",
            "2023",
            "EE",
            "Electrical Engineering",
        );

        assert!(result.is_some());
        let details = result.unwrap();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].name, "Midterm");
    }

    #[test]
    fn test_select_grade_details_global_default() {
        let mut grades_summary = HashMap::new();
        let mut course_entry = HashMap::new();

        course_entry.insert(
            "default".to_string(),
            vec![create_test_grade_detail("Final", "100%")],
        );

        grades_summary.insert("CHEM101".to_string(), course_entry);

        let result = select_grade_details(
            &grades_summary,
            "CHEM101",
            "2024",
            "ME",
            "Mechanical Engineering",
        );

        assert!(result.is_some());
        let details = result.unwrap();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].name, "Final");
    }

    #[test]
    fn test_select_grade_details_not_found() {
        let grades_summary = HashMap::new();

        let result =
            select_grade_details(&grades_summary, "UNKNOWN", "2023", "CS", "Computer Science");

        assert!(result.is_none());
    }

    #[test]
    fn test_select_grade_details_empty_details() {
        let mut grades_summary = HashMap::new();
        let mut course_entry = HashMap::new();

        course_entry.insert("2023_CS".to_string(), vec![]);
        course_entry.insert(
            "default".to_string(),
            vec![create_test_grade_detail("Backup", "100%")],
        );

        grades_summary.insert("TEST101".to_string(), course_entry);

        let result =
            select_grade_details(&grades_summary, "TEST101", "2023", "CS", "Computer Science");

        // Should fallback to default since 2023_CS is empty
        assert!(result.is_some());
        let details = result.unwrap();
        assert_eq!(details[0].name, "Backup");
    }

    #[test]
    fn test_resolve_repo_id_plan_specific() {
        let mut lookup_table = HashMap::new();
        let mut mapping = HashMap::new();
        mapping.insert("PLAN_A".to_string(), "REPO_A".to_string());
        mapping.insert("DEFAULT".to_string(), "REPO_DEFAULT".to_string());
        lookup_table.insert("COURSE1".to_string(), mapping);

        let repo_id = resolve_repo_id(&lookup_table, "COURSE1", "PLAN_A");
        assert_eq!(repo_id, "REPO_A");
    }

    #[test]
    fn test_resolve_repo_id_default_fallback() {
        let mut lookup_table = HashMap::new();
        let mut mapping = HashMap::new();
        mapping.insert("DEFAULT".to_string(), "REPO_DEFAULT".to_string());
        lookup_table.insert("COURSE1".to_string(), mapping);

        let repo_id = resolve_repo_id(&lookup_table, "COURSE1", "PLAN_B");
        assert_eq!(repo_id, "REPO_DEFAULT");
    }

    #[test]
    fn test_resolve_repo_id_identity_fallback() {
        let lookup_table: LookupTable = HashMap::new();

        let repo_id = resolve_repo_id(&lookup_table, "COURSE1", "PLAN_A");
        assert_eq!(repo_id, "COURSE1");
    }

    #[test]
    fn test_load_repos_list_nonexistent() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_repos_list_nonexistent");
        let _ = std::fs::create_dir_all(&temp_dir);

        let result = load_repos_list(&temp_dir).unwrap();
        assert!(result.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_repos_list_with_content() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_repos_list_with_content");
        let _ = std::fs::create_dir_all(&temp_dir);
        let repos_file = temp_dir.join("repos_list.txt");

        let mut file = fs::File::create(&repos_file).unwrap();
        writeln!(file, "MATH101").unwrap();
        writeln!(file, "PHYS201").unwrap();
        writeln!(file, "  CHEM301  ").unwrap(); // with whitespace
        writeln!(file, "").unwrap(); // empty line
        writeln!(file, "CS401").unwrap();

        let result = load_repos_list(&temp_dir).unwrap();

        assert_eq!(result.len(), 4);
        assert!(result.contains("MATH101"));
        assert!(result.contains("PHYS201"));
        assert!(result.contains("CHEM301"));
        assert!(result.contains("CS401"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_grades_summary_missing_file() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_grades_missing");
        let _ = std::fs::create_dir_all(&temp_dir);

        let result = load_grades_summary(&temp_dir);
        assert!(result.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_grades_summary_valid_file() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_grades_valid");
        let _ = std::fs::create_dir_all(&temp_dir);
        let grades_file = temp_dir.join("grades_summary.json");

        let grades_data = serde_json::json!({
            "MATH101": {
                "2023_CS": [
                    {"name": "Exam", "percent": "70%"}
                ],
                "default": [
                    {"name": "Exam", "percent": "60%"}
                ]
            }
        });

        fs::write(&grades_file, grades_data.to_string()).unwrap();

        let result = load_grades_summary(&temp_dir);

        assert_eq!(result.len(), 1);
        assert!(result.contains_key("MATH101"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_grades_summary_invalid_json() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_grades_invalid");
        let _ = std::fs::create_dir_all(&temp_dir);
        let grades_file = temp_dir.join("grades_summary.json");

        fs::write(&grades_file, "invalid json{{{").unwrap();

        let result = load_grades_summary(&temp_dir);

        // Should return empty HashMap on parse error
        assert!(result.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_lookup_table_missing_file() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_lookup_missing");
        let _ = std::fs::create_dir_all(&temp_dir);

        let result = load_lookup_table(&temp_dir);
        assert!(result.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_lookup_table_valid_file() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_lookup_valid");
        let _ = std::fs::create_dir_all(&temp_dir);
        let lookup_file = temp_dir.join("lookup_table.toml");

        fs::write(
            &lookup_file,
            r#"
[COURSE1]
DEFAULT = "REPO1"

[COURSE2]
PLAN_A = "REPO2A"
"#,
        )
        .unwrap();

        let result = load_lookup_table(&temp_dir);

        assert_eq!(
            result.get("COURSE1").and_then(|m| m.get("DEFAULT")),
            Some(&"REPO1".to_string())
        );
        assert_eq!(
            result.get("COURSE2").and_then(|m| m.get("PLAN_A")),
            Some(&"REPO2A".to_string())
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_lookup_table_invalid_toml() {
        use std::env;
        let temp_dir = env::temp_dir().join("test_lookup_invalid");
        let _ = std::fs::create_dir_all(&temp_dir);
        let lookup_file = temp_dir.join("lookup_table.toml");

        fs::write(&lookup_file, "[COURSE1\nDEFAULT = \"BROKEN\"").unwrap();

        let result = load_lookup_table(&temp_dir);
        assert!(result.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
