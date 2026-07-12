use crate::constants::{get_semester_title_by_folder, parse_semester_folders, SEMESTER_MAPPING};
use crate::error::Result;
use crate::models::{
    Course, CourseMetadata, Frontmatter, GradeDetail, GradingItem, HourDistributionMeta, Plan,
    SharedCategory, WorktreeData,
};
use crate::tree::{build_file_tree, tree_to_jsx};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
struct SemesterCourseCard {
    slug: String,
    name: String,
    course_nature: Option<String>,
    assessment_method: Option<String>,
    source_index: usize,
}

/// Build YAML frontmatter for a course page using serde_yaml
fn build_frontmatter(title: &str, course: &Course) -> String {
    let credit = course.credit.unwrap_or(0.0);
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

                (percent > 0).then(|| GradingItem {
                    name: detail.name.clone(),
                    percent,
                })
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

fn title_from_mdx(mdx_content: &str, fallback: &str) -> String {
    let lines: Vec<&str> = mdx_content.lines().collect();
    for line in lines.iter().take(5) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" {
            continue;
        }
        let raw = if let Some(t) = trimmed.strip_prefix("title:") {
            t.trim().trim_matches('"').trim_matches('\'').to_string()
        } else {
            trimmed.to_string()
        };
        let raw = raw.trim_start_matches("# ").trim();
        return if let Some(rest) = raw.split_once(" - ") {
            rest.1.trim().to_string()
        } else {
            raw.to_string()
        };
    }
    fallback.to_string()
}

fn minimal_course(repo_id: &str, name: &str, grade_details: Option<Vec<GradeDetail>>) -> Course {
    Course {
        repo_id: repo_id.to_string(),
        name: name.to_string(),
        credit: None,
        assessment_method: None,
        course_nature: None,
        recommended_semester: None,
        hours: None,
        grade_details,
    }
}

fn readme_body_content(readme_content: &str) -> String {
    readme_content
        .lines()
        .skip(2)
        .collect::<Vec<_>>()
        .join("\n")
}

fn missing_repo_notice(repo_id: &str) -> String {
    format!(
        "当前培养方案中包含这门课程，但 HOA 暂未收录对应的 GitHub 仓库（`{repo_id}`）/建立相关的映射。\
\n\n如果你希望补充这门课程的资料，请前往 [HOA Wiki](https://wiki.hoa.moe/contribution-guide/course-mapping) 查看如何建立映射或者联系管理员添加仓库。"
    )
}

fn build_course_page_content(
    frontmatter: &str,
    content: &str,
    filetree_content: &str,
    use_course_info: bool,
) -> String {
    let mut sections = vec![frontmatter.to_string()];

    if use_course_info {
        sections.push("<CourseInfo />".to_string());
    }

    let body = format!("{}{}", content, filetree_content);
    if !body.trim().is_empty() {
        sections.push(body);
    }

    sections.join("\n\n")
}

fn course_nature_rank(course_nature: Option<&str>) -> u8 {
    match course_nature.map(str::trim) {
        Some("必修") => 0,
        Some("限选") => 1,
        Some("选修") => 2,
        _ => 3,
    }
}

fn assessment_method_rank(assessment_method: Option<&str>) -> u8 {
    match assessment_method.map(str::trim) {
        Some("考试") => 0,
        Some("考查") => 1,
        _ => 2,
    }
}

fn sort_semester_cards(cards: &mut [SemesterCourseCard]) {
    cards.sort_by(|a, b| {
        course_nature_rank(a.course_nature.as_deref())
            .cmp(&course_nature_rank(b.course_nature.as_deref()))
            .then_with(|| {
                assessment_method_rank(a.assessment_method.as_deref())
                    .cmp(&assessment_method_rank(b.assessment_method.as_deref()))
            })
            .then_with(|| a.source_index.cmp(&b.source_index))
            .then_with(|| a.slug.cmp(&b.slug))
    });
}

