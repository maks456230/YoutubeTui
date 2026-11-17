//! Тесты. `cargo test` — без сети; `cargo test -- --ignored` — живой поиск через yt-dlp.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use ratatui_image::picker::Picker;

use crate::app::App;
use crate::ui;
use crate::yt::{self, Track};

fn sample() -> Vec<Track> {
    let names = [
        ("Daft Punk", "Get Lucky (feat. Pharrell Williams)", 248.0),
        (
            "Daft Punk",
            "Instant Crush (feat. Julian Casablancas)",
            337.0,
        ),
        ("Daft Punk", "One More Time", 320.0),
        ("Daft Punk", "Harder, Better, Faster, Stronger", 224.0),
        ("Daft Punk", "Digital Love", 301.0),
        ("Daft Punk", "Aerodynamic", 212.0),
        ("Daft Punk", "Something About Us", 232.0),
    ];
    names
        .iter()
        .enumerate()
        .map(|(i, (a, t, d))| Track {
            id: format!("id{i:09}"),
            title: t.to_string(),
            artist: a.to_string(),
            duration: *d,
            thumb: None,
        })
        .collect()
}

fn dump(app: &mut App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[test]
fn renders_dashboard() {
    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    app.query = "daft punk".into();
    app.results = sample();
    app.results_state.select(Some(1));
    app.now = Some(app.results[0].clone());
    app.playing = true;
    app.pos = 71.0;
    app.dur = 248.0;
    app.volume = 80;
    app.queue = app.results.clone();
    app.qpos = Some(0);
    app.queue_state.select(Some(2));
    app.status = "найдено: 7".into();
    app.cover_id = Some("id000000000".into());

    let out = dump(&mut app, 108, 30);
    println!("\n{out}");
    assert!(out.contains("сейчас играет"));
    assert!(out.contains("результаты"));
    assert!(out.contains("очередь"));
    assert!(out.contains("Get Lucky"));
    assert!(
        out.contains("1:11 / 4:08"),
        "прогресс/время не отрисовались"
    );
}

#[test]
fn renders_empty_state() {
    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    let out = dump(&mut app, 90, 24);
    println!("\n{out}");
    assert!(out.contains("ничего не играет"));
    assert!(out.contains("нажми /"));
}

#[test]
fn renders_tiny_terminal_without_panic() {
    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    let _ = dump(&mut app, 20, 6);
}

#[test]
fn encodes_query_and_parses_links() {
    assert_eq!(yt::urlencode("daft punk"), "daft+punk");
    assert_eq!(yt::urlencode("a&b"), "a%26b");

    let t = yt::direct_link("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD")
        .expect("watch-ссылка");
    assert_eq!(t.id, "dQw4w9WgXcQ");

    let t = yt::direct_link("https://youtu.be/dQw4w9WgXcQ").expect("youtu.be");
    assert_eq!(t.id, "dQw4w9WgXcQ");

    let t = yt::direct_link("dQw4w9WgXcQ").expect("голый id");
    assert_eq!(t.id, "dQw4w9WgXcQ");

    assert!(yt::direct_link("daft punk").is_none());
}

#[test]
fn parses_ytdlp_search_json() {
    // реальный ответ yt-dlp --flat-playlist по music.youtube.com/search
    let json = r#"{"id":"search","title":"daft punk","entries":[
        {"_type":"url","ie_key":"Youtube","id":"5NV6Rdv1a3I","title":"Get Lucky","duration":248,
         "channel":"Daft Punk","thumbnails":[{"url":"https://i.ytimg.com/vi/5NV6Rdv1a3I/hq720.jpg","width":720,"height":404},
                                             {"url":"https://i.ytimg.com/vi/5NV6Rdv1a3I/maxresdefault.jpg","width":1280,"height":720}]},
        {"_type":"url","ie_key":"Youtube","id":"RDCLAK5uy_abc","title":"Daft Punk Radio","duration":null},
        {"_type":"url","ie_key":"Youtube","id":"m4hWHF5r6jU","title":"One More Time","duration":320,"channel":"Daft Punk"}
    ]}"#;

    let raw: yt::RawSearch = serde_json::from_str(json).unwrap();
    let tracks = yt::tracks_from_search(raw);
    // плейлист-радио отфильтрован, обложка выбрана максимальная
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].id, "5NV6Rdv1a3I");
    assert!(tracks[0]
        .thumb
        .as_deref()
        .unwrap()
        .contains("maxresdefault"));
}

