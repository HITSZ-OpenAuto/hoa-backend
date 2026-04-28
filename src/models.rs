use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct TomlPlan {
    pub info: PlanInfo,
    pub courses: Vec<TomlCourse>,
}

#[derive(Debug, Deserialize)]
pub struct PlanInfo {
    pub year: String,
    pub major_code: String,
    pub major_name: String,
    #[serde(rename = "plan_ID")]
    pub plan_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TomlCourse {
    pub course_code: String,
    pub course_name: String,
    pub credit: Option<f64>,
    pub assessment_method: Option<String>,
    pub course_nature: Option<String>,
    pub recommended_year_semester: Option<String>,
    pub hours: Option<HourDistribution>,
    pub grade_details: Option<Vec<GradeDetail>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GradeDetail {
    pub name: String,
    pub percent: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HourDistribution {
    pub theory: Option<u32>,
    pub lab: Option<u32>,
    pub practice: Option<u32>,
    pub exercise: Option<u32>,
    pub computer: Option<u32>,
    pub tutoring: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub year: String,
    pub major_code: String,
    pub major_name: String,
    pub courses: Vec<Course>,
}

#[derive(Debug, Clone)]
pub struct SharedCategory {
    pub id: String,
    pub title: String,
    pub repo_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Course {
    pub repo_id: String,
    pub name: String,
    pub credit: Option<f64>,
    pub assessment_method: Option<String>,
    pub course_nature: Option<String>,
    pub recommended_semester: Option<String>,
    pub hours: Option<HourDistribution>,
    pub grade_details: Option<Vec<GradeDetail>>,
}

#[derive(Debug, Deserialize)]
pub struct WorktreeData(pub std::collections::HashMap<String, FileMetadata>);

#[derive(Debug, Deserialize)]
pub struct FileMetadata {
    pub size: Option<u64>,
    pub time: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub node_type: NodeType,
    pub children: Vec<FileNode>,
    pub url: Option<String>,
    pub size: Option<u64>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    Folder,
    File,
}

#[derive(Debug, Serialize)]
pub struct Frontmatter {
    pub title: String,
    pub description: String,
    pub course: CourseMetadata,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseMetadata {
    pub credit: f64,
    pub assessment_method: String,
    pub course_nature: String,
    pub hour_distribution: HourDistributionMeta,
    pub grading_scheme: Vec<GradingItem>,
}

#[derive(Debug, Serialize)]
pub struct HourDistributionMeta {
    pub theory: u32,
    pub lab: u32,
    pub practice: u32,
    pub exercise: u32,
    pub computer: u32,
    pub tutoring: u32,
}

#[derive(Debug, Serialize)]
pub struct GradingItem {
    pub name: String,
    pub percent: u32,
}

impl Frontmatter {
    /// Convert frontmatter to YAML string
    pub fn to_yaml(&self) -> String {
        // Use serde_yaml to serialize, but customize for better formatting
        match serde_yaml::to_string(self) {
            Ok(yaml) => format!("---\n{}---", yaml),
            Err(_) => {
                // Fallback to empty frontmatter
                "---\ntitle: ''\ndescription: ''\n---".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontmatter_to_yaml_basic() {
        let frontmatter = Frontmatter {
            title: "Test Course".to_string(),
            description: "A test description".to_string(),
            course: CourseMetadata {
                credit: 3.0,
                assessment_method: "Exam".to_string(),
                course_nature: "Required".to_string(),
                hour_distribution: HourDistributionMeta {
                    theory: 48,
                    lab: 0,
                    practice: 0,
                    exercise: 0,
                    computer: 0,
                    tutoring: 0,
                },
                grading_scheme: vec![
                    GradingItem {
                        name: "Final Exam".to_string(),
                        percent: 70,
                    },
                    GradingItem {
                        name: "Homework".to_string(),
                        percent: 30,
                    },
                ],
            },
        };

        let yaml = frontmatter.to_yaml();

        assert!(yaml.starts_with("---\n"));
        assert!(yaml.ends_with("---"));
        assert!(yaml.contains("title: Test Course"));
        assert!(yaml.contains("description: A test description"));
        assert!(yaml.contains("credit: 3"));
        assert!(yaml.contains("assessmentMethod: Exam"));
        assert!(yaml.contains("courseNature: Required"));
    }

    #[test]
    fn test_frontmatter_to_yaml_with_grading_scheme() {
        let frontmatter = Frontmatter {
            title: "Advanced Math".to_string(),
            description: "".to_string(),
            course: CourseMetadata {
                credit: 4.0,
                assessment_method: "Mixed".to_string(),
                course_nature: "Elective".to_string(),
                hour_distribution: HourDistributionMeta {
                    theory: 32,
                    lab: 16,
                    practice: 0,
                    exercise: 0,
                    computer: 0,
                    tutoring: 0,
                },
                grading_scheme: vec![
                    GradingItem {
                        name: "Midterm".to_string(),
                        percent: 30,
                    },
                    GradingItem {
                        name: "Final".to_string(),
                        percent: 50,
                    },
                    GradingItem {
                        name: "Lab".to_string(),
                        percent: 20,
                    },
                ],
            },
        };

        let yaml = frontmatter.to_yaml();

        assert!(yaml.contains("gradingScheme:"));
        assert!(yaml.contains("name: Midterm"));
        assert!(yaml.contains("percent: 30"));
        assert!(yaml.contains("name: Final"));
        assert!(yaml.contains("percent: 50"));
        assert!(yaml.contains("name: Lab"));
        assert!(yaml.contains("percent: 20"));
    }

    #[test]
    fn test_frontmatter_to_yaml_empty_grading_scheme() {
        let frontmatter = Frontmatter {
            title: "Simple Course".to_string(),
            description: "No grading details".to_string(),
            course: CourseMetadata {
                credit: 2.0,
                assessment_method: "Pass/Fail".to_string(),
                course_nature: "Optional".to_string(),
                hour_distribution: HourDistributionMeta {
                    theory: 24,
                    lab: 0,
                    practice: 0,
                    exercise: 0,
                    computer: 0,
                    tutoring: 0,
                },
                grading_scheme: vec![],
            },
        };

        let yaml = frontmatter.to_yaml();

        assert!(yaml.contains("title: Simple Course"));
        assert!(yaml.contains("gradingScheme: []"));
    }

    #[test]
    fn test_hour_distribution_meta_all_fields() {
        let frontmatter = Frontmatter {
            title: "Complex Course".to_string(),
            description: "".to_string(),
            course: CourseMetadata {
                credit: 5.0,
                assessment_method: "Comprehensive".to_string(),
                course_nature: "Core".to_string(),
                hour_distribution: HourDistributionMeta {
                    theory: 32,
                    lab: 16,
                    practice: 8,
                    exercise: 4,
                    computer: 8,
                    tutoring: 2,
                },
                grading_scheme: vec![],
            },
        };

        let yaml = frontmatter.to_yaml();

        assert!(yaml.contains("theory: 32"));
        assert!(yaml.contains("lab: 16"));
        assert!(yaml.contains("practice: 8"));
        assert!(yaml.contains("exercise: 4"));
        assert!(yaml.contains("computer: 8"));
        assert!(yaml.contains("tutoring: 2"));
    }

    #[test]
    fn test_grading_item_serialization() {
        let item = GradingItem {
            name: "Quiz".to_string(),
            percent: 15,
        };

        let yaml = serde_yaml::to_string(&item).unwrap();

        assert!(yaml.contains("name: Quiz"));
        assert!(yaml.contains("percent: 15"));
    }

    #[test]
    fn test_hour_distribution_meta_zero_values() {
        let hours = HourDistributionMeta {
            theory: 0,
            lab: 0,
            practice: 0,
            exercise: 0,
            computer: 0,
            tutoring: 0,
        };

        let yaml = serde_yaml::to_string(&hours).unwrap();

        // All fields should be present even if zero
        assert!(yaml.contains("theory: 0"));
        assert!(yaml.contains("lab: 0"));
    }
}
