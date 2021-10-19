#!/usr/bin/env sh
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    install ./target/release/mount /sbin/mount.tifs
elif [[ "$OSTYPE" == "darwin"* ]]; then
    install ./target/release/mount /sbin/mount_tifs
else
    echo "unsupported OS type: $OSTYPE"
    exit 1
fi