#[test]
#[ignore = "нужен yt-dlp и сеть"]
fn live_search_works() {
    let tracks = yt::search("daft punk get lucky", crate::mode::Mode::Music)
        .expect("поиск не должен падать");
    assert!(!tracks.is_empty(), "пустой результат");
    println!("первый результат: {:#?}", tracks.first());
    assert_eq!(tracks[0].id.len(), 11);
}

#[test]
#[ignore = "нужен yt-dlp, сеть и mpv"]
fn live_cover_downloads() {
    let tracks = yt::search("daft punk get lucky", crate::mode::Mode::Music).unwrap();
    let img = yt::load_cover(&tracks[0]).expect("обложка должна скачаться");
    println!("обложка {}x{}", img.width(), img.height());
    assert!(img.width() > 100);
}

// ------------------------------------------------------------ живой mpv IPC

fn write_tone_wav(path: &std::path::Path, secs: u32) {
    // 8 кГц, моно, 16 бит — минимальный валидный wav
    let rate = 8000u32;
    let n = rate * secs;
    let mut data = Vec::with_capacity(n as usize * 2);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.05 * i16::MAX as f32;
        data.extend_from_slice(&(v as i16).to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out).unwrap();
}

#[test]
#[ignore = "нужен mpv"]
fn live_mpv_ipc() {
    use crate::mpv::{Event, Player};
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join("ytmt-mpv-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mpv.conf"), "ao=null\n").unwrap();
    std::env::set_var("MPV_HOME", &dir);

    let wav = dir.join("tone.wav");
    write_tone_wav(&wav, 2);

    let player = Player::spawn(crate::mpv::PlayerOptions {
        volume: 50,
        ..Default::default()
    })
    .expect("mpv должен подняться и отдать IPC");
    player.load(wav.to_str().unwrap());

    let deadline = Instant::now() + Duration::from_secs(25);
    let (mut pos, mut eof) = (false, false);
    while Instant::now() < deadline && !eof {
        for ev in player.poll() {
            println!("event: {ev:?}");
            match ev {
                Event::TimePos(_) => pos = true,
                Event::Eof => eof = true,
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(pos, "mpv не прислал time-pos");
    assert!(eof, "mpv не прислал end-file");
}

#[test]
#[ignore = "нужны mpv, yt-dlp и доступ к YouTube"]
fn live_youtube_playback() {
    use crate::mode::Mode;
    use crate::mpv::{Event, Player, PlayerOptions};
    use std::time::{Duration, Instant};

    // как в приложении: сначала получаем ссылку на поток через yt-dlp,
    // только потом отдаём её mpv
    let track = yt::search("daft punk get lucky", Mode::Music)
        .expect("поиск")
        .remove(0);
    let stream = yt::resolve(&track, false).expect("ссылку на поток не отдали");

    let dir = std::env::temp_dir().join("ytmt-mpv-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mpv.conf"), "ao=null\n").unwrap();
    std::env::set_var("MPV_HOME", &dir);

    let player = Player::spawn(PlayerOptions {
        volume: 70,
        ..Default::default()
    })
    .unwrap();
    player.load(&stream.url);

    let deadline = Instant::now() + Duration::from_secs(60);
    let mut best = 0.0f64;
    while Instant::now() < deadline && best < 3.0 {
        for ev in player.poll() {
            match ev {
                Event::TimePos(t) => best = best.max(t),
                Event::Log(l) => println!("log: {l}"),
                other => println!("event: {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(best >= 3.0, "поток не заиграл (best={best})");
}

// ------------------------------------------------------ логика очереди/повтора

fn app_with_queue(n: usize, pos: Option<usize>) -> App {
    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    app.queue = sample().into_iter().take(n).collect();
    app.qpos = pos;
    app.now = pos.and_then(|i| app.queue.get(i).cloned());
    app
}

#[test]
fn next_target_follows_queue() {
    let mut app = app_with_queue(3, Some(0));
    let (pos, t) = app.next_target(false).expect("есть следующий");
    assert_eq!(pos, Some(1));
    assert_eq!(t.title, app.queue[1].title);

    // конец очереди без повтора — дальше некуда
    app.qpos = Some(2);
    assert!(app.next_target(false).is_none());

    // repeat=all заворачивает на начало
    app.repeat = crate::app::Repeat::All;
    let (pos, _) = app.next_target(false).expect("заворот на начало");
    assert_eq!(pos, Some(0));

    // repeat=one на конце трека играет тот же
    app.repeat = crate::app::Repeat::One;
    app.qpos = Some(1);
    app.now = Some(app.queue[1].clone());
    let (pos, t) = app.next_target(true).expect("повтор трека");
    assert_eq!(pos, Some(1));
    assert_eq!(t.id, app.queue[1].id);
}

#[test]
fn add_and_remove_from_queue() {
    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    app.results = sample();
    app.results_state.select(Some(2));
    app.focus = crate::app::Focus::Results;
    app.on_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('a'),
        ratatui::crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.queue.len(), 1);

    // повторное добавление не дублирует
    app.on_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('a'),
        ratatui::crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.queue.len(), 1);

    app.focus = crate::app::Focus::Queue;
    app.queue_state.select(Some(0));
    app.on_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char('d'),
        ratatui::crossterm::event::KeyModifiers::NONE,
    ));
    assert!(app.queue.is_empty());
}

#[test]
fn typing_a_query() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert!(app.editing);
    for c in "daft".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.query, "daf");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.editing);
    // в режиме набора 'q' не выходит из приложения
    app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.should_quit);
}

