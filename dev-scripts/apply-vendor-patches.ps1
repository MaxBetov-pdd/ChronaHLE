$ErrorActionPreference = "Continue"

$repositoryRoot = Split-Path -Parent $PSScriptRoot

function Apply-VendorPatch {
    param(
        [Parameter(Mandatory)] [string] $Submodule,
        [Parameter(Mandatory)] [string] $Patch
    )

    $submodulePath = Join-Path $repositoryRoot $Submodule
    $patchPath = Join-Path $repositoryRoot $Patch

    & git -C $submodulePath apply --ignore-space-change --ignore-whitespace --check $patchPath 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        & git -C $submodulePath apply --ignore-space-change --ignore-whitespace $patchPath
        if ($LASTEXITCODE -ne 0) { throw "Could not apply $Patch" }
        Write-Host "Applied $Patch"
        return
    }

    & git -C $submodulePath apply --ignore-space-change --ignore-whitespace --reverse --check $patchPath 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Already applied: $Patch"
        return
    }

    throw "$Patch does not match the checked-out $Submodule revision"
}

Apply-VendorPatch "vendor/dynarmic" "vendor-patches/dynarmic-neon-high-narrow.patch"
Apply-VendorPatch "vendor/SDL" "vendor-patches/sdl-android-poll-once.patch"
