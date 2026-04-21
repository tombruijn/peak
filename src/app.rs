use chrono::{TimeZone, Utc};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use git2::{BranchType, Repository};
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph, Row, StatefulWidget, Table, TableState, Widget, Wrap},
};

use crate::structs::{Alert, Branch, Branches, DisplayMode};
use crate::utils::{pluralize, time_ago_in_words};
use crate::widgets::{ConfirmPopup, MessagePopup, popup_area};

const UI_HEIGHT: u16 = 1;
const CURRENT_BRANCH_STYLE: Style = Style::new().fg(Color::Green);
const NORMAL_BRANCH_STYLE: Style = Style::new();
const ARROW_STYLE: Style = Style::new().fg(Color::Blue);
const AUTHOR_NAME_STYLE: Style = Style::new().fg(Color::Magenta);
const TIME_AGO_STYLE: Style = Style::new().fg(Color::DarkGray);

pub struct App {
    repository: Repository,
    viewport_height: u16,
    display_mode: DisplayMode,
    alert: Option<Alert>,
    active_filter: Option<String>,
    cursor_index: Option<usize>,
    pub branches: Branches,
    help_state: TableState,
    debug_text: Option<String>,
}

impl App {
    pub fn new(viewport_height: u16) -> Result<Self> {
        let mut app = Self {
            repository: Repository::init(".")?,
            viewport_height,
            display_mode: DisplayMode::default(),
            alert: None,
            active_filter: None,
            cursor_index: Some(0),
            branches: Branches {
                entries: vec![],
                included_indexes: vec![],
                marked_indexes: vec![],
                branch_type: Some(BranchType::Local),
                state: TableState::default(),
            },
            help_state: TableState::default(),
            debug_text: None,
        };
        app.load_branches()?;
        app.apply_filter();

        Ok(app)
    }

    fn is_filter_mode(&self) -> bool {
        self.display_mode == DisplayMode::Filter
    }

    fn is_confirm_deletion_mode(&self) -> bool {
        matches!(self.display_mode, DisplayMode::ConfirmDeletion(_))
    }

    fn is_help_mode(&self) -> bool {
        self.display_mode == DisplayMode::Help
    }

    fn switch_to_normal_mode(&mut self) {
        self.display_mode = DisplayMode::Normal;
    }

    fn switch_to_filter_mode(&mut self) {
        self.display_mode = DisplayMode::Filter;
        self.cursor_index = None;
    }

    fn switch_to_confirm_deletion_mode(&mut self) {
        self.display_mode = DisplayMode::ConfirmDeletion(true);
    }

    fn switch_to_help_mode(&mut self) {
        self.display_mode = DisplayMode::Help;
    }

    fn switch_to_confirm_deletion_mode_with_cursor_branch(&mut self) {
        if let Some(branch_index) = self.get_branch_index_for_cursor() {
            let branch = &self.branches.entries[branch_index];
            if branch.current {
                return;
            }
            self.branches.marked_indexes = vec![branch_index];
            self.display_mode = DisplayMode::ConfirmDeletion(false);
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| frame.render_widget(&mut self, frame.area()))?;

            match event::read()? {
                Event::Resize(_columns, rows) => {
                    self.viewport_height = rows;
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let KeyCode::Char('Q') = key.code {
                        return Ok(());
                    }

                    if self.alert.is_some() {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                                self.alert = None;
                            }
                            _ => (),
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(());
                        }
                        _ => {}
                    }

                    match self.display_mode {
                        DisplayMode::Normal => match key.code {
                            KeyCode::Char('h') => {
                                self.switch_to_help_mode();
                            }

                            KeyCode::Char('t') => {
                                if self.branches.branch_type == Some(BranchType::Local) {
                                    self.branches.branch_type = None;
                                } else {
                                    self.branches.branch_type = Some(BranchType::Local);
                                }
                                self.cursor_index = Some(0);
                                *self.branches.state.offset_mut() = 0;
                                self.load_branches()?;
                                self.apply_filter();
                            }

                            KeyCode::Char('/' | 'f') => {
                                self.switch_to_filter_mode();
                            }

                            KeyCode::Esc | KeyCode::Char('q') => {
                                return Ok(());
                            }

                            KeyCode::Enter
                                if self.cursor_index.is_some()
                                    && self.select_current_item().is_ok() =>
                            {
                                if key.modifiers.contains(KeyModifiers::SHIFT) {
                                    return Ok(());
                                } else {
                                    self.refresh_branches();
                                }
                            }

                            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                self.jump_down_section();
                            }
                            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                self.jump_up_section();
                            }

                            KeyCode::Delete | KeyCode::Char('d') => {
                                if self.branches.marked_indexes.is_empty() {
                                    self.switch_to_confirm_deletion_mode_with_cursor_branch();
                                } else {
                                    self.switch_to_confirm_deletion_mode();
                                }
                            }
                            KeyCode::Char('x') => self.mark_current_item(),

                            KeyCode::Char('X') => self.branches.marked_indexes.clear(),

                            KeyCode::Up | KeyCode::Char('k') => self.move_to_previous_item(),

                            KeyCode::Down | KeyCode::Char('j') => self.move_to_next_item(),

                            KeyCode::Char('Z') => {
                                self.active_filter = None;
                                self.apply_filter();
                            }

                            KeyCode::Char('R') => self.refresh_branches(),

                            _ => {}
                        },

