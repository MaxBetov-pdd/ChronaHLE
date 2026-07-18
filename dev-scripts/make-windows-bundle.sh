#!/bin/sh
set -e

# Bundles the ChronaHLE executable with the basic set of files needed to run
# (the same ones found in the Android APK).
# This does not prepare a full release.

if [[ $# == 1 ]]; then
    PATH_TO_BINARY="$1"
    shift

    rm -rf ChronaHLE_windows_bundle
    mkdir ChronaHLE_windows_bundle
    cp "$PATH_TO_BINARY" ChronaHLE_windows_bundle/ChronaHLE.exe
    cp -r ../ChronaHLE_dylibs ChronaHLE_windows_bundle/
    cp -r ../ChronaHLE_fonts ChronaHLE_windows_bundle/
    cp ../ChronaHLE_default_options.txt ChronaHLE_windows_bundle/
    cp ../LICENSE ../FORK_NOTICE.md ../README.md ChronaHLE_windows_bundle/
else
    echo "Incorrect usage."
    exit 1
fi
