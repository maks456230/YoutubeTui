//! Состояние приложения и вся логика клавиш.

use std::sync::mpsc::{channel, Receiver, Sender};

use image::DynamicImage;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

use crate::mode::Mode;
use crate::mpv::{self, Player, PlayerOptions};
use crate::yt::{self, Track};

pub enum Msg {
    SearchDone(Vec<Track>),
    SearchErr(String),
    Cover(String, DynamicImage),
    CoverErr(String),
    Stream {
        id: String,
        url: String,
        duration: Option<f64>,
        /// с какой секунды начинать (нужно при переключении видео на ходу)
        start_at: f64,
    },
    StreamErr {
        id: String,
        err: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Results,
    Queue,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

impl Repeat {
    pub fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Repeat::Off => "выкл",
            Repeat::All => "всё",
            Repeat::One => "трек",
        }
    }
}

pub struct App {
    pub mode: Mode,
    pub picker: Picker,
    pub proto: String,

    pub query: String,
    pub editing: bool,
    pub searching: bool,

    pub results: Vec<Track>,
    pub results_state: ListState,
    pub queue: Vec<Track>,
    pub queue_state: ListState,
    pub qpos: Option<usize>,
    pub focus: Focus,

    pub now: Option<Track>,
    pub playing: bool,
    pub paused: bool,
    pub buffering: bool,
    pub pos: f64,
    pub dur: f64,
    pub volume: i64,
    pub repeat: Repeat,
    /// окно видео включено (только в режиме YouTube)
    pub video: bool,

    pub cover: Option<StatefulProtocol>,
    pub cover_id: Option<String>,
    cover_want: Option<String>,

    pub status: String,
    pub error: Option<String>,
    pub should_quit: bool,
    pub tick: u64,

    player: Option<Player>,
    pub player_error: Option<String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl App {
    pub fn new(picker: Picker, proto: String, mode: Mode) -> Self {
        let (tx, rx) = channel();
        let mut results_state = ListState::default();
        results_state.select(Some(0));
        let mut queue_state = ListState::default();
        queue_state.select(Some(0));

        Self {
            mode,
            picker,
            proto,
            query: String::new(),
            editing: false,
            searching: false,
            results: Vec::new(),
            results_state,
            queue: Vec::new(),
            queue_state,
            qpos: None,
            focus: Focus::Results,
            now: None,
            playing: false,
            paused: false,
            buffering: false,
            pos: 0.0,
            dur: 0.0,
            volume: 80,
            repeat: Repeat::Off,
            video: false,
            cover: None,
            cover_id: None,
            cover_want: None,
            status: "нажми / и введи запрос · Enter — играть".to_string(),
            error: None,
            should_quit: false,
            tick: 0,
            player: None,
            player_error: None,
            tx,
            rx,
        }
    }

    pub fn start_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.search();
    }

    fn search(&mut self) {
        let q = self.query.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.searching = true;
        self.error = None;
        self.status = format!("ищу «{q}»…");
        let mode = self.mode;
        let tx = self.tx.clone();
        std::thread::spawn(move || match yt::search(&q, mode) {
            Ok(tracks) => {
                let _ = tx.send(Msg::SearchDone(tracks));
            }
            Err(e) => {
                let _ = tx.send(Msg::SearchErr(e));
            }
        });
    }

    // ------------------------------------------------------------- ввод

    pub fn on_key(&mut self, key: KeyEvent) {
        self.error = None;

        // Ctrl+C в raw-режиме не превращается в SIGINT — обрабатываем сами
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return;
        }

