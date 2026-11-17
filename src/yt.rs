//! Всё, что связано с YouTube Music: поиск через `yt-dlp`, обложки через curl/wget.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use image::DynamicImage;

use crate::mode::Mode;
use serde::Deserialize;

pub const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0 Safari/537.36";

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
    pub thumb: Option<String>,
}

impl Track {
    pub fn url(&self) -> String {
        format!("https://music.youtube.com/watch?v={}", self.id)
    }
}

pub fn fmt_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "--:--".to_string();
    }
    let s = secs as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Есть ли бинарь в PATH.
/// Первая заданная переменная окружения из списка (новые имена + старые YTMT_*).
pub fn env_first(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|n| std::env::var(n).ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub fn have(bin: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        if dir.join(bin).is_file() {
            return true;
        }
        cfg!(windows) && dir.join(format!("{bin}.exe")).is_file()
    })
}

// ---------------------------------------------------------------- поиск

#[derive(Deserialize)]
pub struct RawSearch {
    #[serde(default)]
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
pub struct RawEntry {
    id: Option<String>,
    title: Option<String>,
    duration: Option<f64>,
    channel: Option<String>,
    uploader: Option<String>,
    artist: Option<String>,
    ie_key: Option<String>,
    #[serde(default)]
    thumbnails: Vec<RawThumb>,
}

#[derive(Deserialize)]
pub struct RawThumb {
    url: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

/// Команда yt-dlp с учётом YTMT_PROXY / YTMT_COOKIES_FROM_BROWSER.
pub fn ytdl() -> Command {
    let mut c = Command::new("yt-dlp");
    if let Some(browser) = env_first(&["YTM_COOKIES_FROM_BROWSER", "YTMT_COOKIES_FROM_BROWSER"]) {
        c.args(["--cookies-from-browser", &browser]);
    }
    if let Some(proxy) = env_first(&["YTM_PROXY", "YTMT_PROXY"]) {
        c.args(["--proxy", &proxy]);
    }
    c
}

/// Куда идти за результатами: каталог YouTube Music или обычный поиск YouTube.
pub fn search_target(query: &str, mode: Mode) -> String {
    match mode {
        Mode::Music => format!("https://music.youtube.com/search?q={}", urlencode(query)),
        Mode::Youtube => format!("ytsearch25:{query}"),
    }
}

/// Ищет треки/видео. Возвращает ошибку только если всё совсем плохо.
pub fn search(query: &str, mode: Mode) -> Result<Vec<Track>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    if !have("yt-dlp") {
        return Err("yt-dlp не найден в PATH — поставь его (pipx install yt-dlp)".into());
    }

    if let Some(t) = direct_link(q) {
        return Ok(vec![t]);
    }

    match mode {
        Mode::Youtube => search_json(&search_target(q, mode)),
        // 1) каталог YouTube Music, 2) фолбэк — обычный поиск YouTube
        Mode::Music => match search_json(&search_target(q, mode)) {
            Ok(v) if !v.is_empty() => Ok(v),
            _ => search_json(&search_target(q, Mode::Youtube)),
        },
    }
}

fn search_json(target: &str) -> Result<Vec<Track>, String> {
    let out = ytdl()
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--ignore-errors",
            "--no-warnings",
            "--no-progress",
            "--socket-timeout",
            "15",
            target,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("не удалось запустить yt-dlp: {e}"))?;

    if out.stdout.is_empty() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail = err.lines().rev().take(2).collect::<Vec<_>>().join(" / ");
        return Err(if tail.is_empty() {
            "yt-dlp ничего не вернул".to_string()
        } else {
            tail
        });
    }

    let raw: RawSearch = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("не смог разобрать ответ yt-dlp: {e}"))?;

    Ok(tracks_from_search(raw))
}