// --------------------------------------------- рендер с настоящей картинкой

#[test]
fn renders_real_cover_with_halfblocks() {
    use image::{Rgb, RgbImage};

    // синтетическая «обложка»: градиент + круг
    let (w, h) = (640u32, 360u32);
    let mut img = RgbImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let cx = x as f32 - w as f32 / 2.0;
        let cy = y as f32 - h as f32 / 2.0;
        let inside = (cx * cx + cy * cy).sqrt() < 120.0;
        *px = if inside {
            Rgb([255, 60, 82])
        } else {
            Rgb([
                (x * 255 / w) as u8,
                (y * 255 / h) as u8,
                ((x + y) * 255 / (w + h)) as u8,
            ])
        };
    }

    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    app.results = sample();
    app.now = Some(app.results[0].clone());
    app.playing = true;
    app.pos = 71.0;
    app.dur = 248.0;
    app.queue = app.results.clone();
    app.qpos = Some(0);
    app.results_state.select(Some(1));
    app.cover = Some(
        app.picker
            .new_resize_protocol(image::DynamicImage::ImageRgb8(img)),
    );
    app.cover_id = Some("id000000000".into());

    let out = dump(&mut app, 100, 28);
    println!("\n{out}");
    // halfblock-рендер обязан оставить в буфере полублочные символы
    assert!(
        out.contains('▀') || out.contains('▄'),
        "обложка не нарисовалась (нет полублоков)"
    );
}

// ------------------------------------- выгрузка кадра для рендера «скриншота»

#[test]
#[ignore = "служебный: генерирует JSON кадра для docs/screenshot.png"]
fn dump_frame_json() {
    use ratatui::style::Color;
    use serde_json::json;

    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    app.query = "daft punk".into();
    app.results = sample();
    app.results_state.select(Some(1));
    app.now = Some(app.results[0].clone());
    app.playing = true;
    app.buffering = false;
    app.pos = 71.0;
    app.dur = 248.0;
    app.queue = app.results.clone();
    app.qpos = Some(0);
    app.queue_state.select(Some(2));
    app.status = "найдено: 7".into();

    // настоящая обложка из кэша, если хоть одна уже скачана
    let cached_img = std::fs::read_dir(yt::cache_dir())
        .ok()
        .and_then(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| p.extension().map(|x| x == "img").unwrap_or(false))
        })
        .and_then(|p| std::fs::read(p).ok());
    let img = cached_img
        .and_then(|b| image::load_from_memory(&b).ok())
        .unwrap_or_else(|| {
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                640,
                360,
                image::Rgb([255, 60, 82]),
            ))
        });
    app.cover = Some(app.picker.new_resize_protocol(img));
    app.cover_id = Some("4D7u5KF7SP8".into());

    let (w, h) = (112u16, 32u16);
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, &mut app)).unwrap();
    let buf = term.backend().buffer().clone();

    let color = |c: Color| match c {
        Color::Rgb(r, g, b) => json!([r, g, b]),
        Color::Reset => serde_json::Value::Null,
        Color::Black => json!([0, 0, 0]),
        Color::White => json!([255, 255, 255]),
        other => json!(format!("{other:?}")),
    };

    let mut cells = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let cell = &buf[(x, y)];
            cells.push(json!({
                "s": cell.symbol(),
                "f": color(cell.fg),
                "b": color(cell.bg),
            }));
        }
    }
    let frame = json!({"w": w, "h": h, "cells": cells});
    let path = std::env::temp_dir().join("ytmt_frame.json");
    std::fs::write(&path, serde_json::to_vec(&frame).unwrap()).unwrap();
    println!("frame: {}", path.display());
}

