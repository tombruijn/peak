use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    text::Text,
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Widget, Wrap},
};

#[derive(Debug, Default)]
pub struct MessagePopup {
    pub title: String,
    pub content: String,
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
            Constraint::Length(1),
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
pub struct ConfirmPopup {
    pub title: String,
    pub content: String,
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
            Constraint::Length(1),
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

pub fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}