                        DisplayMode::Filter => match key.code {
                            KeyCode::Enter => {
                                self.switch_to_normal_mode();
                                if !self.branches.included_indexes.is_empty() {
                                    self.cursor_index = Some(0);
                                }
                            }
                            KeyCode::Esc => {
                                self.active_filter = None;
                                self.apply_filter();
                                self.switch_to_normal_mode();
                                self.cursor_index = Some(0);
                            }
                            KeyCode::Backspace => {
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
                            key_code => {
                                if let Some(char) = key_code.as_char() {
                                    if let Some(filter) = self.active_filter {
                                        self.active_filter = Some(format!("{}{}", filter, char))
                                    } else {
                                        self.active_filter = Some(char.to_string())
                                    };
                                    self.apply_filter();
                                }
                            }
                        },

                        DisplayMode::ConfirmDeletion(_) => match key.code {
                            KeyCode::Char('y' | 'Y') => {
                                self.delete_marked_items()?;
                            }
                            KeyCode::Esc | KeyCode::Char('n' | 'N' | 'q') => {
                                if matches!(self.display_mode, DisplayMode::ConfirmDeletion(false))
                                {
                                    self.branches.marked_indexes.clear();
                                }
                                self.switch_to_normal_mode();
                            }

                            _ => {}
                        },

                        DisplayMode::Help => match key.code {
                            KeyCode::Esc | KeyCode::Char('q' | 'h') => {
                                self.switch_to_normal_mode();
                            }

                            KeyCode::Up | KeyCode::Char('k') => {
                                self.help_state.select_previous();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                self.help_state.select_next();
                            }
                            _ => {}
                        },
                    }
                }

                _ => {}
            }
        }
    }

    fn move_to_previous_item(&mut self) {
        if self.branches.included_indexes.is_empty() {
            return;
        }

        if let Some(cursor_index) = self.cursor_index {
            let current_offset = self.branches.state.offset();
            let new_cursor_index = cursor_index.checked_sub(1);
            if let Some(new_cursor_index) = new_cursor_index {
                if new_cursor_index <= current_offset {
                    let new_offset = current_offset.saturating_sub(1);
                    *self.branches.state.offset_mut() = new_offset;
                }
                self.cursor_index = Some(new_cursor_index);
            } else {
                let list_height = (self.viewport_height - UI_HEIGHT) as usize;
                let branches_len = self.branches.included_indexes.len();
                let new_cursor_index = branches_len - 1;
                if let Some(new_offset) = branches_len.checked_sub(list_height) {
                    *self.branches.state.offset_mut() = new_offset;
                }
                self.cursor_index = Some(new_cursor_index);
            }
        } else {
            self.cursor_index = Some(0);
            *self.branches.state.offset_mut() = 0;
        }
    }

    fn move_to_next_item(&mut self) {
        if self.branches.included_indexes.is_empty() {
            return;
        }

        if let Some(cursor_index) = self.cursor_index {
            let new_cursor_index = cursor_index + 1;
            let branch_length = self.branches.included_indexes.len();
            if new_cursor_index < branch_length {
                let current_offset = self.branches.state.offset();
                let list_height = (self.viewport_height - 1 - UI_HEIGHT) as usize;
                let viewport_end = current_offset + list_height;
                if new_cursor_index >= viewport_end {
                    *self.branches.state.offset_mut() = if new_cursor_index == branch_length - 1 {
                        current_offset
                    } else {
                        current_offset + 1
                    }
                }
                self.cursor_index = Some(new_cursor_index);
            } else {
                self.cursor_index = Some(0);
                *self.branches.state.offset_mut() = 0;
            }
        } else {
            self.cursor_index = Some(0);
            *self.branches.state.offset_mut() = 0;
        }
    }

    fn half_screen_height(&self) -> usize {
        (self.viewport_height / 2) as usize
    }

    fn jump_up_section(&mut self) {
        if self.branches.included_indexes.is_empty() {
            return;
        }

        if let Some(cursor_index) = self.cursor_index {
            let half_screen_height = self.half_screen_height();
            let current_offset = self.branches.state.offset();
            let new_cursor_index = cursor_index.checked_sub(half_screen_height);

            if let Some(new_cursor_index) = new_cursor_index {
                if new_cursor_index <= current_offset {
                    *self.branches.state.offset_mut() = new_cursor_index;
                }
                self.cursor_index = Some(new_cursor_index + 1);
            } else {
                self.cursor_index = Some(0);
                *self.branches.state.offset_mut() = 0;
            }
        }
    }

    fn jump_down_section(&mut self) {
        if self.branches.included_indexes.is_empty() {
            return;
        }

        if let Some(cursor_index) = self.cursor_index {
            let new_cursor_index = cursor_index + self.half_screen_height();
            let included_branch_len = self.branches.included_indexes.len();
            let list_height = (self.viewport_height - 1 - UI_HEIGHT) as usize;

            if new_cursor_index < included_branch_len {
                let current_offset = self.branches.state.offset();
                if new_cursor_index >= (current_offset + list_height) {
                    *self.branches.state.offset_mut() = new_cursor_index - list_height;
                }
                self.cursor_index = Some(new_cursor_index - 1);
            } else {
                let new_cursor_index = included_branch_len - 1;
                self.cursor_index = Some(new_cursor_index);
                *self.branches.state.offset_mut() = new_cursor_index - list_height;
            }
        }
    }

    fn select_current_item(&mut self) -> Result<(), ()> {
        if let Some(cursor_index) = self.cursor_index {
            let branch_index = self.branches.included_indexes[cursor_index];
            let branch_entry = &self.branches.entries[branch_index];
            let branch_name = branch_entry.name.clone();

            if let Ok(branch) = self
                .repository
                .find_branch(&branch_name, branch_entry.branch_type)
            {
                let commit = match branch.get().peel_to_commit() {
                    Ok(c) => c,
                    Err(err) => {
                        self.alert = Some(Alert {
                            title: "Can't checkout branch".to_string(),
                            message: format!(
                                "Can't get commit for '{branch_name}' branch: {}",
                                err.message()
                            ),
                        });
                        return Err(());
                    }
                };

                let local_branch = match branch_entry.branch_type {
                    BranchType::Local => branch,
                    BranchType::Remote => {
                        let remote_branch_name = branch_name.clone();
                        let local_branch_name = remote_branch_name
                            .strip_prefix("refs/remotes/")
                            .unwrap_or(&remote_branch_name);
                        let local_branch_name = match local_branch_name.split_once("/") {
                            Some((_remote, name)) => name.to_string(),
                            None => {
                                self.alert = Some(Alert {
                                    title: "Can't normalize branch name".to_string(),
                                    message:
                                        "Can't find normalize branch name. Not a remote branch?"
                                            .to_string(),
                                });
                                return Err(());
                            }
                        };
                        if let Ok(local_branch) = self
                            .repository
                            .find_branch(&local_branch_name, BranchType::Local)
                        {
                            local_branch
                        } else {
                            match self.repository.branch(&local_branch_name, &commit, false) {
                                Ok(mut new_local_branch) => {
                                    if let Err(err) =
                                        new_local_branch.set_upstream(Some(&remote_branch_name))
                                    {
                                        self.alert = Some(Alert {
                                            title: "Can't set upstream of branch".to_string(),
                                            message: format!(
                                                "Can't set upstream of new local '{local_branch_name}' branch branch: {}",
                                                err.message()
                                            ),
                                        });
                                        return Err(());
                                    }
                                    new_local_branch
                                }
                                Err(err) => {
                                    self.alert = Some(Alert {
                                        title: "Can't checkout branch".to_string(),
                                        message: format!(
                                            "Can't set head to '{local_branch_name}' branch: {}",
                                            err.message()
                                        ),
                                    });
                                    return Err(());
                                }
                            }
                        }
                    }
                };

                if let Err(err) = self.repository.checkout_tree(commit.as_object(), None) {
                    self.alert = Some(Alert {
                        title: "Can't checkout branch".to_string(),
                        message: format!(
                            "Can't checkout '{branch_name}' branch: {}",
                            err.message()
                        ),
                    });
                    return Err(());
                }

                let reference = local_branch.into_reference();
                let branch_name = reference.name().unwrap();
                if let Err(err) = self.repository.set_head(branch_name) {
                    self.alert = Some(Alert {
                        title: "Can't checkout branch".to_string(),
                        message: format!(
                            "Can't set head to '{branch_name}' branch: {}",
                            err.message()
                        ),
                    });
                    return Err(());
                }

                Ok(())
            } else {
                self.alert = Some(Alert {
                    title: "Can't checkout branch".to_string(),
                    message: format!("Can't find '{branch_name}' branch"),
                });
                Err(())
            }
        } else {
            panic!("This should not happen: branch checkout without branch selected");
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
        self.branches.marked_indexes.clear();
        let offset = self.branches.state.offset();
        *self.branches.state.offset_mut() = if offset > 0 { offset - 1 } else { 0 };
        if let Some(cursor_index) = self.cursor_index {
            self.cursor_index = if let Some(new_cursor_index) = cursor_index.checked_sub(1) {
                Some(new_cursor_index)
            } else {
                Some(0)
            }
        }
        self.switch_to_normal_mode();
        self.load_branches()?;
        self.apply_filter();
        Ok(())
    }

    fn mark_current_item(&mut self) {
        if let Some(branch_index) = self.get_branch_index_for_cursor() {
            let branch = &self.branches.entries[branch_index];
            if branch.current {
                return;
            }

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

    fn get_branch_index_for_cursor(&self) -> Option<usize> {
        if let Some(cursor_index) = self.cursor_index {
            let branch_index = self.branches.included_indexes[cursor_index];
            Some(branch_index)
        } else {
            None
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

    fn refresh_branches(&mut self) {
        match self.load_branches() {
            Ok(()) => {
                if let Some(cursor_index) = self.cursor_index
                    && cursor_index >= self.branches.entries.len()
                {
                    self.cursor_index = Some(0);
                }
            }
            Err(err) => {
                self.alert = Some(Alert {
                    title: "Error fetching branches".to_string(),
                    message: format!("An error occurred: {}", err),
                });
            }
        }
    }

    fn load_branches(&mut self) -> Result<()> {
        self.branches.entries = self
            .repository
            .branches(self.branches.branch_type)?
            .filter_map(|branch_item| {
                if let Ok((branch, branch_type)) = branch_item {
                    let name = if let Ok(Some(name)) = branch.name() {
                        name.to_string()
                    } else {
                        return None;
                    };
                    let commit = branch.get().peel_to_commit().ok()?;
                    let author = commit.author().name()?.to_string();
                    let last_commit_at = Utc.timestamp_opt(commit.time().seconds(), 0).single()?;

                    Some(Branch {
                        name,
                        branch_type,
                        author,
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
            .sort_by_key(|b| std::cmp::Reverse(b.last_commit_at));
        Ok(())
    }

    fn render_list(&mut self, area: Rect, buffer: &mut Buffer) {
        let now = Utc::now();
        let mut max_branch_name_length = 0;
        let mut max_author_name_length = 0;
        let rows: Vec<Row> = self
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
                    (NORMAL_BRANCH_STYLE, Span::raw("  "))
                };
                let duration = now.signed_duration_since(branch.last_commit_at);
                let time_ago = time_ago_in_words(duration);

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
                let author_name_line =
                    Line::from(vec![Span::styled(branch.author.clone(), AUTHOR_NAME_STYLE)]);
                let author_name_length = author_name_line.width();
                if author_name_length > max_author_name_length {
                    max_author_name_length = author_name_length;
                }
                Row::new(vec![
                    branch_name_line,
                    author_name_line,
                    Line::from(vec![Span::styled(time_ago, TIME_AGO_STYLE)]),
                ])
            })
            .collect();
        if rows.is_empty() {
            let no_branches_message = if self.is_filter_mode() {
                "No branches found."
            } else {
                "No branches found. Press 'f' to edit the filter or 'R' to reset it."
            };
            Paragraph::new(no_branches_message)
                .italic()
                .wrap(Wrap { trim: true })
                .render(area, buffer);
        } else {
            let max_branch_name_column_width = area.width as f64 * 0.75;
            let branch_name_column_constraint =
                if max_branch_name_length as f64 > max_branch_name_column_width {
                    Constraint::Percentage(50)
                } else if let Ok(value) = max_branch_name_length.try_into() {
                    Constraint::Length(value)
                } else {
                    Constraint::Percentage(50)
                };
            let widths = vec![
                branch_name_column_constraint,
                Constraint::Length(max_author_name_length as u16),
                Constraint::Percentage(25),
            ];
            let table = Table::new(rows, widths).column_spacing(2);
            StatefulWidget::render(table, area, buffer, &mut self.branches.state);
        }
    }

    fn render_ui(&mut self, area: Rect, buffer: &mut Buffer) {
        if self.is_confirm_deletion_mode() {
            Span::default().render(area, buffer)
        } else {
            let right_column_with = 20 + self.debug_text.as_ref().map_or(0, |s| s.len() as u16);
            let ui_layout = Layout::horizontal([
                Constraint::Percentage(100),
                Constraint::Min(right_column_with),
            ])
            .split(area);
            let left_column = ui_layout[0];
            let right_column = ui_layout[1];

            if self.is_filter_mode() || self.active_filter.is_some() {
                let filter = if let Some(filter) = &self.active_filter {
                    filter
                } else {
                    &"".to_string()
                };
                let mut text = vec![
                    Span::from(format!(
                        "Filter branches ({}/{}): ",
                        self.branches.included_indexes.len(),
                        self.branches.entries.len(),
                    )),
                    Span::from(filter.to_string()),
                ];
                if self.is_filter_mode() {
                    text.push(Span::from("_".to_string()).slow_blink());
                }
                Line::from(text).render(left_column, buffer);
                Paragraph::new("Press (Enter) to submit")
                    .right_aligned()
                    .render(right_column, buffer);
            } else {
                let mut spans = vec![Span::raw(pluralize(
                    self.branches.entries.len() as i64,
                    "branch",
                    "branches",
                ))];
                let selected = self.branches.marked_indexes.len();
                if selected > 0 {
                    spans.insert(0, Span::raw(format!("{selected}/")));
                    spans.push(Span::raw(" selected"));
                }
                Paragraph::new(Line::from(spans)).render(left_column, buffer);
                let mut help_spans = vec![Span::raw("Press (h) for help")];
                if let Some(debug_text) = &self.debug_text {
                    help_spans.push(Span::raw(" | "));
                    help_spans.push(Span::from(debug_text));
                }
                Paragraph::new(Line::from(help_spans))
                    .right_aligned()
                    .render(right_column, buffer);
            }
        }
    }

    fn render_help(&mut self, area: Rect, buffer: &mut Buffer) {
        let areas = Layout::vertical([Constraint::Min(1), Constraint::Percentage(100)]).split(area);
        Block::new()
            .title("Help reference")
            .title_alignment(Alignment::Center)
            .title_style(Style::new().bold())
            .render(areas[0], buffer);

        let rows = vec![
            Row::new(vec![Text::from("Help view"), Text::default()])
                .cyan()
                .bold(),
            Row::new(vec![
                Text::from("Keys").right_aligned(),
                Text::from("Description"),
            ])
            .bold(),
            Row::new(vec![
                Text::from("q | h").right_aligned(),
                Text::from("Exit help view"),
            ]),
            Row::new(vec![
                Text::from("UP | k").right_aligned(),
                Text::from("Move cursor up one line"),
            ]),
            Row::new(vec![
                Text::from("DOWN | j").right_aligned(),
                Text::from("Move cursor down one line"),
            ]),
            Row::new(vec![Text::from("Main view"), Text::default()])
                .cyan()
                .bold()
                .top_margin(1),
            Row::new(vec![
                Text::from("Keys").right_aligned(),
                Text::from("Description"),
            ])
            .bold(),
            Row::new(vec![
                Text::from("h").right_aligned(),
                Text::from("Toggle help page"),
            ]),
            Row::new(vec![
                Text::from("q | ESC").right_aligned(),
                Text::from("Quit UI or app"),
            ]),
            Row::new(vec![
                Text::from("UP | k").right_aligned(),
                Text::from("Move cursor up one line"),
            ]),
            Row::new(vec![
                Text::from("DOWN | j").right_aligned(),
                Text::from("Move cursor down one line"),
            ]),
            Row::new(vec![
                Text::from("Ctrl + u").right_aligned(),
                Text::from("Move cursor up half the screen"),
            ]),
            Row::new(vec![
                Text::from("Ctrl + d").right_aligned(),
                Text::from("Move cursor down half the screen"),
            ]),
            Row::new(vec![
                Text::from("Enter").right_aligned(),
                Text::from("Check out branch"),
            ]),
            Row::new(vec![
                Text::from("f | /").right_aligned(),
                Text::from("Focus branch filter"),
            ]),
            Row::new(vec![
                Text::from("R").right_aligned(),
                Text::from("Reset branch filter"),
            ]),
            Row::new(vec![
                Text::from("x").right_aligned(),
                Text::from("Mark branch"),
            ]),
            Row::new(vec![
                Text::from("X").right_aligned(),
                Text::from("Clear all marked branches"),
            ]),
            Row::new(vec![
                Text::from("d | DELETE").right_aligned(),
                Text::from("Delete marked branches"),
            ]),
            Row::new(vec![
                Text::from("t").right_aligned(),
                Text::from("Toggle showing remote branches"),
            ]),
            Row::new(vec![Text::from("Branch filter mode"), Text::default()])
                .cyan()
                .bold()
                .top_margin(1),
            Row::new(vec![
                Text::from("Keys").right_aligned(),
                Text::from("Description"),
            ])
            .bold(),
            Row::new(vec![
                Text::from("Enter").right_aligned(),
                Text::from("Submit filter"),
            ]),
            Row::new(vec![
                Text::from("ESC").right_aligned(),
                Text::from("Discard filter"),
            ]),
        ];
        let widths = [Constraint::Length(20), Constraint::Percentage(100)];

        let selected_row_style = Style::new().fg(Color::White).reversed();
        let table = Table::new(rows, widths)
            .column_spacing(2)
            .row_highlight_style(selected_row_style);
        StatefulWidget::render(table, areas[1], buffer, &mut self.help_state);
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if self.is_help_mode() {
            self.render_help(area, buffer);
            return;
        }

        let [ui_area, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);

        self.render_list(list_area, buffer);
        self.render_ui(ui_area, buffer);

        if self.is_confirm_deletion_mode() {
            let popup_area = popup_area(area, 50, 50);
            Clear.render(popup_area, buffer);
            let marked_branches = self.branches.marked_indexes.len();
            let popup = ConfirmPopup {
                title: " Confirm deletion ".to_string(),
                content: format!(
                    "Are you sure you want to delete {}?",
                    pluralize(marked_branches as i64, "branch", "branches")
                ),
            };
            popup.render(popup_area, buffer);
        }

        if let Some(alert) = &self.alert {
            let popup_area = popup_area(area, 50, 50);
            Clear.render(popup_area, buffer);
            let popup = MessagePopup {
                title: alert.title.to_string(),
                content: alert.message.to_string(),
            };
            popup.render(popup_area, buffer);
        }
    }
}