/// Превращает ответ yt-dlp в список треков: отбрасывает альбомы/плейлисты,
/// выбирает самую крупную обложку, треки с известной длительностью — первыми.
pub fn tracks_from_search(raw: RawSearch) -> Vec<Track> {
    let mut tracks: Vec<Track> = raw
        .entries
        .into_iter()
        .filter_map(|e| {
            let id = e.id?;
            // каналы/альбомы/плейлисты приходят с ie_key YoutubeTab — выкидываем
            if let Some(ie) = e.ie_key.as_deref() {
                if ie != "Youtube" {
                    return None;
                }
            }
            // отсекаем альбомы/плейлисты: у них id не 11 символов
            if id.len() != 11
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return None;
            }
            let title = e.title.unwrap_or_else(|| id.clone());
            let title = title.trim().to_string();
            let artist = e
                .artist
                .or(e.channel)
                .or(e.uploader)
                .unwrap_or_default()
                .trim()
                .to_string();
            // music.youtube.com-поиск не отдаёт превью в flat-режиме, но обложка
            // у любого видео лежит по предсказуемому адресу
            let thumb = e
                .thumbnails
                .into_iter()
                .max_by_key(|t| t.width as u64 * t.height as u64)
                .map(|t| t.url)
                .or_else(|| Some(format!("https://i.ytimg.com/vi/{id}/maxresdefault.jpg")));
            Some(Track {
                id,
                title,
                artist,
                duration: e.duration.unwrap_or(0.0),
                thumb,
            })
        })
        .collect();

    // если длительность известна — это точно трек/видео, такие показываем первыми
    tracks.sort_by_key(|t| t.duration <= 0.0);

    tracks
}

/// Понимает ссылки/голые id: `https://music.youtube.com/watch?v=...`, `youtu.be/...`, `dQw4w9WgXcQ`.
pub fn direct_link(q: &str) -> Option<Track> {
    let q = q.trim();
    let id = if let Some(rest) = q.split("v=").nth(1) {
        rest.split(['&', '#', '?']).next().unwrap_or("")
    } else if q.contains("youtu.be/") {
        q.split("youtu.be/").nth(1)?.split(['&', '#', '?']).next()?
    } else if q.len() == 11
        && q.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        q
    } else {
        return None;
    };
    let id = id.trim();
    if id.len() != 11 {
        return None;
    }
    Some(Track {
        id: id.to_string(),
        title: format!("Прямая ссылка · {q}"),
        artist: "ссылка".to_string(),
        duration: 0.0,
        thumb: Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")),
    })
}

/// Какой формат просить у yt-dlp. Для видео берём прогрессивный (звук+картинка
/// в одном файле) — его можно отдать mpv одной ссылкой.
pub fn format_selector(want_video: bool) -> &'static str {
    if want_video {
        "best[height<=?1080][ext=mp4]/best"
    } else {
        "bestaudio/best"
    }
}

/// Прямая ссылка на поток + длительность (если отдаёт yt-dlp).
pub struct Stream {
    pub url: String,
    pub duration: Option<f64>,
}

/// Резолвим поток сами, а не через ytdl_hook mpv: так мы видим настоящую
/// ошибку yt-dlp (например «Sign in to confirm you're not a bot»).
pub fn resolve(track: &Track, want_video: bool) -> Result<Stream, String> {
    let format = format_selector(want_video);
    if !have("yt-dlp") {
        return Err("yt-dlp не найден в PATH".into());
    }
    let out = ytdl()
        .args([
            "--no-warnings",
            "--no-progress",
            "--socket-timeout",
            "20",
            "-f",
            format,
            "--print",
            "duration",
            "--print",
            "urls",
        ])
        .arg(track.url())
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("не удалось запустить yt-dlp: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let duration = lines.next().and_then(|l| l.parse::<f64>().ok());
    let url = lines.next().map(|s| s.to_string());

    // старые версии yt-dlp не знают "--print urls" — пробуем классический -g
    let url = match url {
        Some(u) if u.starts_with("http") => Some(u),
        _ => {
            let out = ytdl()
                .args(["--no-warnings", "--no-progress", "-f", format, "-g"])
                .arg(track.url())
                .stdin(Stdio::null())
                .output()
                .map_err(|e| format!("не удалось запустить yt-dlp: {e}"))?;
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::trim)
                .find(|l| l.starts_with("http"))
                .map(|s| s.to_string())
        }
    };

    match url {
        Some(url) => Ok(Stream { url, duration }),
        None => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(friendly_error(&err))
        }
    }
}

/// Укорачиваем типичные простыни yt-dlp до одной понятной строки.
fn friendly_error(stderr: &str) -> String {
    let first = stderr
        .lines()
        .find(|l| l.contains("ERROR") || l.contains("error"))
        .or_else(|| stderr.lines().find(|l| !l.trim().is_empty()))
        .unwrap_or("yt-dlp не смог получить поток")
        .trim()
        .to_string();
    let low = first.to_lowercase();
    if low.contains("sign in to confirm") || low.contains("bot") {
        return "YouTube требует вход: запусти с YTMT_COOKIES_FROM_BROWSER=firefox (или chrome)"
            .into();
    }
    if low.contains("video unavailable") {
        return "видео недоступно (удалено или заблокировано в регионе)".into();
    }
    if low.contains("unable to download") || low.contains("timed out") || low.contains("connection")
    {
        return format!("сеть/блокировка: {first}");
    }
    first.chars().take(200).collect()
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// -------------------------------------------------------------- обложки

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ytm")
        .join("covers")
}