        // ---- режим набора запроса
        if self.editing {
            match key.code {
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.clear()
                }
                KeyCode::Char(c) => self.query.push(c),
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Enter => {
                    self.editing = false;
                    self.search();
                }
                KeyCode::Esc => self.editing = false,
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('/') => {
                self.editing = true;
            }
            KeyCode::Esc => {
                self.query.clear();
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Results => Focus::Queue,
                    Focus::Queue => Focus::Results,
                };
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::Char('g') | KeyCode::Home => self.set_sel(0),
            KeyCode::Char('G') | KeyCode::End => self.set_sel(usize::MAX),
            KeyCode::Enter => self.play_selected(),
            KeyCode::Char('a') => self.add_selected_to_queue(),
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Right | KeyCode::Char('l') => self.seek(5.0),
            KeyCode::Left | KeyCode::Char('h') => self.seek(-5.0),
            KeyCode::Char('+') | KeyCode::Char('=') => self.change_volume(5),
            KeyCode::Char('-') | KeyCode::Char('_') => self.change_volume(-5),
            KeyCode::Char('n') => self.next_track(false),
            KeyCode::Char('p') => self.prev_track(),
            KeyCode::Char('r') => {
                self.repeat = self.repeat.next();
                self.status = format!("повтор: {}", self.repeat.label());
            }
            KeyCode::Char('v') if self.mode.allows_video() => self.toggle_video(),
            KeyCode::Char('d') => self.remove_from_queue(),
            KeyCode::Char('c') => {
                self.queue
                    .retain(|t| Some(&t.id) != self.now.as_ref().map(|n| &n.id));
                self.queue_state.select(Some(0));
                self.status = "очередь очищена".to_string();
            }
            _ => {}
        }
    }

    fn move_sel(&mut self, delta: isize) {
        match self.focus {
            Focus::Results => step(&mut self.results_state, self.results.len(), delta),
            Focus::Queue => step(&mut self.queue_state, self.queue.len(), delta),
        }
        self.ensure_cover();
    }

    fn set_sel(&mut self, idx: usize) {
        match self.focus {
            Focus::Results => {
                if !self.results.is_empty() {
                    let i = idx.min(self.results.len() - 1);
                    self.results_state.select(Some(i));
                }
            }
            Focus::Queue => {
                if !self.queue.is_empty() {
                    let i = idx.min(self.queue.len() - 1);
                    self.queue_state.select(Some(i));
                }
            }
        }
        self.ensure_cover();
    }

    pub fn selected(&self) -> Option<&Track> {
        match self.focus {
            Focus::Results => self
                .results_state
                .selected()
                .and_then(|i| self.results.get(i)),
            Focus::Queue => self.queue_state.selected().and_then(|i| self.queue.get(i)),
        }
    }

    // --------------------------------------------------------- плеер

    pub fn want_video(&self) -> bool {
        self.mode.allows_video() && self.video
    }

    fn toggle_video(&mut self) {
        self.video = !self.video;
        self.status = if self.video {
            "видео: включено (окно mpv)".to_string()
        } else {
            "видео: выключено".to_string()
        };
        // поток уже скачан в выбранном формате — перезапрашиваем и продолжаем с того же места
        if let Some(track) = self.now.clone() {
            let at = self.pos;
            self.resolve_and_play(track, at);
        } else if let Some(p) = self.player.as_mut() {
            p.set_video(self.video);
        }
    }

    fn player(&mut self) -> Option<&mut Player> {
        if self.player.is_none() {
            let opts = PlayerOptions {
                volume: self.volume,
                allow_video: self.mode.allows_video(),
                start_video: self.want_video(),
            };
            match Player::spawn(opts) {
                Ok(p) => {
                    self.player = Some(p);
                    self.player_error = None;
                }
                Err(e) => {
                    self.player_error = Some(e.clone());
                    self.status = format!("mpv: {e}");
                    return None;
                }
            }
        }
        self.player.as_mut()
    }

    fn play_selected(&mut self) {
        match self.focus {
            Focus::Results => {
                let Some(i) = self.results_state.selected() else {
                    return;
                };
                let Some(track) = self.results.get(i).cloned() else {
                    return;
                };
                // играем «весь список результатов» как очередь, начиная с выбранного
                self.queue = self.results.clone();
                self.queue_state.select(Some(i));
                self.qpos = Some(i);
                self.play(track);
            }
            Focus::Queue => {
                let Some(i) = self.queue_state.selected() else {
                    return;
                };
                let Some(track) = self.queue.get(i).cloned() else {
                    return;
                };
                self.qpos = Some(i);
                self.play(track);
            }
        }
    }

    fn play(&mut self, track: Track) {
        let at = 0.0;
        self.resolve_and_play(track, at);
    }

    fn resolve_and_play(&mut self, track: Track, start_at: f64) {
        self.dur = track.duration;
        self.pos = 0.0;
        self.paused = false;
        self.buffering = true;
        self.now = Some(track.clone());
        self.cover = None;
        self.cover_id = None;
        self.ensure_cover();
        self.status = format!("получаю поток: {}", track.title);
        self.error = None;

        if self.player().is_none() {
            self.playing = false;
            self.buffering = false;
            return;
        }
        self.playing = true;

        // поток резолвим в отдельном потоке, чтобы не морозить интерфейс
        let want_video = self.want_video();
        let tx = self.tx.clone();
        std::thread::spawn(move || match yt::resolve(&track, want_video) {
            Ok(s) => {
                let _ = tx.send(Msg::Stream {
                    id: track.id.clone(),
                    url: s.url,
                    duration: s.duration,
                    start_at,
                });
            }
            Err(err) => {
                let _ = tx.send(Msg::StreamErr {
                    id: track.id.clone(),
                    err,
                });
            }
        });
    }

    fn toggle_pause(&mut self) {
        if self.now.is_none() {
            return;
        }
        self.paused = !self.paused;
        let paused = self.paused;
        if let Some(p) = self.player() {
            p.set_pause(paused);
        }
    }

    fn seek(&mut self, delta: f64) {
        if let Some(p) = self.player() {
            p.seek_rel(delta);
            self.pos = (self.pos + delta).max(0.0);
        }
    }

    fn change_volume(&mut self, delta: i64) {
        self.volume = (self.volume + delta).clamp(0, 150);
        let v = self.volume;
        if let Some(p) = self.player() {
            p.set_volume(v);
        }
        self.status = format!("громкость {v}%");
    }

    /// Куда переключаться дальше — чистая логика, без побочных эффектов.
    pub fn next_target(&self, from_eof: bool) -> Option<(Option<usize>, Track)> {
        if from_eof && self.repeat == Repeat::One {
            return self.now.clone().map(|t| (self.qpos, t));
        }
        if let Some(i) = self.qpos {
            if let Some(t) = self.queue.get(i + 1) {
                return Some((Some(i + 1), t.clone()));
            }
        }
        if self.repeat == Repeat::All {
            if let Some(t) = self.queue.first() {
                return Some((Some(0), t.clone()));
            }
        }
        None
    }

    fn next_track(&mut self, from_eof: bool) {
        match self.next_target(from_eof) {
            Some((pos, t)) => {
                self.qpos = pos;
                if let Some(p) = pos {
                    self.queue_state.select(Some(p));
                }
                self.play(t);
            }
            None => {
                // очередь закончилась
                self.playing = false;
                self.paused = false;
                self.buffering = false;
                if let Some(p) = self.player.as_mut() {
                    p.stop();
                }
                self.status = "очередь закончилась".to_string();
            }
        }
    }

    fn prev_track(&mut self) {
        if self.pos > 3.0 {
            self.seek(-self.pos);
            return;
        }
        if let Some(i) = self.qpos {
            if i > 0 {
                let t = self.queue[i - 1].clone();
                self.qpos = Some(i - 1);
                self.queue_state.select(Some(i - 1));
                self.play(t);
                return;
            }
        }
        self.seek(-self.pos);
    }

    fn add_selected_to_queue(&mut self) {
        if self.focus != Focus::Results {
            return;
        }
        let Some(t) = self.selected().cloned() else {
            return;
        };
        if self.queue.iter().any(|q| q.id == t.id) {
            self.status = "уже в очереди".to_string();
            return;
        }
        self.queue.push(t.clone());
        self.queue_state.select(Some(self.queue.len() - 1));
        self.status = format!("+ в очередь: {}", t.title);
    }

    fn remove_from_queue(&mut self) {
        if self.focus != Focus::Queue {
            return;
        }
        let Some(i) = self.queue_state.selected() else {
            return;
        };
        if self.qpos == Some(i) {
            self.status = "этот трек играет — сначала переключи".to_string();
            return;
        }
        let t = self.queue.remove(i);
        if let Some(q) = self.qpos {
            if i < q {
                self.qpos = Some(q - 1);
            }
        }
        let sel = i.min(self.queue.len().saturating_sub(1));
        self.queue_state.select(Some(sel));
        self.status = format!("− из очереди: {}", t.title);
    }

    // ------------------------------------------------------ обложки

    pub fn ensure_cover(&mut self) {
        let target = self.now.clone().or_else(|| self.selected().cloned());
        let Some(t) = target else { return };
        if self.cover_id.as_deref() == Some(t.id.as_str())
            || self.cover_want.as_deref() == Some(t.id.as_str())
        {
            return;
        }
        self.cover_want = Some(t.id.clone());
        let tx = self.tx.clone();
        std::thread::spawn(move || match yt::load_cover(&t) {
            Some(img) => {
                let _ = tx.send(Msg::Cover(t.id.clone(), img));
            }
            None => {
                let _ = tx.send(Msg::CoverErr(t.id.clone()));
            }
        });
    }

    // ------------------------------------------------- цикл событий

    /// Забирает сообщения воркеров и события mpv. Вызывать каждый кадр.
    pub fn pump(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::SearchDone(tracks) => {
                    self.searching = false;
                    if tracks.is_empty() {
                        self.status = "ничего не найдено".to_string();
                        self.error = Some("ничего не найдено".into());
                    } else {
                        self.status = format!("найдено: {}", tracks.len());
                        self.results = tracks;
                        self.results_state.select(Some(0));
                        self.focus = Focus::Results;
                        self.cover = None;
                        self.cover_id = None;
                        self.ensure_cover();
                    }
                }
                Msg::SearchErr(e) => {
                    self.searching = false;
                    self.status = "ошибка поиска".to_string();
                    self.error = Some(e);
                }
                Msg::Cover(id, img) => {
                    self.cover_want = None;
                    let relevant = self.now.as_ref().map(|t| t.id == id).unwrap_or(false)
                        || self.selected().map(|t| t.id == id).unwrap_or(false);
                    if relevant {
                        self.cover = Some(self.picker.new_resize_protocol(img));
                        self.cover_id = Some(id);
                    }
                }
                Msg::CoverErr(id) => {
                    self.cover_want = None;
                    self.cover_id = Some(id);
                }
                Msg::Stream {
                    id,
                    url,
                    duration,
                    start_at,
                } => {
                    let current = self.now.as_ref().map(|t| t.id == id).unwrap_or(false);
                    if !current {
                        continue; // пользователь уже переключил трек
                    }
                    if let Some(d) = duration {
                        if d > 0.0 {
                            self.dur = d;
                        }
                    }
                    let want_video = self.want_video();
                    if let Some(p) = self.player.as_mut() {
                        p.set_video(want_video);
                        p.set_pause(false);
                        p.load(&url);
                        if start_at > 0.5 {
                            p.seek_abs(start_at);
                        }
                        self.pos = start_at;
                        self.status = self
                            .now
                            .as_ref()
                            .map(|t| {
                                if want_video {
                                    format!("играю (видео): {}", t.title)
                                } else {
                                    format!("играю: {}", t.title)
                                }
                            })
                            .unwrap_or_default();
                    }
                }
                Msg::StreamErr { id, err } => {
                    let current = self.now.as_ref().map(|t| t.id == id).unwrap_or(false);
                    if current {
                        self.playing = false;
                        self.buffering = false;
                        self.status = "не удалось получить поток".to_string();
                        self.error = Some(err);
                    }
                }
            }
        }

        if let Some(player) = self.player.as_ref() {
            for ev in player.poll() {
                match ev {
                    mpv::Event::TimePos(t) => {
                        self.pos = t;
                        self.buffering = false;
                    }
                    mpv::Event::Duration(d) => {
                        if d > 0.0 {
                            self.dur = d;
                        }
                    }
                    mpv::Event::Paused(p) => self.paused = p,
                    mpv::Event::Volume(v) => self.volume = v.round() as i64,
                    mpv::Event::Idle(idle) => {
                        if idle {
                            self.buffering = false;
                        }
                    }
                    mpv::Event::Buffering(b) => self.buffering = b,
                    mpv::Event::Loaded => {
                        self.buffering = false;
                        self.playing = true;
                    }
                    mpv::Event::Eof => {
                        self.next_track(true);
                    }
                    mpv::Event::Stopped => {}
                    mpv::Event::Failed(msg) => {
                        self.buffering = false;
                        self.playing = false;
                        self.status = "ошибка воспроизведения".to_string();
                        self.error = Some(msg);
                    }
                    mpv::Event::Log(line) => {
                        let low = line.to_lowercase();
                        let interesting = ["error", "failed", "cannot", "unable", "sign in"]
                            .iter()
                            .any(|k| low.contains(k));
                        if interesting {
                            let short: String = line.chars().take(160).collect();
                            self.error = Some(short);
                        }
                    }
                    mpv::Event::Dead(e) => {
                        self.player_error = Some(e.clone());
                        self.playing = false;
                        self.player = None;
                        break;
                    }
                }
            }
        }
    }

    pub fn spinner(&self) -> &'static str {
        const F: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        F[(self.tick as usize / 2) % F.len()]
    }
}

fn step(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        state.select(None);
        return;
    }
    let cur = state.selected().unwrap_or(0) as isize;
    let next = (cur + delta).clamp(0, len as isize - 1);
    state.select(Some(next as usize));
}
