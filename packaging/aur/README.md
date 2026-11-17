# AUR-пакеты

Два пакета, оба ставят бинари `ytm` и `ytt`:

| пакет | что делает |
|---|---|
| `ytm-tui` | собирает из исходников тега GitHub (`depends`: `cargo`, `rust`) |
| `ytm-tui-bin` | берёт готовый архив из GitHub Releases (только `x86_64`, без компиляции) |

Пакеты взаимоисключающие (`conflicts`/`provides`), ставить нужно один.

## Первая публикация

```bash
# 1. аккаунт AUR + ssh-ключ: https://aur.archlinux.org (My Account → SSH keys)
# 2. клонируем пустой AUR-репозиторий (появится после первого push)
git clone ssh://aur@aur.archlinux.org/ytm-tui.git
cd ytm-tui
cp /path/to/ytmt/packaging/aur/ytm-tui/PKGBUILD .
cp /path/to/ytmt/packaging/aur/ytm-tui/.SRCINFO .

# 3. проверяем локально (нужен Arch): соберётся и поставится
makepkg -si

# 4. фиксируем актуальные суммы/метаданные и публикуем
updpkgsums                      # заменит SKIP на реальный sha256 исходника
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "ytm-tui 0.2.0"
git push
```

То же самое для `ytm-tui-bin` (только сначала должен существовать релиз `v0.2.0` на GitHub —
его создаёт workflow `.github/workflows/release.yml` при пуше тега).

## Обновление версии

```bash
# в PKGBUILD: pkgver=0.3.0, pkgrel=1
updpkgsums
makepkg --printsrcinfo > .SRCINFO
git commit -am "ytm-tui 0.3.0" && git push
```

Для `ytm-tui-bin` сумму архива печатает job `checksums` в release-workflow —
скопируй оттуда `sha256sums_x86_64`.

## Замечания

- `.SRCINFO` руками не правят: он всегда генерируется из `PKGBUILD`
  (`makepkg --printsrcinfo > .SRCINFO`). CI проверяет, что файл в репозитории
  совпадает с тем, что генерируется.
- Имя пакета `ytm-tui` выбрано так, чтобы не конфликтовать с бинарями `ytm`/`ytt`
  (они объявлены в `provides`).
- Зависимости `mpv` и `yt-dlp` — обязательные: без них плеер работать не будет
  (см. основной README).