/// Достаёт обложку: сначала из кэша, иначе качает.
pub fn load_cover(track: &Track) -> Option<DynamicImage> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{}.img", track.id));

    if let Ok(bytes) = std::fs::read(&path) {
        if let Some(img) = decode(&bytes) {
            return Some(downscale(img, 1024));
        }
        let _ = std::fs::remove_file(&path); // битый кэш
    }

    for url in cover_urls(track) {
        if let Some(bytes) = fetch(&url) {
            if let Some(img) = decode(&bytes) {
                let _ = std::fs::write(&path, &bytes);
                return Some(downscale(img, 1024));
            }
        }
    }

    // последний шанс: пусть yt-dlp сам вытащит превью
    let tmp = std::env::temp_dir().join(format!("ytmt-{}.img", std::process::id()));
    if ytdlp_thumb(track, &tmp) {
        if let Ok(bytes) = std::fs::read(&tmp) {
            let img = decode(&bytes);
            let _ = std::fs::remove_file(&tmp);
            if let Some(img) = img {
                let _ = std::fs::write(&path, &bytes);
                return Some(downscale(img, 1024));
            }
        }
    }
    None
}

/// Адреса обложек: сначала тот, что дал yt-dlp, потом предсказуемые по id.
fn cover_urls(track: &Track) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(u) = &track.thumb {
        urls.push(u.clone());
    }
    for name in ["maxresdefault", "sddefault", "hqdefault", "mqdefault"] {
        let u = format!("https://i.ytimg.com/vi/{}/{name}.jpg", track.id);
        if !urls.contains(&u) {
            urls.push(u);
        }
    }
    urls
}

fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    image::load_from_memory(bytes).ok()
}

fn downscale(img: DynamicImage, max: u32) -> DynamicImage {
    if img.width() <= max && img.height() <= max {
        return img;
    }
    // резко уменьшаем до создания протокола — иначе кодирование в терминал тормозит
    let (w, h) = (img.width() as f64, img.height() as f64);
    let k = (max as f64) / w.max(h);
    img.resize(
        (w * k) as u32,
        (h * k) as u32,
        image::imageops::FilterType::Triangle,
    )
}

fn fetch(url: &str) -> Option<Vec<u8>> {
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let tmp = std::env::temp_dir().join(format!("ytmt-{stamp}.thumb"));
    let proxy = env_first(&["YTM_PROXY", "YTMT_PROXY"]);

    let ok = if have("curl") {
        let mut c = Command::new("curl");
        c.args(["-fsSL", "--max-time", "25", "-A", UA, "-o"])
            .arg(&tmp)
            .arg(url);
        if let Some(p) = &proxy {
            c.args(["-x", p]);
        }
        c.status().map(|s| s.success()).unwrap_or(false)
    } else if have("wget") {
        let mut c = Command::new("wget");
        c.args(["-q", "-T", "25", "-U", UA, "-O"])
            .arg(&tmp)
            .arg(url);
        if let Some(p) = &proxy {
            c.arg("-e")
                .arg("use_proxy=yes")
                .arg("-e")
                .arg(format!("https_proxy={p}"));
        }
        c.status().map(|s| s.success()).unwrap_or(false)
    } else {
        false
    };

    let bytes = if ok {
        std::fs::read(&tmp).ok().filter(|b| !b.is_empty())
    } else {
        None
    };
    let _ = std::fs::remove_file(&tmp);
    bytes
}

fn ytdlp_thumb(track: &Track, out: &std::path::Path) -> bool {
    if !have("yt-dlp") {
        return false;
    }
    let base = out.with_extension("thumb");
    let status = ytdl()
        .args([
            "--skip-download",
            "--no-warnings",
            "--no-progress",
            "--write-thumbnail",
            "--convert-thumbnails",
            "jpg",
            "-o",
        ])
        .arg(&base)
        .arg(track.url())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !status {
        return false;
    }
    let jpg = base.with_extension("thumb.jpg");
    match std::fs::read(&jpg) {
        Ok(bytes) => {
            let _ = std::fs::write(out, bytes);
            let _ = std::fs::remove_file(&jpg);
            true
        }
        Err(_) => false,
    }
}
