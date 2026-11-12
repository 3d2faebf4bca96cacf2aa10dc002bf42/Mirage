#!/usr/bin/env pwsh
# Build helper for Windows.
#
# Why this exists: audiopus_sys compiles libopus with CMake, and opus still
# declares `cmake_minimum_required(VERSION < 3.5)`, which CMake 4 refuses by
# default. CMake 4 accepts the migration when CMAKE_POLICY_VERSION_MINIMUM is set,
# and it honours the variable from the environment.
#
# Requirements: Rust stable (MSVC toolchain), CMake >= 4 on PATH, Visual Studio
# with the C++ workload (MSVC v143+), and a linker that `cargo` can find.
#
# Usage:  pwsh scripts/build.ps1 [cargo args...]   (defaults to `build`)

[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CargoArgs
)

$ErrorActionPreference = 'Stop'
$env:CMAKE_POLICY_VERSION_MINIMUM = '3.5'

Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (-not $CargoArgs -or $CargoArgs.Count -eq 0) {
        $CargoArgs = @('build')
    }
    Write-Host "cargo $($CargoArgs -join ' ')" -ForegroundColor Cyan
    & cargo @CargoArgs
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
