use chrono::{DateTime, Duration, TimeZone, Utc};
use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal,
};
use git2::{BranchType, Repository};
use ratatui::{
    DefaultTerminal,
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
const AUTHOR_NAME_STYLE: Style = Style::new().fg(Color::Magenta);
const TIME_AGO_STYLE: Style = Style::new().fg(Color::DarkGray);

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let (_terminal_columns, terminal_rows) = terminal::size()?;
    let terminal = ratatui::init();
    let result = App::new(terminal_rows)?.run(terminal);
    ratatui::restore();
    result
}

#[derive(Debug)]
struct Branch {
    name: String,
    branch_type: BranchType,
    author: String,
    current: bool,
    last_commit_at: DateTime<Utc>,
}

#[derive(Debug)]
struct Branches {
    entries: Vec<Branch>,
    included_indexes: Vec<usize>,
    marked_indexes: Vec<usize>,
    branch_type: Option<BranchType>,
    state: TableState,
}

#[derive(Debug, Default, PartialEq)]
enum DisplayMode {
    #[default]
    Normal,
    Help,
    Filter,
    ConfirmDeletion(bool), // First value is 'multi selection mode' yes or no
}

struct Alert {
    title: String,
    message: String,
}

pub struct App {
    repository: Repository,
    viewport_height: u16,
    display_mode: DisplayMode,
    alert: Option<Alert>,
    active_filter: Option<String>,
    cursor_index: Option<usize>,
    branches: Branches,
    help_state: TableState,
    debug_text: Option<String>,
}

impl App {
    fn new(viewport_height: u16) -> Result<Self> {
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
                // Can't mark current branch for deletion
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
                    // Use 'Q' to quit in any mode
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
                        continue; // Ignore any other key listeners defined below
                    }

