//! Validation — filled in Task 3.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestIssueLevel { Error, Warning }

#[derive(Debug, Clone)]
pub struct ManifestIssue {
    pub field: String,
    pub message: String,
    pub level: ManifestIssueLevel,
}
