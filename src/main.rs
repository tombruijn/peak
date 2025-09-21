use std::process::Command;

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
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, Padding, Paragraph, Row, StatefulWidget, Table,
        TableState, Widget, Wrap,
    },
};

const UI_HEIGHT: u16 = 1;
const CURRENT_BRANCH_STYLE: Style = Style::new().fg(Color::Green);
const NORMAL_BRANCH_STYLE: Style = Style::new();
const ARROW_STYLE: Style = Style::new().fg(Color::Blue);
const TIME_AGO_STYLE: Style = Style::new().fg(Color::DarkGray);

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
    state: TableState,
}

#[derive(Debug, Default, PartialEq)]
enum DisplayMode {
    #[default]
    Normal,
    Filter,
    ConfirmDeletion,
}

pub struct App {
    repository: Repository,
    viewport_height: u16,
    display_mode: DisplayMode,
    active_filter: Option<String>,
    cursor_index: Option<usize>,
    branches: Branches,
}

impl App {
    fn new(viewport_height: u16) -> Result<Self> {
        let mut app = Self {
            repository: Repository::init(".")?,

            viewport_height,
            display_mode: DisplayMode::default(),
            active_filter: None,
            cursor_index: Some(0),
            branches: Branches::default(),
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

    fn is_confirm_deletion_mode(&self) -> bool {
        self.display_mode == DisplayMode::ConfirmDeletion
    }

    fn switch_to_normal_mode(&mut self) {
        self.display_mode = DisplayMode::Normal;
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
                    // Force quit with CTRL+C at all times
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }

                    // Confirm branch deletion
                    KeyCode::Char('y' | 'Y') if self.is_confirm_deletion_mode() => {
                        self.delete_marked_items()?;
                    }
                    // Exit branch deletion
                    KeyCode::Esc | KeyCode::Char('n' | 'N' | 'q')
                        if self.is_confirm_deletion_mode() =>
                    {
                        self.switch_to_normal_mode();
                    }

                    // Submit filter
                    KeyCode::Enter if self.is_filter_mode() => {
                        self.switch_to_normal_mode();
                        self.cursor_index = Some(0);
                    }
                    // Dismiss and reset filter
                    KeyCode::Esc if self.is_filter_mode() => {
                        self.active_filter = None;
                        self.apply_filter();
                        self.switch_to_normal_mode();
                        self.cursor_index = Some(0);
                    }
                    // Remove last character of the filter
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
                    // Add character to filter
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

                    // Switch to filter mode
                    KeyCode::Char('/' | 'f') if self.is_normal_mode() => {
                        self.switch_to_filter_mode();
                    }

                    // Exit in normal mode
                    KeyCode::Esc | KeyCode::Char('q') if self.is_normal_mode() => {
                        return Ok(());
                    }

                    // Switch to the branch on which the line cursor is
                    KeyCode::Enter if self.is_normal_mode() => {
                        if let Err(err) = self.select_current_item() {
                            todo!("Branch deletion error not handled: {:?}", err);
                        } else {
                            return Ok(());
                        }
                    }

                    // Delete the marked branches
                    // But first show a confirmation prompt
                    KeyCode::Delete | KeyCode::Char('d') if self.is_normal_mode() => {
                        if !self.branches.marked_indexes.is_empty() {
                            self.display_mode = DisplayMode::ConfirmDeletion;
                        }
                    }
                    // Mark an item for deletion
                    KeyCode::Char('x') if self.is_normal_mode() => self.mark_current_item(),
                    // Reset selection
                    KeyCode::Char('X') if self.is_normal_mode() => {
                        self.branches.marked_indexes.clear()
                    }

                    // Move up to the branch above
                    // If on the first line, wrap around to the end
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab
                        if self.is_normal_mode() =>
                    {
                        self.move_to_previous_item()
                    }
                    // Move down to the branch below
                    // If on the last line, wrap around to the beginning
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab if self.is_normal_mode() => {
                        self.move_to_next_item()
                    }

                    // Reset filter
                    KeyCode::Char('R') if self.is_normal_mode() => {
                        self.active_filter = None;
                        self.apply_filter();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    fn move_to_previous_item(&mut self) {
        if self.branches.included_indexes.is_empty() {
            return;
        }

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
        if self.branches.included_indexes.is_empty() {
            return;
        }

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

    fn select_current_item(&mut self) -> Result<(), String> {
        if let Some(cursor_index) = self.cursor_index {
            let branch_index = self.branches.included_indexes[cursor_index];
            let branch = &self.branches.entries[branch_index];
            let branch_name = branch.name.clone();
            match Command::new("git").args(["switch", &branch_name]).output() {
                Ok(_output) => Ok(()),
                Err(err) => Err(format!(
                    "Could not check out '{}' branch: {}",
                    branch_name, err
                )),
            }
        } else {
            todo!("This should not happen: branch checkout without branch selected");
        }
    }

    fn delete_marked_items(&mut self) -> Result<()> {
        for branch_index in self.branches.marked_indexes.iter() {
            let branch = &self.branches.entries[*branch_index];
            if let Ok(mut git_branch) = self.repository.find_branch(&branch.name, BranchType::Local)
            {
                git_branch.delete()?;
            }
        }
        // Remove mark for deleted branches
        self.branches.marked_indexes.clear();
        let offset = self.branches.state.offset();
        *self.branches.state.offset_mut() = if offset > 0 {
            // Move the offset 1 line up, in case deleting the last line
            offset - 1
        } else {
            0
        };
        if let Some(cursor_index) = self.cursor_index {
            self.cursor_index = if let Some(new_cursor_index) = cursor_index.checked_sub(1) {
                // Move current line 1 line up if not on first line
                Some(new_cursor_index)
            } else {
                // If deleting the first branch: set cursor to first line
                Some(0)
            }
        }
        self.switch_to_normal_mode();
        self.fetch_branches()?;
        self.apply_filter();
        Ok(())
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
        self.branches.entries = self
            .repository
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

    fn render_list(&mut self, area: Rect, buffer: &mut Buffer) {
        let now = Utc::now();
        let mut max_branch_name_length = 0;
        let mut rows: Vec<Row> = self
            .branches
            .included_indexes
            .iter()
            .enumerate()
            .map(|(item_index, &branch_index)| {
                let branch = &self.branches.entries[branch_index];
                let (style, label) = if branch.current {
                    (
                        CURRENT_BRANCH_STYLE,
                        Span::styled(" *", CURRENT_BRANCH_STYLE),
                    )
                } else {
                    (NORMAL_BRANCH_STYLE, Span::default())
                };
                let duration = now.signed_duration_since(branch.last_commit_at);
                let time_ago = if duration.num_days() > 0 {
                    format!("{} days ago", duration.num_days())
                } else if duration.num_hours() > 0 {
                    format!("{} hours ago", duration.num_hours())
                } else {
                    format!("{} minutes ago", duration.num_minutes().max(1))
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
                let branch_name_line = Line::from(vec![
                    selector,
                    Span::raw(" "),
                    mark,
                    Span::raw(" "),
                    Span::styled(branch.name.clone(), style),
                    label,
                ]);
                let branch_name_length = branch_name_line.width();
                if branch_name_length > max_branch_name_length {
                    max_branch_name_length = branch_name_length;
                }
                Row::new(vec![
                    branch_name_line,
                    Line::from(vec![Span::styled(time_ago, TIME_AGO_STYLE)]),
                ])
            })
            .collect();
        if rows.is_empty() {
            rows.push(Row::new(vec![Span::styled(
                "No branches found. Press 'f' to edit the filter.",
                Style::new().italic(),
            )]));
        }

        let max_branch_name_column_width = area.width as f64 * 0.75;
        let branch_name_column_constraint =
            if max_branch_name_length as f64 > max_branch_name_column_width {
                // If branch name is very long, cut off the column width to 75% of the screen
                Constraint::Percentage(75)
            } else if let Ok(value) = max_branch_name_length.try_into() {
                // Use branch length as column width
                // Makes it so that the time ago timestamp isn't all the way on the other side of
                // the screen
                Constraint::Length(value)
            } else {
                // Fallback in case the custom branch name width couldn't be calculated
                Constraint::Percentage(75)
            };
        let widths = [branch_name_column_constraint, Constraint::Percentage(25)];
        let table = Table::new(rows, widths).column_spacing(2);
        StatefulWidget::render(table, area, buffer, &mut self.branches.state);
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

        if self.is_confirm_deletion_mode() {
            Paragraph::new("Confirm branch deletion").render(header_area, buffer);
        } else if self.is_filter_mode() || self.active_filter.is_some() {
            let filter = if let Some(filter) = &self.active_filter {
                filter
            } else {
                &"".to_string()
            };
            Paragraph::new(format!(
                "Filter branches ({}/{}): {}",
                self.branches.included_indexes.len(),
                self.branches.entries.len(),
                filter
            ))
            .render(header_area, buffer);
        } else {
            Paragraph::new(format!("Select branch ({}):", self.branches.entries.len()))
                .render(header_area, buffer);
        }

        self.render_list(list_area, buffer);

        if self.is_confirm_deletion_mode() {
            let popup_area = popup_area(area, 50, 50);
            Clear.render(popup_area, buffer);
            let marked_branches = self.branches.marked_indexes.len();
            let branches_label = if marked_branches == 1 {
                "branch"
            } else {
                "branches"
            };
            let popup = ConfirmPopup {
                title: "Confirm deletion".to_string(),
                content: format!(
                    "Are you sure you want to delete {} {}?",
                    marked_branches, branches_label
                ),
            };
            popup.render(popup_area, buffer);
        }
    }
}

#[derive(Debug, Default)]
struct ConfirmPopup {
    title: String,
    content: String,
}

impl Widget for ConfirmPopup {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::new()
            .title(self.title)
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .borders(Borders::ALL)
            .padding(Padding::uniform(1));

        let inner_rect = block.inner(area);
        let inner_areas = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1), // Spacing between areas
            Constraint::Length(1),
        ])
        .flex(Flex::Center)
        .split(inner_rect);

        let message_area = inner_areas[0];
        let actions_area = inner_areas[2];

        let action_inner_areas =
            Layout::horizontal([Constraint::Length(10), Constraint::Length(10)])
                .flex(Flex::SpaceAround)
                .split(actions_area);
        let yes_area = action_inner_areas[0];
        let no_area = action_inner_areas[1];

        Paragraph::new(Text::from(self.content))
            .wrap(Wrap { trim: true })
            .centered()
            .render(message_area, buf);

        Paragraph::new("Yes (y)").centered().render(yes_area, buf);
        Paragraph::new("No (n)").centered().render(no_area, buf);

        block.render(area, buf);
    }
}

// Area in the center of the viewport
fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}