// ------------------------------------- kitty-протокол: что реально уходит в TTY

#[test]
fn kitty_protocol_emits_graphics_sequences() {
    use image::{Rgb, RgbImage};
    use ratatui_image::picker::ProtocolType;

    let mut img = RgbImage::new(96, 54);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = Rgb([(x * 2) as u8, (y * 4) as u8, 200]);
    }

    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);

    let mut app = App::new(
        Picker::halfblocks(),
        "Halfblocks".into(),
        crate::mode::Mode::Music,
    );
    app.picker = picker;
    app.now = Some(sample()[0].clone());
    app.playing = true;
    app.cover = Some(
        app.picker
            .new_resize_protocol(image::DynamicImage::ImageRgb8(img)),
    );

    let mut term = Terminal::new(TestBackend::new(60, 24)).unwrap();
    term.draw(|f| ui::draw(f, &mut app)).unwrap();
    let buf = term.backend().buffer().clone();

    let mut all = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            all.push_str(buf[(x, y)].symbol());
        }
    }
    let kitty = all.matches("\u{1b}_G").count();
    println!("kitty APC-последовательностей в буфере: {kitty}");
    println!(
        "есть base64-payload: {}",
        all.contains("a=T") || all.contains("f=32")
    );
    assert!(
        kitty > 0,
        "kitty-протокол не выдал графических последовательностей"
    );
}

#[test]
fn elides_by_display_width() {
    use crate::ui::elide;
    assert_eq!(elide("abcdef", 10), "abcdef");
    assert_eq!(elide("abcdef", 4), "abc…");
    // CJK/эмодзи занимают две ячейки: «日本語» — это 6 ячеек
    assert_eq!(elide("日本語テスト", 6), "日本…");
    assert_eq!(elide("🎵🎵🎵🎵", 5), "🎵🎵…");
}

// ------------------------------------------------- режимы ytm / ytt

#[test]
fn search_target_depends_on_mode() {
    use crate::mode::Mode;
    let music = yt::search_target("daft punk", Mode::Music);
    assert!(music.starts_with("https://music.youtube.com/search?q="));
    assert!(music.ends_with("daft+punk"));

    let video = yt::search_target("daft punk", Mode::Youtube);
    assert_eq!(video, "ytsearch25:daft punk");
}

#[test]
fn format_selector_follows_video_flag() {
    assert_eq!(yt::format_selector(false), "bestaudio/best");
    assert!(yt::format_selector(true).starts_with("best[height<=?1080]"));
}

#[test]
fn mpv_args_per_mode() {
    use crate::mpv::{spawn_args, PlayerOptions};

    // музыка: видео выключено на уровне VO
    let music = spawn_args(
        PlayerOptions {
            volume: 80,
            allow_video: false,
            start_video: false,
        },
        "/tmp/s.sock",
    );
    assert!(music.iter().any(|a| a == "--no-video"));
    assert!(music.iter().any(|a| a == "--input-ipc-server=/tmp/s.sock"));
    assert!(music.iter().any(|a| a == "--volume=80"));

    // youtube, звук: VO готов, но дорожка выключена
    let yt_audio = spawn_args(
        PlayerOptions {
            volume: 80,
            allow_video: true,
            start_video: false,
        },
        "/tmp/s.sock",
    );
    assert!(!yt_audio.iter().any(|a| a == "--no-video"));
    assert!(yt_audio.iter().any(|a| a == "--vid=no"));

    // youtube, видео: ничего не выключаем — окно появится само
    let yt_video = spawn_args(
        PlayerOptions {
            volume: 80,
            allow_video: true,
            start_video: true,
        },
        "/tmp/s.sock",
    );
    assert!(!yt_video.iter().any(|a| a == "--no-video"));
    assert!(!yt_video.iter().any(|a| a == "--vid=no"));
}

