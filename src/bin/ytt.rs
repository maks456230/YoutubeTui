//! `ytt` — плеер обычного YouTube в терминале (со звуком и, по желанию, видео).

fn main() -> anyhow::Result<()> {
    ytmt::cli::main_for(ytmt::mode::Mode::Youtube)
}
