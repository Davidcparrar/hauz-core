//! `hauz-tui [--db <path>]`: thin shell. Args, then `load` (the DB opens before the
//! terminal is touched), then the key loop. Untested by design; the non-terminal paths are
//! covered via `tests/e2e_cli.rs`.

mod args;

use std::env;

use crossterm::event::{self, Event, KeyEventKind};
use hauz_tui::{App, Flow, draw, load};

use args::{Command, USAGE, parse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(command) = parse(env::args_os().skip(1)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    match command {
        Command::Help => {
            println!("{USAGE}");
            Ok(())
        }
        Command::Browse { db } => {
            let app = load(&db).await?;
            let mut terminal = ratatui::init();
            let result = run(&mut terminal, app);
            ratatui::restore();
            result
        }
    }
}

fn run(terminal: &mut ratatui::DefaultTerminal, mut app: App) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && app.on_key(key.code) == Flow::Quit
        {
            return Ok(());
        }
    }
}
