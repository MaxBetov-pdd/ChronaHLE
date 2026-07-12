#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

apply_vendor_patch() {
    local submodule="$1"
    local patch="$2"

    if git -C "$repository_root/$submodule" apply --check "$repository_root/$patch" 2>/dev/null; then
        git -C "$repository_root/$submodule" apply "$repository_root/$patch"
        echo "Applied $patch"
    elif git -C "$repository_root/$submodule" apply --reverse --check "$repository_root/$patch" 2>/dev/null; then
        echo "Already applied: $patch"
    else
        echo "$patch does not match the checked-out $submodule revision" >&2
        exit 1
    fi
}

apply_vendor_patch "vendor/dynarmic" "vendor-patches/dynarmic-neon-high-narrow.patch"
apply_vendor_patch "vendor/SDL" "vendor-patches/sdl-android-poll-once.patch"
