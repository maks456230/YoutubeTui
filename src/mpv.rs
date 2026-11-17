//! Управление mpv через JSON IPC (unix-сокет / named pipe).
//!
//! mpv сам умеет отдавать YouTube-ссылки в yt-dlp (встроенный ytdl_hook),
//! поэтому мы просто говорим ему `loadfile <watch url>`.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum Event {
    TimePos(f64),
    Duration(f64),
    Paused(bool),
    Volume(f64),
    Idle(bool),
    Buffering(bool),
    /// трек доиграл до конца
    Eof,
    /// файл прерван нами (loadfile/stop) — не повод переключать трек
    Stopped,
    /// mpv не смог воспроизвести файл
    Failed(String),
    /// файл загружен и начал играть
    Loaded,
    /// mpv умер
    Dead(String),
    /// строчка из лога mpv (там же ошибки yt-dlp)
    Log(String),
}

#[cfg(unix)]
type Ipc = std::os::unix::net::UnixStream;
#[cfg(windows)]
type Ipc = std::fs::File;

/// Что умеет конкретный запуск mpv.
#[derive(Clone, Copy, Debug)]
pub struct PlayerOptions {
    pub volume: i64,
    /// режим, в котором вообще может быть видео (ytt)
    pub allow_video: bool,
    /// начать сразу с включённым видео
    pub start_video: bool,
}

impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            volume: 80,
            allow_video: false,
            start_video: false,
        }
    }
}

/// Аргументы mpv. Отдельной функцией — чтобы можно было протестировать.
pub fn spawn_args(opts: PlayerOptions, socket: &str) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--idle=yes".into(),
        "--no-terminal".into(),
        "--really-quiet".into(),
        "--no-input-terminal".into(),
        "--no-resume-playback".into(),
        "--save-position-on-quit=no".into(),
        format!("--volume={}", opts.volume.clamp(0, 150)),
        format!("--input-ipc-server={socket}"),
    ];

    if opts.allow_video {
        // видеоразрешение оставляем готовым: окно появится, когда включим дорожку
        if !opts.start_video {
            a.push("--vid=no".into());
        }
    } else {
        // чисто музыкальный режим — видео не нужно вообще
        a.push("--no-video".into());
        a.push("--audio-display=no".into());
    }
    // формат уже выбран нами в yt-dlp, хук mpv не нужен
    a.push("--no-ytdl".into());
    a
}

pub struct Player {
    child: Child,
    sock: PathBuf,
    writer: Arc<Mutex<Ipc>>,
    events: Receiver<Event>,
    volume: i64,
}

