use chrono::{DateTime, Utc};
use git2::BranchType;

#[derive(Debug)]
pub struct Branch {
    pub name: String,
    pub branch_type: BranchType,
    pub author: String,
    pub current: bool,
    pub last_commit_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct Branches {
    pub entries: Vec<Branch>,
    pub included_indexes: Vec<usize>,
    pub marked_indexes: Vec<usize>,
    pub branch_type: Option<BranchType>,
    pub state: ratatui::widgets::TableState,
}

#[derive(Debug, Default, PartialEq)]
pub enum DisplayMode {
    #[default]
    Normal,
    Help,
    Filter,
    ConfirmDeletion(bool),
}

pub struct Alert {
    pub title: String,
    pub message: String,
}
