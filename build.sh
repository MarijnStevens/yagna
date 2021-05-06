#!/bin/bash


# 
# packages:  libssl-dev:aarch64 
# sudo apt-get install libglib2.0
# ARCH="aarch64"

ARCH="armv7"
TARGET="$ARCH-unknown-linux-gnueabihf"
TARGET_DIR="./target/$TARGET/release/"

PKG_CONFIG_ALLOW_CROSS=1 OPENSSL_STATIC=true \
cargo build --release --target=$TARGET -p yagna -p ya-exe-unit -p ya-provider -p golemsp -p gftp


echo -e Export to $TARGET_DIR

echo -e $(readelf --arch-specific "$TARGET_DIR/yagna") && \

echo -e Export to $TARGET_DIR
\cp "$TARGET_DIR/exe-unit" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/gftp" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/yagna" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/golemsp" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/libgftp.rlib" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/libya_exe_unit.rlib" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/libgftp.rlib" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/libya_provider.rlib" "../yagna-binaries/$ARCH/"
\cp "$TARGET_DIR/ya-provider" "../yagna-binaries/$ARCH/"
