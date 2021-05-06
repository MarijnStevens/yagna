#!/bin/bash


# 
# packages:  libssl-dev:aarch64 
# sudo apt-get install libglib2.0


#TARGET="aarch64-unknown-linux-gnu"  
TARGET="armv7-unknown-linux-gnueabihf"
#TARGET="arm-unknown-linux-gnueabihf"

PKG_CONFIG_ALLOW_CROSS=1 OPENSSL_STATIC=true \
cargo build --release --target=$TARGET -p yagna -p ya-exe-unit -p ya-provider -p golemsp -p gftp