                    // Global keys
                    match key.code {
                        // Force quit with CTRL+C at all times
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(());
                        }
                        _ => {}
                    }

                    match self.display_mode {
                        DisplayMode::Normal => {
                            match key.code {
                                // Show help
                                KeyCode::Char('h') => {
                                    self.switch_to_help_mode();
                                }

                                // Switch branch types
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

                                // Switch to filter mode
                                KeyCode::Char('/' | 'f') => {
                                    self.switch_to_filter_mode();
                                }

                                // Exit in normal mode
                                KeyCode::Esc | KeyCode::Char('q') => {
                                    return Ok(());
                                }

                                // Switch to the branch on which the line cursor is
                                KeyCode::Enter => {
                                    if self.cursor_index.is_some()
                                        && self.select_current_item().is_ok()
                                    {
                                        // Exit if Shift + Enter is pressed
                                        let switch_and_exit =
                                            key.modifiers.contains(KeyModifiers::SHIFT);
                                        if switch_and_exit {
                                            return Ok(());
                                        } else {
                                            self.refresh_branches();
                                        }
                                    }
                                }

                                // Jump chunk
                                KeyCode::Char('d')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    self.jump_down_section();
                                }
                                // Jump chunk
                                KeyCode::Char('u')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    self.jump_up_section();
                                }

                                // Delete the marked branches
                                // But first show a confirmation prompt
                                KeyCode::Delete | KeyCode::Char('d') => {
                                    if self.branches.marked_indexes.is_empty() {
                                        self.switch_to_confirm_deletion_mode_with_cursor_branch();
                                    } else {
                                        self.switch_to_confirm_deletion_mode();
                                    }
                                }
                                // Mark an item for deletion
                                KeyCode::Char('x') => self.mark_current_item(),

                                // Reset selection
                                KeyCode::Char('X') => self.branches.marked_indexes.clear(),

                                // Move up to the branch above
                                // If on the first line, wrap around to the end
                                KeyCode::Up | KeyCode::Char('k') => self.move_to_previous_item(),

                                // Move down to the branch below
                                // If on the last line, wrap around to the beginning
                                KeyCode::Down | KeyCode::Char('j') => self.move_to_next_item(),

                                // Reset filter
                                KeyCode::Char('Z') => {
                                    self.active_filter = None;
                                    self.apply_filter();
                                }

                                // Refresh branch list
                                KeyCode::Char('R') => self.refresh_branches(),

                                _ => {}
                            }
                        }

                        DisplayMode::Filter => {
                            match key.code {
                                // Submit filter
                                KeyCode::Enter => {
                                    self.switch_to_normal_mode();
                                    if !self.branches.included_indexes.is_empty() {
                                        self.cursor_index = Some(0);
                                    }
                                }
                                // Dismiss and reset filter
                                KeyCode::Esc => {
                                    self.active_filter = None;
                                    self.apply_filter();
                                    self.switch_to_normal_mode();
                                    self.cursor_index = Some(0);
                                }
                                // Remove last character of the filter
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
                                // Add character to filter
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
                            }
                        }

                        DisplayMode::ConfirmDeletion(_) => {
                            match key.code {
                                // Confirm branch deletion
                                KeyCode::Char('y' | 'Y') => {
                                    self.delete_marked_items()?;
                                }
                                // Exit branch deletion
                                KeyCode::Esc | KeyCode::Char('n' | 'N' | 'q') => {
                                    if matches!(
                                        self.display_mode,
                                        DisplayMode::ConfirmDeletion(false)
                                    ) {
                                        self.branches.marked_indexes.clear();
                                    }
                                    self.switch_to_normal_mode();
                                }

                                _ => {}
                            }
                        }

                        DisplayMode::Help => {
                            match key.code {
                                // Exit help
                                KeyCode::Esc | KeyCode::Char('q' | 'h') => {
                                    self.switch_to_normal_mode();
                                }

                                // Move up a line
                                KeyCode::Up | KeyCode::Char('k') => {
                                    self.help_state.select_previous();
                                }
                                // Move down a line
                                KeyCode::Down | KeyCode::Char('j') => {
                                    self.help_state.select_next();
                                }
                                _ => {}
                            }
                        }
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
                // Move up one line normally
                if new_cursor_index <= current_offset {
                    // Update offset to show line with cursor
                    let new_offset = current_offset.saturating_sub(1);
                    *self.branches.state.offset_mut() = new_offset;
                }
                self.cursor_index = Some(new_cursor_index);
            } else {
                // Wrap cursor to bottom of the list
                let list_height = (self.viewport_height - UI_HEIGHT) as usize;
                let branches_len = self.branches.included_indexes.len();
                let new_cursor_index = branches_len - 1; // -1 because 0 index
                // Calculate new offset so the end of the list is visible
                // If the last list item is already visible (the new offset would be
                // negative), do nothing.
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
                // -1 for 0 based index
                let list_height = (self.viewport_height - 1 - UI_HEIGHT) as usize;
                let viewport_end = current_offset + list_height;
                // If the cursor index moves lower than the viewport end
                if new_cursor_index >= viewport_end {
                    // Update the viewport location
                    *self.branches.state.offset_mut() = if new_cursor_index == branch_length - 1 {
                        // -1 because of zero based index
                        // Put cursor at the bottom of the viewport to make it clear there's
                        // nothing more in the list
                        current_offset
                    } else {
                        // +1 to keep a line below the cursor
                        // Make it clear there's more items lower in the list
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
        // Nothing to navigate: skip this behavior
        if self.branches.included_indexes.is_empty() {
            return;
        }

        if let Some(cursor_index) = self.cursor_index {
            // Try to move up half the screen
            let half_screen_height = self.half_screen_height();
            let current_offset = self.branches.state.offset();
            let new_cursor_index = cursor_index.checked_sub(half_screen_height);

            // Is the new cursor index a positive number, can we navigate to it?
            if let Some(new_cursor_index) = new_cursor_index {
                // Move up the viewport if the cursor would go outside the
                // visible area
                if new_cursor_index <= current_offset {
                    // Update offset to show line with cursor
                    *self.branches.state.offset_mut() = new_cursor_index;
                }
                // +1 so we're not on the first line of the viewport, like the normal single line
                // up behavior
                self.cursor_index = Some(new_cursor_index + 1);
            } else {
                // If reached the top, because the new index would be a negative
                // number, set the cursor and offset to the top
                self.cursor_index = Some(0);
                *self.branches.state.offset_mut() = 0;
            }
        }
    }

    fn jump_down_section(&mut self) {
        // Nothing to navigate: skip this behavior
        if self.branches.included_indexes.is_empty() {
            return;
        }

        if let Some(cursor_index) = self.cursor_index {
            // Try to move down half the screen
            let new_cursor_index = cursor_index + self.half_screen_height();
            let included_branch_len = self.branches.included_indexes.len();
            let list_height = (self.viewport_height - 1 - UI_HEIGHT) as usize;

            // Is the new cursor index within the list range, can we navigate to it?
            if new_cursor_index < included_branch_len {
                let current_offset = self.branches.state.offset();
                // Check if the new cursor position is outside of the visible range
                if new_cursor_index >= (current_offset + list_height) {
                    // If so, update offset to show the line with cursor at the bottom of the
                    // viewport
                    *self.branches.state.offset_mut() = new_cursor_index - list_height;
                }
                // -1 so we're not on the last line of the viewport, like the normal single line
                // down behavior
                self.cursor_index = Some(new_cursor_index - 1);
            } else {
                // We can't navigate to the new cursor position because it's too far, so
                // jump to the last item
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
                        // Strip the remote branch format of the branch name, either
                        // - refs/remote/origin/...
                        // - origin/...
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
                            // If a local branch with the same name as the remote branch already
                            // exists, check that one out directly
                            local_branch
                        } else {
                            // If no local branch exists yet, create a local branch with the name
                            // of the remote branch
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

                // Giving the branch name on the branch_entry directly doesn't work to set the head
                // Convert the branch into a reference and use that name to check out the
                // branch
                // The reference name format is 'refs/heads/<name>'
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
                // TODO: refresh branch list?
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
        self.load_branches()?;
        self.apply_filter();
        Ok(())
    }

    fn mark_current_item(&mut self) {
        if let Some(branch_index) = self.get_branch_index_for_cursor() {
            let branch = &self.branches.entries[branch_index];
            if branch.current {
                // Can't mark current branch for deletion
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
            .sort_by(|a, b| b.last_commit_at.cmp(&a.last_commit_at));
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
                    // Reserve space for the current branch marker
                    // Prevents the UI from jumping around
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
                    // If branch name is very long, cut off the column width to 75% of the screen
                    Constraint::Percentage(50)
                } else if let Ok(value) = max_branch_name_length.try_into() {
                    // Use branch length as column width
                    // Makes it so that the time ago timestamp isn't all the way on the other side of
                    // the screen
                    Constraint::Length(value)
                } else {
                    // Fallback in case the custom branch name width couldn't be calculated
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
            // Help view
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
            // Main view
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
            // Filter view
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

#[derive(Debug, Default)]
struct MessagePopup {
    title: String,
    content: String,
}

impl Widget for MessagePopup {
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

        Paragraph::new(Text::from(self.content))
            .wrap(Wrap { trim: true })
            .centered()
            .render(message_area, buf);

        Paragraph::new("OK").centered().render(actions_area, buf);

        block.render(area, buf);
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

fn time_ago_in_words(duration: Duration) -> String {
    let years = duration.num_weeks() / 52;
    let time_ago = if years > 0 {
        pluralize(years, "year", "years")
    } else {
        let months = duration.num_weeks() / 4;
        if months > 0 {
            pluralize(months, "month", "months")
        } else if duration.num_weeks() > 0 {
            pluralize(duration.num_weeks(), "week", "weeks")
        } else if duration.num_days() > 0 {
            pluralize(duration.num_days(), "day", "days")
        } else if duration.num_hours() > 0 {
            pluralize(duration.num_hours(), "hour", "hours")
        } else {
            pluralize(duration.num_minutes(), "minute", "minutes")
        }
    };
    format!("{time_ago} ago")
}

fn pluralize(number: i64, singular: &'static str, plural: &'static str) -> String {
    let label = if number == 1 { singular } else { plural };
    format!("{} {}", number, label)
}
