#!/bin/bash

# Configuration
APP_NAME="autoencode"
VERSION="1.0.0"
ARCH="amd64"
MAINTAINER="SirenityK <eldarielmario@gmail.com>"
DESCRIPTION="Intelligent video encoder for WhatsApp and Discord"

# Directory setup
DIR_NAME="${APP_NAME}_${VERSION}_${ARCH}"
mkdir -p "$DIR_NAME/usr/local/bin"
mkdir -p "$DIR_NAME/DEBIAN"

echo "1. Cross-compiling for Linux/$ARCH..."
# GOOS=linux and GOARCH=amd64 enables cross-compilation automatically
env GOOS=linux GOARCH=amd64 go build -o "$DIR_NAME/usr/local/bin/$APP_NAME" main.go

echo "2. Generating Control File..."
cat > "$DIR_NAME/DEBIAN/control" <<EOF
Package: $APP_NAME
Version: $VERSION
Architecture: $ARCH
Maintainer: $MAINTAINER
Description: $DESCRIPTION
Depends: ffmpeg, bash
EOF

# Optional: Add post-installation script to generate completions
# mkdir -p "$DIR_NAME/DEBIAN"
# cat > "$DIR_NAME/DEBIAN/postinst" <<EOF
# #!/bin/bash
# autoencode completion bash > /etc/bash_completion.d/autoencode
# EOF
# chmod 755 "$DIR_NAME/DEBIAN/postinst"

echo "3. Building .deb package..."
dpkg-deb --build "$DIR_NAME"

echo "Done! Package created: ${DIR_NAME}.deb"
echo "Install with: sudo dpkg -i ${DIR_NAME}.deb"

# Cleanup
rm -rf "$DIR_NAME"
