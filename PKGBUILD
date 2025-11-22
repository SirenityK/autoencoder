# Maintainer: Your Name <you@example.com>
pkgname=autoencode
pkgver=1.0.0
pkgrel=1
pkgdesc="Intelligent video encoder for WhatsApp and Discord"
arch=('x86_64' 'aarch64')
url="https://git.dariel.lat/sirenityk/autoencoder"
license=('MIT')
depends=('ffmpeg' 'glibc')
makedepends=('go')
# Empty source because we use local files
source=()
sha256sums=()

prepare() {
    # Copy main.go and module files from the directory where makepkg is run ($startdir)
    # to the clean build directory ($srcdir)
    cp "$startdir"/main.go "$srcdir/"
    
    # Try to copy go.mod/go.sum if they exist, otherwise ignore
    cp "$startdir"/go.mod "$srcdir/" 2>/dev/null || true
    cp "$startdir"/go.sum "$srcdir/" 2>/dev/null || true
}

build() {
    cd "$srcdir"
    
    # Initialize module if not present (failsafe for single-file builds)
    if [ ! -f go.mod ]; then
        go mod init autoencode
        go get github.com/spf13/cobra
    fi

    # Arch standards: build with appropriate flags
    export CGO_CPPFLAGS="${CPPFLAGS}"
    export CGO_CFLAGS="${CFLAGS}"
    export CGO_CXXFLAGS="${CXXFLAGS}"
    export CGO_LDFLAGS="${LDFLAGS}"
    export GOFLAGS="-buildmode=pie -trimpath -mod=readonly -modcacherw"

    # If we just initialized the mod locally, we need to allow mod fetch
    if [ -f go.mod ]; then
         export GOFLAGS="-buildmode=pie -trimpath"
    fi

    go build -o autoencode main.go
}

package() {
    cd "$srcdir"
    
    # Install binary
    install -Dm755 autoencode "$pkgdir/usr/bin/autoencode"

    # Generate and install completions
    install -dm755 "$pkgdir/usr/share/bash-completion/completions"
    install -dm755 "$pkgdir/usr/share/zsh/site-functions"
    install -dm755 "$pkgdir/usr/share/fish/vendor_completions.d"

    # Generate completions using the built binary
    ./autoencode completion bash > "$pkgdir/usr/share/bash-completion/completions/autoencode"
    ./autoencode completion zsh > "$pkgdir/usr/share/zsh/site-functions/_autoencode"
    ./autoencode completion fish > "$pkgdir/usr/share/fish/vendor_completions.d/autoencode.fish"
}
