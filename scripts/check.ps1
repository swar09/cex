param (
    [switch]$Fix
)

$ErrorActionPreference = "Continue"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

$Failed = $false
$hasNightly = (rustup toolchain list 2>$null | Select-String "nightly") -ne $null

if (Get-Command typos -ErrorAction SilentlyContinue) {
    if ($Fix) {
        typos --write-changes
        if ($LASTEXITCODE -ne 0) { $Failed = $true }
    } else {
        typos
        if ($LASTEXITCODE -ne 0) { $Failed = $true }
    }
}

if ($Fix) {
    if ($hasNightly) { cargo +nightly fmt --all } else { cargo fmt --all }
    if ($LASTEXITCODE -ne 0) { $Failed = $true }
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
    if ($LASTEXITCODE -ne 0) { $Failed = $true }
} else {
    if ($hasNightly) { cargo +nightly fmt --all -- --check } else { cargo fmt --all -- --check }
    if ($LASTEXITCODE -ne 0) { $Failed = $true }
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { $Failed = $true }
}

if (Get-Command pnpm -ErrorAction SilentlyContinue) {
    if ($Fix) {
        pnpm -r --if-present run lint:fix
    } else {
        pnpm -r --if-present run lint
    }
}

if ($Failed) {
    Write-Host "[x] Checks failed!" -ForegroundColor Red
    exit 1
} else {
    Write-Host "[v] All checks passed!" -ForegroundColor Green
}