#[test]
fn cli_flags_per_mode() {
    use crate::cli::parse_args;
    use crate::mode::Mode;

    let o = parse_args(
        Mode::Youtube,
        ["--video", "daft", "punk"].iter().map(|s| s.to_string()),
    )
    .unwrap();
    assert!(o.video);
    assert_eq!(o.query.as_deref(), Some("daft punk"));

    let o = parse_args(
        Mode::Youtube,
        ["--protocol=kitty", "-p", "sixel"]
            .iter()
            .map(|s| s.to_string()),
    )
    .unwrap();
    assert_eq!(o.protocol.as_deref(), Some("sixel"));

    // в музыкальном режиме окна видео нет — флаг должен ругаться
    assert!(parse_args(Mode::Music, ["--video"].iter().map(|s| s.to_string())).is_err());
    assert!(
        parse_args(Mode::Music, ["--halfblocks"].iter().map(|s| s.to_string()))
            .unwrap()
            .halfblocks
    );
}

#[test]
fn renders_youtube_mode() {
    use crate::mode::Mode;

    let mut app = App::new(Picker::halfblocks(), "Halfblocks".into(), Mode::Youtube);
    app.video = true;
    app.results = sample();
    app.results_state.select(Some(0));
    app.now = Some(app.results[1].clone());
    app.playing = true;
    app.pos = 10.0;
    app.dur = 337.0;

    let out = dump(&mut app, 120, 30);
    println!("\n{out}");
    assert!(out.contains("ytt"), "в шапке нет имени бинаря");
    assert!(out.contains("youtube"), "нет подписи режима");
    assert!(out.contains("VID"), "нет индикатора видео");
    assert!(out.contains("видео"), "нет подсказки про клавишу v");
}

#[test]
#[ignore = "нужен yt-dlp и сеть"]
fn live_youtube_mode_search() {
    use crate::mode::Mode;
    let tracks = yt::search("daft punk get lucky", Mode::Youtube).expect("поиск не должен падать");
    assert!(!tracks.is_empty());
    let first = &tracks[0];
    println!("{first:#?}");
    // обычный поиск YouTube отдаёт канал и длительность
    assert!(first.duration > 0.0, "нет длительности");
    assert!(!first.artist.is_empty(), "нет канала/исполнителя");
    assert!(first.thumb.is_some(), "нет обложки");
}

/// Проверка видео-ветки: mpv должен открыть окно и играть файл.
/// Путь к тестовому ролику передаётся в YTMT_TEST_VIDEO (нужен X-дисплей).
#[test]
#[ignore = "нужны mpv, X-дисплей и ролик в YTMT_TEST_VIDEO"]
fn live_mpv_video() {
    use crate::mpv::{spawn_args, Event, Player, PlayerOptions};
    use std::time::{Duration, Instant};

    let Ok(path) = std::env::var("YTMT_TEST_VIDEO") else {
        return;
    };
    let opts = PlayerOptions {
        volume: 30,
        allow_video: true,
        start_video: true,
    };
    // аргументы не должны ничего выключать
    let args = spawn_args(opts, "/tmp/x.sock");
    assert!(!args.iter().any(|a| a == "--vid=no" || a == "--no-video"));

    let player = Player::spawn(opts).expect("mpv должен подняться");
    player.load(&path);

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut best = 0.0f64;
    while Instant::now() < deadline && best < 1.5 {
        for ev in player.poll() {
            if let Event::TimePos(t) = ev {
                best = best.max(t);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // на живом видео mpv должен сообщить и длительность
    println!("сыграно: {best:.1} c");
    assert!(best >= 1.5, "видео не заиграло");
}
