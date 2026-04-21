mod app;
mod structs;
mod utils;
mod widgets;

use color_eyre::Result;
use crossterm::terminal;

fn main() -> Result<()> {
    color_eyre::install()?;
    let (_terminal_columns, terminal_rows) = terminal::size()?;
    let terminal = ratatui::init();
    let result = app::App::new(terminal_rows)?.run(terminal);
    ratatui::restore();
    result
}
