# Maintainer: scubba
#
# ewe-mobile — your phone on your desktop: an ewe window over KDE Connect's
# daemon. Tauri v2 has no pacman bundler, so this PKGBUILD is how it is
# packaged for Arch, exactly like ewe Sync, Komble and ewe Settings. No
# privileged helper: everything it does is a D-Bus call on the user's session.

pkgname=ewe-mobile
# The git TAG and the pacman pkgver are different strings and always will
# be: a tag may carry -beta, an Arch pkgver may not contain a hyphen. Of the
# legal spellings only 0.1.0beta sorts BELOW the eventual 0.1.0 under vercmp.
# The archive is addressed by the TAG and extracts to <repo>-<tag without the
# leading v>, so the source URL and the directory below follow _tag, never
# pkgver. The release workflow rewrites both.
_tag=v0.1.0-beta
pkgver=0.1.0beta
pkgrel=1
pkgdesc="ewe Mobile — your phone on your desktop: messages, contacts, notifications, media and files"
arch=('x86_64' 'aarch64')
url="https://github.com/prj786/ewe-mobile"
license=('MIT')

depends=(
  'webkit2gtk-4.1'
  'gtk3'
  # kdeconnectd keeps the link to the phone; ewe Mobile is its window. The
  # package's own apps (kdeconnect-app, -sms, -indicator) come with it and
  # stay out of the way.
  'kdeconnect'
  # KDE Connect mounts the phone's storage with it (Files → Open). Optional
  # for kdeconnect itself; here "Browse its files" should simply work.
  'sshfs'
)
optdepends=(
  'ewe: the desktop this follows the look of (ewe-theme), and the ewe.phone plugin for the bar'
  'ewe-apps: ewe Files opens the phone’s storage and shows received files'
)
makedepends=('rust' 'cargo' 'nodejs' 'npm')

# Arch's makepkg LTO breaks linking some crates' assembly (see Komble's
# PKGBUILD for the full story); Rust-level LTO stays on in Cargo.toml.
options=(!lto !debug)

source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/$_tag.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$srcdir/$pkgname-${_tag#v}"
  npm ci
  # Through the Tauri CLI, never bare `cargo build`: tauri-build decides
  # dev-vs-production at compile time and a plain cargo build bakes the vite
  # devUrl into the binary. --no-bundle: this PKGBUILD is the packaging.
  npm run tauri build -- --no-bundle
}

package() {
  cd "$srcdir/$pkgname-${_tag#v}"

  install -Dm755 src-tauri/target/release/ewe-mobile "$pkgdir/usr/bin/ewe-mobile"

  install -Dm644 packaging/io.github.prj786.ewe-mobile.desktop \
    "$pkgdir/usr/share/applications/io.github.prj786.ewe-mobile.desktop"
  for s in 32 64 128 256; do
    install -Dm644 "src-tauri/icons/${s}x${s}.png" \
      "$pkgdir/usr/share/icons/hicolor/${s}x${s}/apps/io.github.prj786.ewe-mobile.png"
  done
  install -Dm644 packaging/ewe-mobile.svg \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/io.github.prj786.ewe-mobile.svg"

  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  # the bundled faces (compiled into the binary's frontend): Geist is OFL
  install -Dm644 src/assets/GEIST-LICENSE "$pkgdir/usr/share/licenses/$pkgname/GEIST-LICENSE"
  install -Dm644 src/assets/LUCIDE-LICENSE "$pkgdir/usr/share/licenses/$pkgname/LUCIDE-LICENSE"
}
