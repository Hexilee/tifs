#!/usr/bin/env bash
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    install ./target/release/tifs /sbin/mount.tifs
elif [[ "$OSTYPE" == "darwin"* ]]; then
    install ./target/release/tifs /sbin/mount_tifs
else
    echo "unsupported OS type: $OSTYPE"
    exit 1
fi
