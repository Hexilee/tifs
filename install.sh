#!/usr/bin/env sh

DOAS=$(which sudo || which doas)
$DOAS install ./bin/release/mount /sbin/mount.tifs