impl Player {
    pub fn spawn(opts: PlayerOptions) -> Result<Self, String> {
        let stamp = std::process::id();
        #[cfg(unix)]
        let sock = std::env::temp_dir().join(format!("ytmt-mpv-{stamp}.sock"));
        #[cfg(windows)]
        let sock = PathBuf::from(format!(r"\\.\pipe\ytmt-mpv-{stamp}"));

        let _ = std::fs::remove_file(&sock);

        let mut cmd = Command::new("mpv");
        cmd.args(spawn_args(opts, &sock.display().to_string()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("не удалось запустить mpv: {e}"))?;

        // ждём, пока поднимется IPC
        let mut conn = None;
        for _ in 0..100 {
            if let Ok(s) = connect(&sock) {
                conn = Some(s);
                break;
            }
            if let Ok(Some(status)) = child.try_wait() {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                return Err(format!(
                    "mpv завершился при старте ({status}): {}",
                    err.lines().last().unwrap_or("").trim()
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let conn = conn.ok_or_else(|| "mpv не поднял IPC-сокет (таймаут)".to_string())?;

        let writer = Arc::new(Mutex::new(conn));
        let (tx, events) = channel();

        send_raw(
            &writer,
            json!({"command": ["set_property", "pause", false]}),
        );

        // stderr обязательно вычитываем: иначе mpv упрётся в полный pipe,
        // а заодно показываем пользователю его ошибки
        if let Some(stderr) = child.stderr.take() {
            let tx2 = tx.clone();
            std::thread::spawn(move || {
                let mut buf = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match buf.read_line(&mut line) {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {}
                    }
                    let l = line.trim().to_string();
                    if l.is_empty() {
                        continue;
                    }
                    if tx2.send(Event::Log(l)).is_err() {
                        return;
                    }
                }
            });
        }

        spawn_reader(sock.clone(), tx);

        Ok(Self {
            child,
            sock,
            writer,
            events,
            volume: opts.volume,
        })
    }

    pub fn poll(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            out.push(ev);
        }
        out
    }

    pub fn load(&self, url: &str) {
        send_raw(
            &self.writer,
            json!({"command": ["loadfile", url, "replace"]}),
        );
    }

    pub fn set_pause(&self, paused: bool) {
        send_raw(
            &self.writer,
            json!({"command": ["set_property", "pause", paused]}),
        );
    }

    pub fn seek_rel(&self, secs: f64) {
        send_raw(&self.writer, json!({"command": ["seek", secs, "relative"]}));
    }

    /// Включить/выключить видеодорожку (окно mpv появляется само).
    pub fn set_video(&self, on: bool) {
        send_raw(
            &self.writer,
            json!({"command": ["set_property", "vid", if on { "auto" } else { "no" }]}),
        );
    }

    pub fn seek_abs(&self, secs: f64) {
        send_raw(&self.writer, json!({"command": ["seek", secs, "absolute"]}));
    }

    pub fn stop(&self) {
        send_raw(&self.writer, json!({"command": ["stop"]}));
    }

    pub fn set_volume(&mut self, v: i64) {
        self.volume = v.clamp(0, 150);
        send_raw(
            &self.writer,
            json!({"command": ["set_property", "volume", self.volume]}),
        );
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        send_raw(&self.writer, json!({"command": ["quit"]}));
        std::thread::sleep(Duration::from_millis(60));
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.sock);
    }
}

fn send_raw(writer: &Arc<Mutex<Ipc>>, v: Value) {
    if let Ok(mut w) = writer.lock() {
        let mut line = v.to_string();
        line.push('\n');
        let _ = w.write_all(line.as_bytes());
        let _ = w.flush();
    }
}

#[cfg(unix)]
fn connect(path: &PathBuf) -> std::io::Result<Ipc> {
    std::os::unix::net::UnixStream::connect(path)
}

#[cfg(windows)]
fn connect(path: &PathBuf) -> std::io::Result<Ipc> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
}

fn spawn_reader(sock: PathBuf, tx: Sender<Event>) {
    std::thread::spawn(move || {
        // ВАЖНО: property-change приходят только тому клиенту, который подписался,
        // поэтому observe_property шлём по тому же соединению, из которого читаем.
        let Ok(mut reader) = connect(&sock) else {
            return;
        };
        for (id, prop) in [
            (1, "time-pos"),
            (2, "duration"),
            (3, "pause"),
            (4, "volume"),
            (5, "idle-active"),
            (6, "paused-for-cache"),
        ] {
            let mut line = json!({"command": ["observe_property", id, prop]}).to_string();
            line.push('\n');
            let _ = reader.write_all(line.as_bytes());
        }
        let _ = reader.flush();

        let mut buf = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match buf.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Event::Dead("mpv отключился".into()));
                    return;
                }
                Ok(_) => {}
            }
            let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            let ev = match v.get("event").and_then(|e| e.as_str()) {
                Some("property-change") => {
                    let id = v.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                    let data = v.get("data").cloned().unwrap_or(Value::Null);
                    match id {
                        1 => data.as_f64().map(Event::TimePos),
                        2 => data.as_f64().map(Event::Duration),
                        3 => data.as_bool().map(Event::Paused),
                        4 => data.as_f64().map(Event::Volume),
                        5 => data.as_bool().map(Event::Idle),
                        6 => data.as_bool().map(Event::Buffering),
                        _ => None,
                    }
                }
                Some("file-loaded") => Some(Event::Loaded),
                Some("end-file") => {
                    let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                    match reason {
                        "eof" => Some(Event::Eof),
                        "error" => Some(Event::Failed(
                            v.get("file_error")
                                .and_then(|e| e.as_str())
                                .unwrap_or("mpv не смог воспроизвести поток")
                                .to_string(),
                        )),
                        // stop / quit / redirect — это наши собственные действия
                        _ => Some(Event::Stopped),
                    }
                }
                _ => None,
            };
            if let Some(ev) = ev {
                if tx.send(ev).is_err() {
                    return;
                }
            }
        }
    });
}
