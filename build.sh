#!/bin/bash

ARCH="aarch64"
TARGET="$ARCH-unknown-linux-gnu"
TARGET_DIR="./target/$TARGET/release"

PKG_CONFIG_ALLOW_CROSS=1 OPENSSL_STATIC=true \
cargo build --release --target=$TARGET -p yagna -p ya-exe-unit -p ya-provider -p golemsp -p gftp

mkdir -p ../yagna-binaries/ && \
\cp "$TARGET_DIR/exe-unit" "../yagna-binaries/" && \
\cp "$TARGET_DIR/gftp" "../yagna-binaries/" && \
\cp "$TARGET_DIR/yagna" "../yagna-binaries/" && \
\cp "$TARGET_DIR/golemsp" "../yagna-binaries/" && \
\cp "$TARGET_DIR/ya-provider" "../yagna-binaries/" 
