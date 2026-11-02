#!/usr/bin/env bash
# Gera os instaladores do FeatherPDF em pacote/saida/, com os nomes fixos que o
# launcher procura na release (releases/latest/download/<nome>):
#   featherpdf-linux-x86_64.tar.gz   Linux portátil (instalar.sh na pasta do usuário)
#   featherpdf-linux-amd64.deb       Debian, Ubuntu, Mint…
#   FeatherPDF-windows-x64.exe       Windows 10/11 (instalador por usuário, sem administrador)
#   SHA256SUMS
#
# Uso: pacote/gerar.sh [linux|windows ...]   (sem argumentos = todos)
# Precisa de: cargo, dpkg-deb, objdump; podman ou docker para o Linux (sem eles compila aqui,
# e o .deb só instala em distros com a glibc desta máquina); para Windows, o alvo
# x86_64-pc-windows-gnu, mingw-w64 e makensis.
set -euo pipefail

pacote="$(cd "$(dirname "$0")" && pwd)"
raiz="$(dirname "$pacote")"
alvo="${CARGO_TARGET_DIR:-$raiz/target}"
versao="$(sed -n 's/^version = "\(.*\)"/\1/p' "$raiz/Cargo.toml" | head -1)"
saida="$pacote/saida"
alvos=("$@")
[[ ${#alvos[@]} -eq 0 ]] && alvos=(linux windows)
rm -rf "$saida" && mkdir -p "$saida"
echo "FeatherPDF $versao → $saida"

# O binário pede a glibc de onde foi compilado: num Debian 12 (2.36) ele roda no Ubuntu 22.04+,
# Debian 12+ e Mint 21+. Compilado aqui pediria a glibc desta máquina.
compilar_linux() {
  local motor; motor="$(command -v podman || command -v docker || true)"
  if [[ -z $motor ]]; then
    echo "Sem podman nem docker: compilando aqui (o .deb vai pedir a glibc desta máquina)." >&2
    cargo build --release -p featherpdf-pelicano --manifest-path "$raiz/Cargo.toml"
    bin="$alvo/release/featherpdf-pelicano"
    return
  fi
  # A imagem com o Rust sai uma vez só; o registro de crates fica guardado num volume.
  if ! "$motor" image exists featherpdf-debian12 2>/dev/null && ! "$motor" image inspect featherpdf-debian12 >/dev/null 2>&1; then
    "$motor" build -t featherpdf-debian12 -f - "$pacote" <<'IMG'
FROM docker.io/library/debian:12
RUN apt-get update && apt-get install -y --no-install-recommends gcc libc6-dev curl ca-certificates && rm -rf /var/lib/apt/lists/*
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH=/root/.cargo/bin:$PATH
IMG
  fi
  "$motor" run --rm -v "$raiz":/src:z -v featherpdf-registro:/root/.cargo/registry -e CARGO_TARGET_DIR=/src/target/debian12 -e CARGO_BUILD_JOBS -w /src \
    featherpdf-debian12 cargo build --release -p featherpdf-pelicano
  bin="$raiz/target/debian12/release/featherpdf-pelicano"
}

linux() {
  local bin; compilar_linux
  # A glibc mínima é a maior versão que o binário usa.
  local glibc; glibc="$(objdump -T "$bin" | grep -o 'GLIBC_2\.[0-9]*' | sort -V | tail -1 | cut -d_ -f2)"
  echo "glibc mínima: $glibc"

  # .deb
  local d; d="$(mktemp -d)"
  install -Dm755 "$bin" "$d/usr/bin/featherpdf"
  install -Dm644 "$pacote/featherpdf.desktop" "$d/usr/share/applications/featherpdf.desktop"
  for t in 16 24 32 48 64 128 256 512; do
    install -Dm644 "$pacote/icones/featherpdf-$t.png" "$d/usr/share/icons/hicolor/${t}x${t}/apps/featherpdf.png"
  done
  install -Dm644 "$pacote/icones/featherpdf.svg" "$d/usr/share/icons/hicolor/scalable/apps/featherpdf.svg"
  install -Dm644 "$raiz/LICENSE" "$d/usr/share/doc/featherpdf/copyright"
  local tamanho; tamanho="$(du -sk "$d/usr" | cut -f1)"
  mkdir -p "$d/DEBIAN"
  cat > "$d/DEBIAN/control" <<CTL
Package: featherpdf
Version: $versao
Section: text
Priority: optional
Architecture: amd64
Installed-Size: $tamanho
Depends: libc6 (>= $glibc), libxkbcommon0, libwayland-client0 | libx11-6, libfontconfig1
Recommends: libvulkan1 | libgl1
Maintainer: FeatherWorks <https://github.com/feather-works>
Homepage: https://github.com/feather-works/featherpdf
Description: Editor de PDF da suíte FeatherWorks
 Editor de PDF em Rust, tudo local: ler, anotar, organizar páginas,
 formulários, assinatura digital (PAdES) e OCR.
CTL
  cat > "$d/DEBIAN/postinst" <<'SH'
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
SH
  cp "$d/DEBIAN/postinst" "$d/DEBIAN/postrm"
  chmod 755 "$d/DEBIAN/postinst" "$d/DEBIAN/postrm"
  dpkg-deb --root-owner-group --build "$d" "$saida/featherpdf-linux-amd64.deb" >/dev/null
  rm -rf "$d"

  # .tar.gz portátil: "<app>-<versão>/instalar.sh", o mesmo que o launcher roda.
  local t; t="$(mktemp -d)/featherpdf-$versao"
  mkdir -p "$t"
  cp "$bin" "$t/featherpdf"
  cp "$pacote/featherpdf.desktop" "$raiz/LICENSE" "$t/"
  cp -r "$pacote/icones" "$t/"
  cat > "$t/instalar.sh" <<'SH'
#!/bin/sh
# Instala o FeatherPDF só para este usuário (~/.local), sem administrador.
set -e
cd "$(dirname "$0")"
mkdir -p ~/.local/bin ~/.local/share/applications
install -m755 featherpdf ~/.local/bin/featherpdf
sed "s#^Exec=featherpdf#Exec=$HOME/.local/bin/featherpdf#" featherpdf.desktop > ~/.local/share/applications/featherpdf.desktop
for t in 16 24 32 48 64 128 256 512; do
  mkdir -p ~/.local/share/icons/hicolor/${t}x${t}/apps
  cp icones/featherpdf-$t.png ~/.local/share/icons/hicolor/${t}x${t}/apps/featherpdf.png
done
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp icones/featherpdf.svg ~/.local/share/icons/hicolor/scalable/apps/featherpdf.svg
# Só entra no "Abrir com" (MimeType= do .desktop); o leitor de PDF padrão continua o da pessoa.
update-desktop-database ~/.local/share/applications 2>/dev/null || true
echo "FeatherPDF instalado. Procure por \"FeatherPDF\" no menu de aplicativos."
SH
  chmod 755 "$t/instalar.sh"
  tar -C "$(dirname "$t")" -czf "$saida/featherpdf-linux-x86_64.tar.gz" "featherpdf-$versao"
  rm -rf "$(dirname "$t")"
}

windows() {
  cargo build --release -p featherpdf-pelicano --target x86_64-pc-windows-gnu --manifest-path "$raiz/Cargo.toml"
  local exe="$alvo/x86_64-pc-windows-gnu/release/featherpdf-pelicano.exe"
  x86_64-w64-mingw32-strip "$exe" 2>/dev/null || true
  (cd "$pacote" && makensis -V2 -DVERSAO="$versao" -DEXE="$exe" -DSAIDA="$saida/FeatherPDF-windows-x64.exe" featherpdf.nsi)
}

for a in "${alvos[@]}"; do
  echo "== $a"
  "$a"
done
(cd "$saida" && sha256sum -- * | grep -v SHA256SUMS > SHA256SUMS)
ls -lh "$saida"
