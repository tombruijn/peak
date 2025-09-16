use chrono::{DateTime, TimeZone, Utc};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use git2::{BranchType, Repository};
use ratatui::{
    DefaultTerminal, TerminalOptions, Viewport,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph, Widget},
};

const CURRENT_BRANCH_STYLE: Style = Style::new().fg(Color::Green);
const NORMAL_BRANCH_STYLE: Style = Style::new();
const ARROW_STYLE: Style = Style::new().fg(Color::Blue);

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init_with_options(TerminalOptions {
        viewport: Viewport::Inline(10),
    });
    let result = App::new()?.run(terminal);
    ratatui::restore();
    result
}

#[derive(Debug)]
struct Branch {
    name: String,
    current: bool,
    last_commit_at: DateTime<Utc>,
}

#[derive(Default, Debug)]
struct Branches {
    entries: Vec<Branch>,
}

#[derive(Debug, Default)]
pub struct App {
    display_filter: bool,
    active_filter: Option<String>,
    cursor_index: Option<usize>,
    included_branch_indexes: Vec<usize>,
    branches: Branches,
}

impl App {
    fn new() -> Result<Self> {
        let mut app = Self {
            cursor_index: Some(0),
            ..Default::default()
        };
        app.fetch_branches()?;
        app.apply_filter();
        Ok(app)
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;

            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Enter | KeyCode::Esc if self.display_filter => {
                        self.display_filter = false;
                        self.cursor_index = Some(0);
                    }
                    KeyCode::Backspace if self.display_filter => {
                        if let Some(mut filter) = self.active_filter {
                            filter.pop();
                            self.active_filter = if filter.is_empty() {
                                None
                            } else {
                                Some(filter)
                            };
                            self.apply_filter();
                        }
                    }
                    key_code if self.display_filter => {
                        if let Some(char) = key_code.as_char() {
                            if let Some(filter) = self.active_filter {
                                self.active_filter = Some(format!("{}{}", filter, char))
                            } else {
                                self.active_filter = Some(char.to_string())
                            };
                            self.apply_filter();
                        }
                    }
                    KeyCode::Char('/') if !self.display_filter => {
                        self.cursor_index = None;
                        self.display_filter = true
                    }
                    KeyCode::Esc if !self.display_filter => {
                        return Ok(());
                    }
                    KeyCode::Enter if !self.display_filter => {
                        self.select_current_item();
                        return Ok(());
                    }
                    KeyCode::Up | KeyCode::BackTab => self.move_to_previous_item(),
                    KeyCode::Char('k') => self.move_to_previous_item(),
                    KeyCode::Down | KeyCode::Tab => self.move_to_next_item(),
                    KeyCode::Char('j') => self.move_to_next_item(),
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('R') => {
                        self.active_filter = None;
                        self.apply_filter();
                    }
                    _ => {}
                },
                Event::Mouse(_) => {}
                Event::Resize(_, _) => {} // TODO: rerender
                _ => {}
            }
        }
    }

    fn move_to_previous_item(&mut self) {
        if let Some(cursor_index) = self.cursor_index {
            let new_cursor_index = cursor_index.checked_sub(1);
            if new_cursor_index.is_some() {
                self.cursor_index = new_cursor_index;
            }
        } else {
            self.cursor_index = Some(0)
        }
    }

    fn move_to_next_item(&mut self) {
        if let Some(cursor_index) = self.cursor_index {
            if cursor_index + 1 < self.included_branch_indexes.len() {
                self.cursor_index = Some(cursor_index + 1)
            }
        } else {
            self.cursor_index = Some(0)
        }
    }

    fn select_current_item(&mut self) {
        todo!("Checkout branch: {:?}", self.cursor_index);
    }

    fn apply_filter(&mut self) {
        self.included_branch_indexes = self
            .branches
            .entries
            .iter()
            .enumerate()
            .filter(|(_index, branch)| {
                if let Some(filter) = &self.active_filter {
                    branch.name.to_lowercase().contains(&filter.to_lowercase())
                } else {
                    true
                }
            })
            .map(|(index, _branch)| index)
            .collect();
    }

    fn fetch_branches(&mut self) -> Result<()> {
        let repository = Repository::init(".")?;
        self.branches.entries = repository
            .branches(Some(BranchType::Local))?
            .filter_map(|branch_item| {
                if let Ok((branch, _branch_type)) = branch_item {
                    let name = if let Ok(Some(name)) = branch.name() {
                        name.to_string()
                    } else {
                        return None;
                    };
                    let commit = branch.get().peel_to_commit().ok()?;
                    let last_commit_at = Utc.timestamp_opt(commit.time().seconds(), 0).single()?;

                    Some(Branch {
                        name,
                        current: branch.is_head(),
                        last_commit_at,
                    })
                } else {
                    None
                }
            })
            .collect();
        self.branches
            .entries
            .sort_by(|a, b| b.last_commit_at.cmp(&a.last_commit_at));
        Ok(())
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let [header_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        if self.display_filter || self.active_filter.is_some() {
            let filter = if let Some(filter) = &self.active_filter {
                filter
            } else {
                &"".to_string()
            };
            Paragraph::new(format!("Filter branches: {}", filter)).render(header_area, buffer);
        } else {
            Paragraph::new("Select branch:").render(header_area, buffer);
        }

        let now = Utc::now();
        let items: Vec<ListItem> = self
            .included_branch_indexes
            .iter()
            .enumerate()
            .map(|(item_index, &branch_index)| {
                let branch = &self.branches.entries[branch_index];
                let (style, label) = if branch.current {
                    (CURRENT_BRANCH_STYLE, Span::raw(" [current]"))
                } else {
                    (NORMAL_BRANCH_STYLE, Span::default())
                };
                let duration = now.signed_duration_since(branch.last_commit_at);
                let time_ago = if duration.num_days() > 0 {
                    format!(" ({} days ago)", duration.num_days())
                } else if duration.num_hours() > 0 {
                    format!(" ({} hours ago)", duration.num_hours())
                } else {
                    format!(" ({} minutes ago)", duration.num_minutes().max(1))
                };

                let selector = if self.cursor_index == Some(item_index) {
                    Span::styled("▶", ARROW_STYLE)
                } else {
                    Span::raw(" ")
                };
                ListItem::new(Line::from(vec![
                    selector,
                    Span::raw(" "),
                    Span::styled(branch.name.clone(), style),
                    Span::raw(time_ago),
                    label,
                ]))
            })
            .collect();
        List::new(items).render(list_area, buffer);
    }
}
