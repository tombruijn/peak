use chrono::{DateTime, TimeZone, Utc};
use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal,
};
use git2::{BranchType, Repository};
use ratatui::{
    DefaultTerminal, TerminalOptions, Viewport,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

const UI_HEIGHT: u16 = 1;
const CURRENT_BRANCH_STYLE: Style = Style::new().fg(Color::Green);
const NORMAL_BRANCH_STYLE: Style = Style::new();
const ARROW_STYLE: Style = Style::new().fg(Color::Blue);

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let (_terminal_columns, terminal_rows) = terminal::size()?;
    let viewport_height = [terminal_rows, 20].iter().cloned().min().unwrap_or(20);
    let viewport = Viewport::Inline(viewport_height);
    let terminal = ratatui::init_with_options(TerminalOptions { viewport });
    let result = App::new(viewport_height)?.run(terminal);
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
    included_indexes: Vec<usize>,
    marked_indexes: Vec<usize>,
    state: ListState,
}

#[derive(Debug, Default, PartialEq)]
enum DisplayMode {
    #[default]
    Normal,
    Filter,
}

#[derive(Debug, Default)]
pub struct App {
    viewport_height: u16,
    display_mode: DisplayMode,
    active_filter: Option<String>,
    cursor_index: Option<usize>,
    branches: Branches,
}

impl App {
    fn new(viewport_height: u16) -> Result<Self> {
        let mut app = Self {
            viewport_height,
            cursor_index: Some(0),
            ..Default::default()
        };
        app.fetch_branches()?;
        app.apply_filter();

        Ok(app)
    }

    fn is_normal_mode(&self) -> bool {
        self.display_mode == DisplayMode::Normal
    }

    fn is_filter_mode(&self) -> bool {
        self.display_mode == DisplayMode::Filter
    }

    fn switch_to_normal_mode(&mut self) {
        self.display_mode = DisplayMode::Normal;
        self.cursor_index = Some(0);
    }

    fn switch_to_filter_mode(&mut self) {
        self.display_mode = DisplayMode::Filter;
        self.cursor_index = None;
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;

            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    KeyCode::Enter | KeyCode::Esc if self.is_filter_mode() => {
                        self.switch_to_normal_mode();
                    }
                    KeyCode::Backspace if self.is_filter_mode() => {
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
                    key_code if self.is_filter_mode() => {
                        if let Some(char) = key_code.as_char() {
                            if let Some(filter) = self.active_filter {
                                self.active_filter = Some(format!("{}{}", filter, char))
                            } else {
                                self.active_filter = Some(char.to_string())
                            };
                            self.apply_filter();
                        }
                    }
                    KeyCode::Char('/') if self.is_normal_mode() => {
                        self.switch_to_filter_mode();
                    }
                    KeyCode::Esc if self.is_normal_mode() => {
                        return Ok(());
                    }
                    KeyCode::Enter if self.is_normal_mode() => {
                        self.select_current_item();
                        return Ok(());
                    }
                    KeyCode::Delete if self.is_normal_mode() => {
                        self.delete_marked_items();
                        return Ok(());
                    }
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                        self.move_to_previous_item()
                    }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.move_to_next_item(),
                    KeyCode::Char('x') => self.mark_current_item(),
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('R') => {
                        self.active_filter = None;
                        self.apply_filter();
                    }
                    KeyCode::Char('X') => self.branches.marked_indexes.clear(),
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
            if let Some(new_cursor_index) = new_cursor_index {
                self.cursor_index = Some(new_cursor_index);
            } else {
                let branches_len = self.branches.included_indexes.len();
                let new_cursor_index = branches_len - 1;
                self.cursor_index = Some(new_cursor_index);
            }
        } else {
            self.cursor_index = Some(0);
        }
    }

    fn move_to_next_item(&mut self) {
        if let Some(cursor_index) = self.cursor_index {
            if cursor_index + 1 < self.branches.included_indexes.len() {
                let new_cursor_index = cursor_index + 1;
                self.cursor_index = Some(new_cursor_index);
            } else {
                self.cursor_index = Some(0);
            }
        } else {
            self.cursor_index = Some(0);
        }
    }

    fn select_current_item(&mut self) {
        todo!("Checkout branch: {:?}", self.cursor_index);
    }

    fn delete_marked_items(&mut self) {
        todo!("Delete branches: {:?}", self.branches.marked_indexes);
    }

    fn mark_current_item(&mut self) {
        if let Some(cursor_index) = self.cursor_index {
            let branch_index = self.branches.included_indexes[cursor_index];
            let index_of_mark_index = self
                .branches
                .marked_indexes
                .iter()
                .position(|mark_index| *mark_index == branch_index);
            if let Some(mark_index) = index_of_mark_index {
                self.branches.marked_indexes.remove(mark_index);
            } else {
                self.branches.marked_indexes.push(branch_index);
            }
        }
    }

    fn apply_filter(&mut self) {
        self.branches.included_indexes = self
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

        let list_viewport_offset = if let Some(cursor_index) = self.cursor_index {
            // -1 for 0 based index
            let list_height = (self.viewport_height - 1 - UI_HEIGHT) as usize;
            if cursor_index > list_height {
                cursor_index.checked_sub(list_height).unwrap_or_default()
            } else {
                0
            }
        } else {
            0
        };
        *self.branches.state.offset_mut() = list_viewport_offset;

        if self.is_filter_mode() || self.active_filter.is_some() {
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
            .branches
            .included_indexes
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
                let mark = if self.branches.marked_indexes.contains(&branch_index) {
                    Span::styled("■", ARROW_STYLE)
                } else {
                    Span::raw(" ")
                };
                ListItem::new(Line::from(vec![
                    selector,
                    Span::raw(" "),
                    mark,
                    Span::raw(" "),
                    Span::styled(branch.name.clone(), style),
                    Span::raw(time_ago),
                    label,
                ]))
            })
            .collect();
        let list = List::new(items);
        StatefulWidget::render(list, list_area, buffer, &mut self.branches.state);
    }
}
