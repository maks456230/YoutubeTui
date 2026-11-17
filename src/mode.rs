//! Режим работы: каталог YouTube Music или обычный YouTube.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// `ytm` — music.youtube.com
    Music,
    /// `ytt` — youtube.com
    Youtube,
}

impl Mode {
    pub fn bin(self) -> &'static str {
        match self {
            Mode::Music => "ytm",
            Mode::Youtube => "ytt",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Mode::Music => "youtube music",
            Mode::Youtube => "youtube",
        }
    }

    /// Может ли режим в принципе показывать видео в окне mpv.
    pub fn allows_video(self) -> bool {
        matches!(self, Mode::Youtube)
    }
}
