use crate::constants::{get_semester_folder, SEMESTER_MAPPING};
use crate::error::Result;
use crate::models::{
    Course, CourseMetadata, Frontmatter, GradingItem, HourDistributionMeta, Plan, WorktreeData,
};
use crate::tree::{build_file_tree, tree_to_jsx};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

// ============================================================================
// Frontmatter Generation
// ============================================================================

/// Build YAML frontmatter for a course page using serde_yaml
fn build_frontmatter(title: &str, course: &Course) -> String {
    let credit = course.credit.map(|c| c as u32).unwrap_or(0);
    let assessment_method = course
        .assessment_method
        .as_deref()
        .unwrap_or("")
        .to_string();
    let course_nature = course.course_nature.as_deref().unwrap_or("").to_string();

    let hour_distribution = if let Some(ref h) = course.hours {
        HourDistributionMeta {
            theory: h.theory.unwrap_or(0),
            lab: h.lab.unwrap_or(0),
            practice: h.practice.unwrap_or(0),
            exercise: h.exercise.unwrap_or(0),
            computer: h.computer.unwrap_or(0),
            tutoring: h.tutoring.unwrap_or(0),
        }
    } else {
        HourDistributionMeta {
            theory: 0,
            lab: 0,
            practice: 0,
            exercise: 0,
            computer: 0,
            tutoring: 0,
        }
    };

    let grading_scheme = if let Some(ref details) = course.grade_details {
        details
            .iter()
            .filter_map(|detail| {
                let percent = if let Some(ref percent_str) = detail.percent {
                    percent_str
                        .trim_end_matches('%')
                        .parse::<u32>()
                        .unwrap_or(0)
                } else {
                    0
                };

                if percent > 0 {
                    Some(GradingItem {
                        name: detail.name.clone(),
                        percent,
                    })
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let frontmatter = Frontmatter {
        title: title.to_string(),
        description: String::new(),
        course: CourseMetadata {
            credit,
            assessment_method,
            course_nature,
            hour_distribution,
            grading_scheme,
        },
    };

    frontmatter.to_yaml()
}

// ============================================================================
// Page Generation
// ============================================================================

/// Generate all course pages and index pages
pub async fn generate_course_pages(
    plans: &[Plan],
    repos_dir: &Path,
    docs_dir: &Path,
    repos_set: &HashSet<String>,
) -> Result<()> {
    let mut years: HashSet<String> = HashSet::new();
    let mut majors_by_year: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for plan in plans {
        years.insert(plan.year.clone());

        majors_by_year
            .entry(plan.year.clone())
            .or_insert_with(Vec::new)
            .push((plan.major_code.clone(), plan.major_name.clone()));

        let major_dir = docs_dir.join(&plan.year).join(&plan.major_code);
        fs::create_dir_all(&major_dir)?;

        // Write major metadata
        //
        // Fumadocs uses `meta.json` -> `pages` to control sidebar ordering. For majors that
        // contain semester subfolders, we want chronological order (大一·秋 → ... → 大四·春)
        // instead of alphabetical.
        let pages: Vec<String> = std::iter::once("...".to_string())
            .chain(SEMESTER_MAPPING.iter().map(|(_, folder, _)| (*folder).to_string()))
            .collect();

        let major_meta = serde_json::json!({
            "title": plan.major_name,
            "root": true,
            "defaultOpen": true,
            "pages": pages,
        });
        fs::write(
            major_dir.join("meta.json"),
            serde_json::to_string_pretty(&major_meta)?,
        )?;

        // Track courses by semester for this major
        let mut courses_by_semester: HashMap<String, Vec<(String, String)>> = HashMap::new();

        // Process each course
        for course in &plan.courses {
            // Only process courses that exist in repos_list (if repos_list.txt exists)
            if !repos_set.is_empty() && !repos_set.contains(&course.code) {
                continue;
            }

            let mdx_path = repos_dir.join(format!("{}.mdx", course.code));
            let json_path = repos_dir.join(format!("{}.json", course.code));

            if !mdx_path.exists() {
                continue;
            }

            // Read README content (skip first 2 lines which are title)
            let readme_content = fs::read_to_string(&mdx_path)?;
            let content_lines: Vec<&str> = readme_content.lines().skip(2).collect();
            let content = content_lines.join("\n");

            // Determine target directory based on semester
            let target_dir = if let Some(ref sem) = course.recommended_semester {
                if let Some((folder, _title)) = get_semester_folder(sem) {
                    let sem_dir = major_dir.join(folder);
                    fs::create_dir_all(&sem_dir)?;
                    courses_by_semester
                        .entry(folder.to_string())
                        .or_insert_with(Vec::new)
                        .push((course.code.clone(), course.name.clone()));
                    sem_dir
                } else {
                    major_dir.clone()
                }
            } else {
                major_dir.clone()
            };

            // Generate file tree from worktree.json
            let filetree_content = if json_path.exists() {
                let json_content = fs::read_to_string(&json_path)?;
                let worktree: WorktreeData = serde_json::from_str(&json_content)?;
                let tree = build_file_tree(&worktree, &course.code);
                let jsx = tree_to_jsx(&tree, 1);
                format!(
                    "\n\n## 资源下载\n\n<Files url=\"https://open.osa.moe/openauto/{}\">\n{}\n</Files>",
                    course.code, jsx
                )
            } else {
                String::new()
            };

            // Build frontmatter
            let frontmatter = build_frontmatter(&course.name, course);

            // Write course page
            let page_content = format!(
                "{}\n\n<CourseInfo />\n\n{}{}",
                frontmatter, content, filetree_content
            );
            fs::write(
                target_dir.join(format!("{}.mdx", course.code)),
                page_content,
            )?;
        }

        // Generate semester index pages
        for (folder, courses) in &courses_by_semester {
            let sem_dir = major_dir.join(folder);
            let sem_title = SEMESTER_MAPPING
                .iter()
                .find(|(_, f, _)| f == folder)
                .map(|(_, _, t)| *t)
                .unwrap_or(folder.as_str());

            let mut cards = vec![
                "---".to_string(),
                format!("title: {}", sem_title),
                "---".to_string(),
                "".to_string(),
                "<Cards>".to_string(),
            ];

            for (code, name) in courses {
                cards.push(format!(
                    "  <Card title=\"{}\" href=\"/docs/{}/{}/{}/{}\" />",
                    name, plan.year, plan.major_code, folder, code
                ));
            }
            cards.push("</Cards>".to_string());

            fs::write(sem_dir.join("index.mdx"), cards.join("\n"))?;
        }

        // Generate major index page with semester cards
        let mut major_index = vec![
            "---".to_string(),
            "title: 目录".to_string(),
            "---".to_string(),
            "".to_string(),
            "<Cards>".to_string(),
        ];

        for (folder, title) in SEMESTER_MAPPING.iter().map(|(_, f, t)| (f, t)) {
            major_index.push(format!(
                "  <Card title=\"{}\" href=\"/docs/{}/{}/{}\" />",
                title, plan.year, plan.major_code, folder
            ));
        }
        major_index.push("</Cards>".to_string());

        fs::write(major_dir.join("index.mdx"), major_index.join("\n"))?;
    }

    // Generate year index pages
    for year in &years {
        let year_dir = docs_dir.join(year);
        let year_meta = serde_json::json!({"title": year});
        fs::write(
            year_dir.join("meta.json"),
            serde_json::to_string_pretty(&year_meta)?,
        )?;

        // Generate year index with major cards
        if let Some(majors) = majors_by_year.get(year) {
            let mut year_index = vec![
                "---".to_string(),
                "title: 目录".to_string(),
                "---".to_string(),
                "".to_string(),
                "<Cards>".to_string(),
            ];

            for (code, name) in majors {
                year_index.push(format!(
                    "  <Card title=\"{}\" href=\"/docs/{}/{}\" />",
                    name, year, code
                ));
            }
            year_index.push("</Cards>".to_string());

            fs::write(year_dir.join("index.mdx"), year_index.join("\n"))?;
        }
    }

    Ok(())
}
