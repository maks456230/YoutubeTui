//! `ytm` — плеер YouTube Music в терминале.

fn main() -> anyhow::Result<()> {
    ytmt::cli::main_for(ytmt::mode::Mode::Music)
}
