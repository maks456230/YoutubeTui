//! Разбор флагов, выбор протокола картинок, запуск терминала и цикл событий.
//! Общий код для обоих бинарей: `ytm` и `ytt`.

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui_image::picker::{Picker, ProtocolType};

use crate::app::App;
use crate::mode::Mode;
use crate::ui;
use crate::yt;

#[derive(Default)]
pub struct Options {
    pub query: Option<String>,
    pub halfblocks: bool,
    pub protocol: Option<String>,
    /// стартовать сразу с окном видео (только для `ytt`)
    pub video: bool,
}

pub fn parse_args<I: Iterator<Item = String>>(mode: Mode, args: I) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut words: Vec<String> = Vec::new();
    let mut it = args.peekable();

    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print_help(mode);
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!(
                    "{} {} · MIT · © Maksim23Gamer",
                    mode.bin(),
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(0);
            }
            "--halfblocks" | "--no-images" => opts.halfblocks = true,
            "--video" | "-v" if mode.allows_video() => opts.video = true,
            other if other.starts_with("--protocol=") => {
                opts.protocol = Some(other["--protocol=".len()..].to_string());
            }
            "--protocol" | "-p" => {
                let Some(v) = it.next() else {
                    return Err("--protocol требует значение: kitty|sixel|iterm2|halfblocks".into());
                };
                opts.protocol = Some(v);
            }
            other if other.starts_with('-') => {
                return Err(format!("неизвестный флаг: {other}"));
            }
            other => words.push(other.to_string()),
        }
    }

    if !words.is_empty() {
        opts.query = Some(words.join(" "));
    }
    Ok(opts)
}

pub fn print_help(mode: Mode) {
    let bin = mode.bin();
    let video_flags = if mode.allows_video() {
        "
  --video, -v          сразу открыть окно видео (mpv);
                       в приложении переключается клавишей v
  -V"
    } else {
        "
  -V"
    };
    println!(
        "{bin} {} — {sub} в терминале, с настоящими обложками

Использование:
  {bin}                    # открыть пустое окно, / — поиск
  {bin} daft punk          # сразу найти
  {bin} \"https://{host}/watch?v=...\"   # играть ссылку и вообще любую ссылку YouTube

Флаги:
  --halfblocks         не использовать графический протокол терминала
                       (полезно, если картинки «мажут» или терминал врёт)
  -p, --protocol=NAME  принудительно: kitty | sixel | iterm2 | halfblocks
                       (или переменная окружения YTM_PROTOCOL){video_flags}, --version
  -h, --help

Требуется:
  mpv      — воспроизведение (0.36+)
  yt-dlp   — поиск и поток
  curl или wget — загрузка обложек

Картинки: kitty / sixel / iTerm2 (kitty, ghostty, wezterm, foot, iTerm2, mlterm,
Konsole с sixel). Если терминал не умеет — фолбэк на unicode-halfblock
(это всё ещё пиксели, но крупными «квадратиками»).",
        env!("CARGO_PKG_VERSION"),
        sub = match mode {
            Mode::Music => "YouTube Music",
            Mode::Youtube => "YouTube",
        },
        host = match mode {
            Mode::Music => "music.youtube.com",
            Mode::Youtube => "www.youtube.com",
        },
    );
    println!("\nАвтор: Maksim23Gamer · лицензия MIT (файл LICENSE в репозитории)");
}

/// Терминалы, про которые известно, что они умеют графику и точно ответят
/// на запрос возможностей. Остальным графику не предлагаем.
fn terminal_may_support_graphics() -> bool {
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    let program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_lowercase();
    let capable_programs = [
        "iterm.app",
        "wezterm",
        "ghostty",
        "rio",
        "kitty",
        "blackbox",
        "bobcat",
    ];
    capable_programs.contains(&program.as_str())
        || ["kitty", "ghostty", "foot", "sixel", "mlterm", "wezterm"]
            .iter()
            .any(|k| term.contains(k))
}

/// Собираем Picker. Важно: `Picker::from_query_stdio()` заводит поток с
/// блокирующим чтением stdin и НЕ завершает его, если терминал не ответил —
/// после этого клавиатура в приложении перестаёт работать. Поэтому спрашиваем
/// терминал только если он опознан как «умеющий», либо если пользователь явно
/// попросил протокол.
pub fn build_picker(opts: &Options) -> (Picker, Option<String>) {
    let forced = opts
        .protocol
        .clone()
        .or_else(|| yt::env_first(&["YTM_PROTOCOL", "YTMT_PROTOCOL"]))
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());

    if opts.halfblocks || forced.as_deref() == Some("halfblocks") {
        return (Picker::halfblocks(), None);
    }

    let forced_proto = match forced.as_deref() {
        Some("kitty") => Some(ProtocolType::Kitty),
        Some("sixel") => Some(ProtocolType::Sixel),
        Some("iterm2") | Some("iterm") => Some(ProtocolType::Iterm2),
        Some(other) if !other.is_empty() => {
            return (
                Picker::halfblocks(),
                Some(format!(
                    "неизвестный протокол «{other}» — рисую полублоками"
                )),
            );
        }
        _ => None,
    };

    if let Some(proto) = forced_proto {
        if let Ok(mut p) = Picker::from_query_stdio() {
            p.set_protocol_type(proto);
            return (p, None);
        }
        let mut p = Picker::halfblocks();
        p.set_protocol_type(proto);
        return (p, None);
    }

    if terminal_may_support_graphics() {
        if let Ok(p) = Picker::from_query_stdio() {
            return (p, None);
        }
    }

    (
        Picker::halfblocks(),
        Some("обложки: halfblocks · --protocol=kitty|sixel|iterm2".into()),
    )
}

/// Точка входа обоих бинарей.
pub fn main_for(mode: Mode) -> anyhow::Result<()> {
    let opts = match parse_args(mode, std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}\n\n{} --help для справки", mode.bin());
            std::process::exit(2);
        }
    };

    if !yt::have("mpv") {
        eprintln!("⚠ mpv не найден в PATH — играть не получится (поставь mpv).");
    }
    if !yt::have("yt-dlp") {
        eprintln!("⚠ yt-dlp не найден в PATH — поиск работать не будет (pipx install yt-dlp).");
    }

    // raw mode + alternate screen; restore вызывается и при панике
    let mut terminal = ratatui::init();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        ratatui::restore();
        hook(p);
    }));

    let (picker, note) = build_picker(&opts);
    let proto = format!("{:?}", picker.protocol_type());

    let mut app = App::new(picker, proto, mode);
    app.video = mode.allows_video() && opts.video;
    if let Some(note) = note {
        app.status = note;
    }
    if let Some(q) = opts.query.clone() {
        app.start_query(&q);
    }
    app.ensure_cover();

    let res = run(&mut terminal, &mut app);
    ratatui::restore();
    res
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    loop {
        app.pump();
        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(Duration::from_millis(90))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
