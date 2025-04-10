use crate::Counter;
use anyhow::Result;
use crossbeam_channel::Receiver;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Status {
    pub counter: Counter,
    pub rx: Receiver<(usize, usize)>,
    pub t_begin: Instant,
    pub run_duration: Duration,
    pub run_num: usize,
    pub camp_num: usize,
    pub buffer_len: usize,
    pub exit: Option<StatusExit>,
}

#[derive(Debug, Clone, Copy)]
pub enum StatusExit {
    Quit,
    Timeout,
}

impl Status {
    pub fn run(
        &mut self,
        terminal: &mut Arc<Mutex<DefaultTerminal>>,
        shutdown: Arc<AtomicBool>,
        rx: Receiver<()>,
    ) -> Result<StatusExit> {
        while self.exit.is_none() {
            if rx.recv().is_err() {
                break;
            }
            while let Ok(size) = self.rx.try_recv() {
                self.counter.increment(size.0);
                self.buffer_len = size.1;
            }
            self.handle_events()?;
            if self.t_begin.elapsed() > self.run_duration {
                self.exit = Some(StatusExit::Timeout);
            }
            terminal.lock().unwrap().draw(|frame| self.draw(frame))?;
        }
        if let Some(StatusExit::Quit) = self.exit {
            shutdown.store(true, Ordering::SeqCst);
        }
        let exit_status = self.exit.unwrap();
        Ok(exit_status)
    }

    pub fn new(
        rx: Receiver<(usize, usize)>,
        run_duration: Duration,
        camp_num: usize,
        run_num: usize,
    ) -> Self {
        Self {
            counter: Counter::default(),
            rx,
            t_begin: Instant::now(),
            run_duration,
            run_num,
            camp_num,
            exit: None,
            buffer_len: 0,
        }
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
    }

    fn handle_events(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(10))? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)
                }
                _ => {}
            };
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = Some(StatusExit::Quit);
    }
}

impl Widget for &Status {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title =
            Line::from(format!(" Campaign {} Run {} Status ", self.camp_num, self.run_num).bold());
        let instructrions = Line::from(vec![" Quit ".into(), "<Q> ".blue().bold()]);
        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructrions.centered())
            .border_set(border::THICK);

        let status_text = Text::from(vec![Line::from(vec![
            "Elapsed time: ".into(),
            self.counter
                .t_begin
                .elapsed()
                .as_secs()
                .to_string()
                .yellow(),
            " s".into(),
            " Events: ".into(),
            self.counter.n_events.to_string().yellow(),
            " Data rate: ".into(),
            format!("{:.2}", self.counter.rate()).yellow(),
            " MB/s ".into(),
            " Buffer length: ".into(),
            self.buffer_len.to_string().yellow(),
        ])]);

        Paragraph::new(status_text)
            .centered()
            .block(block)
            .render(area, buf);
    }
}
