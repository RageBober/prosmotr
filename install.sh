#!/usr/bin/env bash
# Установка и обновление prosmotr из GitHub Releases.
#
#   ./install.sh            поставить или обновить до последней версии
#   ./install.sh --check    только посмотреть, что установлено и что доступно
#   ./install.sh --binary   без пакета: положить бинарник в ~/.local/bin
#
# Скрипт ничего не собирает — берёт готовый .deb, собранный в CI.

set -euo pipefail

REPO="${PROSMOTR_REPO:-RageBober/prosmotr-}"
API="${PROSMOTR_API:-https://api.github.com/repos/$REPO/releases/latest}"
MODE="${1:-install}"

say()  { printf '%s\n' "$*"; }
die()  { printf 'Ошибка: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "нужна программа $1"; }

need curl

# --- что стоит сейчас ---
installed="$(dpkg-query -W -f='${Version}' prosmotr 2>/dev/null || true)"
[ -n "$installed" ] || installed="нет"

# --- что лежит в релизах ---
json="$(curl -fsSL "$API" 2>/dev/null)" || die "не достучался до GitHub. Проверьте сеть"
tag="$(printf '%s' "$json" | grep -m1 '"tag_name"' | cut -d'"' -f4 || true)"
[ -n "$tag" ] || die "в репозитории $REPO ещё нет ни одного релиза"
latest="${tag#v}"

deb_url="$(printf '%s' "$json" | grep -o 'https://[^"]*\.deb' | head -n1 || true)"
bin_url="$(printf '%s' "$json" | grep -o 'https://[^"]*prosmotr-x86_64' | head -n1 || true)"

say "установлено: $installed"
say "в релизах:   $latest"

if [ "$MODE" = "--check" ]; then
  exit 0
fi

# --- уже свежее некуда? ---
if [ "$installed" != "нет" ] && command -v dpkg >/dev/null 2>&1; then
  if dpkg --compare-versions "$installed" ge "$latest"; then
    say "Обновление не нужно — установленная версия не старее."
    exit 0
  fi
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# --- вариант без пакета: просто бинарник в ~/.local/bin ---
if [ "$MODE" = "--binary" ]; then
  [ -n "$bin_url" ] || die "в релизе $tag нет отдельного бинарника"
  say "Скачиваю бинарник…"
  curl -fL --progress-bar "$bin_url" -o "$tmp/prosmotr"
  mkdir -p "$HOME/.local/bin"
  install -m 755 "$tmp/prosmotr" "$HOME/.local/bin/prosmotr"
  say "Готово: $HOME/.local/bin/prosmotr"
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) say "Добавьте в ~/.bashrc:  export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
  esac
  say "Нужен системный WebKit: sudo apt install libwebkit2gtk-4.1-0"
  exit 0
fi

# --- обычный путь: .deb ---
[ -n "$deb_url" ] || die "в релизе $tag нет .deb — попробуйте ./install.sh --binary"
command -v dpkg >/dev/null 2>&1 || die "это не Debian/Ubuntu — используйте ./install.sh --binary"

say "Скачиваю $tag…"
curl -fL --progress-bar "$deb_url" -o "$tmp/prosmotr.deb"

say "Ставлю (потребуется пароль sudo)…"
sudo apt-get install -y "$tmp/prosmotr.deb"

say ""
say "Готово. Запуск: prosmotr ~/Документы"
say "Или из меню приложений — «Просмотрщик»."
