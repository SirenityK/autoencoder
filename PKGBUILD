# Maintainer: Your Name <you@example.com>
pkgname=autoencode
pkgver=2.0.0
pkgrel=2
pkgdesc="Intelligent video encoder for WhatsApp and Discord"
arch=('x86_64' 'aarch64')
url="https://git.dariel.lat/sirenityk/autoencoder"
license=('MIT')
depends=('ffmpeg' 'svt-av1')
makedepends=('cargo')
# Empty source because we use local files
source=()
sha256sums=()

prepare() {
    # Copy the crate into a subdir of $srcdir: the repo's source lives in
    # src/, which collides with makepkg's own $srcdir (also named src/).
    # Copy entries one by one, skipping the dest dir we just created.
    mkdir -p "$srcdir/autoencode/src"
    cp "$startdir"/Cargo.toml "$startdir"/Cargo.lock "$srcdir/autoencode/"
    for entry in "$startdir"/src/*; do
        [ "$entry" = "$srcdir/autoencode" ] && continue
        cp -r "$entry" "$srcdir/autoencode/src/"
    done
}

build() {
    cd "$srcdir/autoencode"
    cargo build --release --locked
}

package() {
    cd "$srcdir/autoencode"

    # Install binary
    install -Dm755 target/release/autoencode "$pkgdir/usr/bin/autoencode"

    # Generate and install completions
    install -dm755 "$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "$pkgdir/usr/share/fish/vendor_completions.d"

    # Generate completions using the built binary
    ./target/release/autoencode completion bash > "$pkgdir/usr/share/bash-completion/completions/autoencode"
    ./target/release/autoencode completion zsh > "$pkgdir/usr/share/zsh/site-functions/_autoencode"
    ./target/release/autoencode completion fish > "$pkgdir/usr/share/fish/vendor_completions.d/autoencode.fish"
}