/// Generate all course pages and index pages
pub async fn generate_course_pages(
    plans: &[Plan],
    shared_categories: &[SharedCategory],
    no_course_info_repo_ids: &HashSet<String>,
    grades_summary: &HashMap<String, HashMap<String, Vec<GradeDetail>>>,
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
            .or_default()
            .push((plan.major_code.clone(), plan.major_name.clone()));

        let major_dir = docs_dir.join(&plan.year).join(&plan.major_code);
        fs::create_dir_all(&major_dir)?;

        // Track courses by semester for this major
        let mut courses_by_semester: HashMap<String, Vec<SemesterCourseCard>> = HashMap::new();

        // Process each course
        for (course_index, course) in plan.courses.iter().enumerate() {
            let mdx_path = repos_dir.join(format!("{}.mdx", course.repo_id));
            let json_path = repos_dir.join(format!("{}.json", course.repo_id));

            // Determine target directories based on semester (supports multi-semester values)
            let semester_folders = course
                .recommended_semester
                .as_deref()
                .map(parse_semester_folders)
                .unwrap_or_default();

            let mut target_dirs = Vec::new();
            if semester_folders.is_empty() {
                target_dirs.push(major_dir.clone());
            } else {
                for (folder, _title) in semester_folders {
                    let sem_dir = major_dir.join(folder);
                    fs::create_dir_all(&sem_dir)?;
                    courses_by_semester
                        .entry(folder.to_string())
                        .or_default()
                        .push(SemesterCourseCard {
                            slug: course.repo_id.clone(),
                            name: course.name.clone(),
                            course_nature: course.course_nature.clone(),
                            assessment_method: course.assessment_method.clone(),
                            source_index: course_index,
                        });
                    target_dirs.push(sem_dir);
                }
            }

            let (content, filetree_content) = if mdx_path.exists() {
                let readme_content = fs::read_to_string(&mdx_path)?;
                let content = readme_body_content(&readme_content);

                let filetree_content = if json_path.exists() {
                    let json_content = fs::read_to_string(&json_path)?;
                    let worktree: WorktreeData = serde_json::from_str(&json_content)?;
                    let tree = build_file_tree(&worktree, &course.repo_id);
                    let jsx = tree_to_jsx(&tree, 1);
                    format!(
                        "\n\n## 资源下载\n\n<Files url=\"https://open.osa.moe/openauto/{}\">\n{}\n</Files>",
                        course.repo_id, jsx
                    )
                } else {
                    String::new()
                };

                (content, filetree_content)
            } else {
                (missing_repo_notice(&course.repo_id), String::new())
            };

            // Build frontmatter
            let frontmatter = build_frontmatter(&course.name, course);

            // Write course page
            let page_content =
                build_course_page_content(&frontmatter, &content, &filetree_content, true);
            for target_dir in target_dirs {
                fs::write(
                    target_dir.join(format!("{}.mdx", course.repo_id)),
                    &page_content,
                )?;
            }
        }

        // Keep semester pages and navigation in semantic order
        let ordered_semester_folders: Vec<String> = SEMESTER_MAPPING
            .iter()
            .filter_map(|(_, folder, _)| {
                courses_by_semester
                    .contains_key(*folder)
                    .then_some((*folder).to_string())
            })
            .collect();

        // Generate semester index pages
        for folder in &ordered_semester_folders {
            let mut courses = courses_by_semester.get(folder).cloned().unwrap_or_default();
            sort_semester_cards(&mut courses);
            let sem_dir = major_dir.join(folder);
            let sem_title = get_semester_title_by_folder(folder).unwrap_or(folder.as_str());

            let mut cards = vec![
                "---".to_string(),
                format!("title: {}", sem_title),
                "---".to_string(),
                "".to_string(),
                "<Cards>".to_string(),
            ];

            for course in &courses {
                cards.push(format!(
                    "  <Card title=\"{}\" href=\"/docs/{}/{}/{}/{}\" />",
                    course.name, plan.year, plan.major_code, folder, course.slug
                ));
            }
            cards.push("</Cards>".to_string());

            fs::write(sem_dir.join("index.mdx"), cards.join("\n"))?;
        }

        // Shared categories
        let mut category_pages: Vec<String> = Vec::new();
        for cat in shared_categories {
            let cat_dir = major_dir.join(&cat.id);
            fs::create_dir_all(&cat_dir)?;

            let mut category_courses: Vec<(String, String)> = Vec::new();

            for repo_id in &cat.repo_ids {
                if !repos_set.is_empty() && !repos_set.contains(repo_id) {
                    continue;
                }

                let mdx_path = repos_dir.join(format!("{}.mdx", repo_id));
                let json_path = repos_dir.join(format!("{}.json", repo_id));

                if !mdx_path.exists() {
                    continue;
                }

                let readme_content = fs::read_to_string(&mdx_path)?;
                let title = title_from_mdx(&readme_content, repo_id);
                category_courses.push((repo_id.clone(), title.clone()));

                let content = readme_body_content(&readme_content);

                let filetree_content = if json_path.exists() {
                    let json_content = fs::read_to_string(&json_path)?;
                    let worktree: WorktreeData = serde_json::from_str(&json_content)?;
                    let tree = build_file_tree(&worktree, repo_id);
                    let jsx = tree_to_jsx(&tree, 1);
                    format!(
                        "\n\n## 资源下载\n\n<Files url=\"https://open.osa.moe/openauto/{}\">\n{}\n</Files>",
                        repo_id, jsx
                    )
                } else {
                    String::new()
                };

                let grade_details = grades_summary
                    .get(repo_id)
                    .and_then(|m| m.get("default"))
                    .cloned();
                let course = minimal_course(repo_id, &title, grade_details);
                let frontmatter = build_frontmatter(&title, &course);
                let use_course_info = !no_course_info_repo_ids.contains(repo_id);
                let page_content = build_course_page_content(
                    &frontmatter,
                    &content,
                    &filetree_content,
                    use_course_info,
                );
                fs::write(cat_dir.join(format!("{}.mdx", repo_id)), &page_content)?;
            }

            if !category_courses.is_empty() {
                category_pages.push(cat.id.clone());

                let mut cards = vec![
                    "---".to_string(),
                    format!("title: {}", cat.title),
                    "---".to_string(),
                    "".to_string(),
                    "<Cards>".to_string(),
                ];
                for (slug, name) in &category_courses {
                    cards.push(format!(
                        "  <Card title=\"{}\" href=\"/docs/{}/{}/{}/{}\" />",
                        name, plan.year, plan.major_code, cat.id, slug
                    ));
                }
                cards.push("</Cards>".to_string());
                fs::write(cat_dir.join("index.mdx"), cards.join("\n"))?;
            }
        }

        // Write major metadata
        let pages: Vec<String> = ordered_semester_folders.iter().cloned()
            .chain(category_pages.iter().cloned())
            .chain(std::iter::once("...".to_string()))
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

        // Generate major index page with semester cards
        let mut major_index = vec![
            "---".to_string(),
            "title: 目录".to_string(),
            "---".to_string(),
            "".to_string(),
            "<Cards>".to_string(),
        ];

        for folder in &ordered_semester_folders {
            let title = get_semester_title_by_folder(folder).unwrap_or(folder.as_str());
            major_index.push(format!(
                "  <Card title=\"{}\" href=\"/docs/{}/{}/{}\" />",
                title, plan.year, plan.major_code, folder
            ));
        }
        for cat in shared_categories {
            if category_pages.contains(&cat.id) {
                major_index.push(format!(
                    "  <Card title=\"{}\" href=\"/docs/{}/{}/{}\" />",
                    cat.title, plan.year, plan.major_code, cat.id
                ));
            }
        }
        major_index.push("</Cards>".to_string());

        fs::write(major_dir.join("index.mdx"), major_index.join("\n"))?;
    }

    // Generate year index pages in sorted order
    let mut year_list: Vec<String> = years.into_iter().collect();
    year_list.sort();
    for year in &year_list {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_repo_notice_contains_templates() {
        let content = missing_repo_notice("MISSING101");

        assert!(content.contains("MISSING101"));
        assert!(content.contains("normal-course-template"));
        assert!(content.contains("multi-project-course-template"));
    }

    #[test]
    fn test_build_course_page_content_omits_empty_body() {
        let page = build_course_page_content("---\ntitle: Test\n---", "", "", true);

        assert_eq!(page, "---\ntitle: Test\n---\n\n<CourseInfo />");
    }

    #[test]
    fn test_sort_semester_cards_by_nature_and_assessment() {
        let mut cards = vec![
            SemesterCourseCard {
                slug: "LIMITED_NON_EXAM".to_string(),
                name: "Limited Non Exam".to_string(),
                course_nature: Some("限选".to_string()),
                assessment_method: Some("考查".to_string()),
                source_index: 0,
            },
            SemesterCourseCard {
                slug: "REQUIRED_NON_EXAM_A".to_string(),
                name: "Required Non Exam A".to_string(),
                course_nature: Some("必修".to_string()),
                assessment_method: Some("考查".to_string()),
                source_index: 1,
            },
            SemesterCourseCard {
                slug: "REQUIRED_EXAM".to_string(),
                name: "Required Exam".to_string(),
                course_nature: Some("必修".to_string()),
                assessment_method: Some("考试".to_string()),
                source_index: 2,
            },
            SemesterCourseCard {
                slug: "ELECTIVE_EXAM".to_string(),
                name: "Elective Exam".to_string(),
                course_nature: Some("选修".to_string()),
                assessment_method: Some("考试".to_string()),
                source_index: 3,
            },
            SemesterCourseCard {
                slug: "REQUIRED_NON_EXAM_B".to_string(),
                name: "Required Non Exam B".to_string(),
                course_nature: Some("必修".to_string()),
                assessment_method: Some("考查".to_string()),
                source_index: 4,
            },
        ];

        sort_semester_cards(&mut cards);

        let ordered_slugs: Vec<_> = cards.into_iter().map(|card| card.slug).collect();
        assert_eq!(
            ordered_slugs,
            vec![
                "REQUIRED_EXAM",
                "REQUIRED_NON_EXAM_A",
                "REQUIRED_NON_EXAM_B",
                "LIMITED_NON_EXAM",
                "ELECTIVE_EXAM",
            ]
        );
    }

    #[tokio::test]
    async fn test_generate_placeholder_for_plan_course_not_in_repos_list() {
        use std::collections::{HashMap, HashSet};
        use std::path::PathBuf;
        use std::time::{SystemTime, UNIX_EPOCH};

        fn make_temp_dir(prefix: &str) -> PathBuf {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{prefix}-{unique}"));
            fs::create_dir_all(&path).unwrap();
            path
        }

        let repo_root = make_temp_dir("hoa-generator-test");
        let repos_dir = repo_root.join("repos");
        let docs_dir = repo_root.join("docs");
        fs::create_dir_all(&repos_dir).unwrap();
        fs::create_dir_all(&docs_dir).unwrap();

        let plans = vec![Plan {
            year: "2024".to_string(),
            major_code: "CS".to_string(),
            major_name: "Computer Science".to_string(),
            courses: vec![Course {
                repo_id: "MISSING101".to_string(),
                name: "Missing Course".to_string(),
                credit: Some(2.0),
                assessment_method: Some("考查".to_string()),
                course_nature: Some("必修".to_string()),
                recommended_semester: Some("第一学年秋季".to_string()),
                hours: None,
                grade_details: None,
            }],
        }];

        let repos_set = HashSet::from([String::from("EXISTING101")]);
        let shared_categories: Vec<SharedCategory> = Vec::new();
        let no_course_info_repo_ids = HashSet::new();
        let grades_summary: HashMap<String, HashMap<String, Vec<GradeDetail>>> = HashMap::new();

        generate_course_pages(
            &plans,
            &shared_categories,
            &no_course_info_repo_ids,
            &grades_summary,
            &repos_dir,
            &docs_dir,
            &repos_set,
        )
        .await
        .unwrap();

        let course_page = docs_dir
            .join("2024")
            .join("CS")
            .join("fresh-autumn")
            .join("MISSING101.mdx");
        let semester_index = docs_dir
            .join("2024")
            .join("CS")
            .join("fresh-autumn")
            .join("index.mdx");

        assert!(course_page.exists());
        assert!(semester_index.exists());

        let page_content = fs::read_to_string(course_page).unwrap();
        let index_content = fs::read_to_string(semester_index).unwrap();

        assert!(page_content.contains("课程资料暂缺"));
        assert!(index_content.contains("Missing Course"));

        let _ = fs::remove_dir_all(repo_root);
    }
}
