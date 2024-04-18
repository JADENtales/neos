use color_eyre::{
    eyre::{bail, WrapErr},
    Result,
};
use crossterm::{event::{self, poll, Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind}, ExecutableCommand};
use ratatui::{
    prelude::*,
    symbols::border,
    widgets::{
        block::{Position, Title},
        *,
    },
};
use std::{io, str::FromStr, time::Duration};
use std::{fs, io::{Read, Seek, SeekFrom}, thread, time};
use std::fs::File;
use regex::Regex;

mod errors;
mod tui;

#[derive(Debug, Default)]
pub struct App<'a> {
    exit: bool,
    file_size: u64,
    vertical: bool,
    chats: [bool; 6],
    titles: [&'a str; 6],
    messages: [Vec<(String, String)>; 6],
    scroll: [u16; 6],
}

impl<'a> App<'a> {
    pub fn run() -> Result<()> {
        errors::install_hooks()?;
        let mut terminal = tui::init()?;
        App::default().main_loop(&mut terminal)?;
        tui::restore()?;
        Ok(())
    }

    /// runs the application's main loop until the user quits
    fn main_loop(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        std::io::stdout().execute(crossterm::event::EnableMouseCapture).unwrap();
        self.chats.iter_mut().for_each(|e| *e = true);
        self.titles = ["All(1)", "Public(2)", "Private(3)", "Team(4)", "Club(5)", "System(6)"];
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.read().unwrap();
            self.handle_events().wrap_err("handle events failed")?;
        }
        Ok(())
    }

    fn render_frame(&mut self, frame: &mut Frame) {
        let descriptions = vec![
            Line::from("1: All, 2: Public, 3: Private, 4: Team, 5: Club, 6: System"),
            Line::from("space: 分割方向切替, q: 終了"),
        ];
        let height = Into::<Text>::into(descriptions.clone()).height() as u16 * 2;
        let parents = Layout::default().direction(Direction::Vertical).constraints(vec![Constraint::Length(height), Constraint::Percentage(100)]).split(frame.size());
        frame.render_widget(Paragraph::new(descriptions).block(Block::new().title(Title::from("")).borders(Borders::ALL)), parents[0]);

        let chats_count = self.chats.iter().filter(|e| **e).collect::<Vec<&bool>>().len() as u16;
        let percentage = 100 / chats_count as u16;
        let mut constraints = Vec::new();
        let mut remainder = 100 - percentage * chats_count;
        for _ in 0..chats_count {
            if 0 < remainder {
                constraints.push(Constraint::Percentage(percentage + 1));
                remainder -= 1;
            } else {
                constraints.push(Constraint::Percentage(percentage));
            }
        }
        let children = if self.vertical {
            Layout::default().direction(Direction::Vertical).constraints(constraints).split(parents[1])
        } else {
            Layout::default().direction(Direction::Horizontal).constraints(constraints).split(parents[1])
        };
        let mut index = 0;
        for (i, chat) in self.chats.iter().enumerate() {
            if *chat {
                let texts = self.messages[i].iter().map(|e| Line::from(e.0.as_str()).fg(Color::from_str(e.1.as_str()).unwrap())).collect::<Vec<_>>();
                let row_count = texts.len();
                if 2 <= children[index].height && children[index].height - 2 < row_count as u16 {
                    self.scroll[i] = row_count as u16 - (children[index].height - 2);
                }
                frame.render_widget(
                    Paragraph::new(texts).scroll((self.scroll[i], 0)).wrap(Wrap { trim: false }).block(Block::new().title(Title::from(self.titles[i])).borders(Borders::ALL)),
                    children[index]);
                if 2 <= children[index].height && children[index].height - 2 < row_count as u16 {
                    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(Some("↑")).end_symbol(Some("↓"));
                    let mut scrollbar_state = ScrollbarState::new(row_count - (children[index].height as usize - 2)).position(self.scroll[i] as usize);
                    frame.render_stateful_widget(scrollbar, children[index].inner(&Margin { vertical: 1, horizontal: 0 }), &mut scrollbar_state);
                }
                index += 1;
            }
        }
    }

    fn read(&mut self) -> Result<()> {
        let path = "C:\\Nexon\\TalesWeaver\\ChatLog\\TWChatLog_2024_04_18.html";
        let mut file = File::open(path)?;
        let file_size = fs::metadata(path)?.len();  

        let diff = file_size - self.file_size;
        if self.file_size == 0 || diff == 0 {
            self.file_size = file_size;
            return Ok(());
        }
        self.file_size = file_size;
        file.seek(SeekFrom::End(-(diff as i64)))?;
        let mut content = vec![0; diff as usize];
        file.read(&mut content)?;
        let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(&content);
        let message = cow.into_owned();
        let messages: Vec<_> = message.split("\r\n").filter(|e| e.trim() != "").collect();
        let regex = Regex::new(r##" <font.+color="(.+)">(.+)</font></br>$"##).unwrap();
        for message in messages {
            match regex.captures(&message) {
                Some(captures) => {
                    let color = &captures[1];
                    let message = &captures[2].replace("&nbsp", " ");
                    let i = match color {
                        "#c8ffc8" => 1,
                        "#64ff64" => 2,
                        "#f7b73c" => 3,
                        "#94ddfa" => 4,
                        "#ff64ff" | "#ff6464" => 5,
                        _ => bail!("invalid captured color")
                    };
                    self.messages[0].push(((*message).clone(), color.to_string()));
                    self.messages[i].push(((*message).clone(), color.to_string()));
                }
                _ => bail!("regex does not match."),
            }
        }
        Ok(())
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self) -> Result<()> {
        if poll(Duration::from_millis(1))? {
            match event::read()? {
                // it's important to check that the event is a key press event as
                // crossterm also emits key release and repeat events on Windows.
                Event::Key(event) if event.kind == KeyEventKind::Press => {
                    self.handle_key_event(event).wrap_err_with(|| {
                        format!("handling key event failed:\n{event:#?}")
                    })
                }
                Event::Mouse(event) => self.handle_mouse_event(event),
                _ => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Char(char) if '1' <= char && char <= '6' => {
                let i = (char.to_digit(10).unwrap() - 1) as usize;
                if self.chats.iter().filter(|e| **e).collect::<Vec<&bool>>().len() == 1 && self.chats[i] {
                    return Ok(())
                }
                self.chats[i] = !self.chats[i];
            }
            KeyCode::Char(' ') => self.vertical = !self.vertical,
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse_event(&mut self, event: MouseEvent) -> Result<()> {
        const SCROLL: u16 = 5;
        match event.kind {
            MouseEventKind::ScrollUp => {
                if SCROLL <= self.scroll[0] {
                    self.scroll[0] -= SCROLL;
                } else {
                    self.scroll[0] = 0;
                }
                return Ok(());
            },
            MouseEventKind::ScrollDown => {
                if self.scroll[0] + SCROLL < self.messages[0].len() as u16 {
                    self.scroll[0] += SCROLL;
                } else {
                    self.scroll[0] = (self.messages[0].len() - 1) as u16;
                }
            },
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_key_event() {
    }
}
