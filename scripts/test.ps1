Param(
    [switch]$Unit,
    [switch]$Contract,
    [switch]$Integration,
    [switch]$E2E,
    [switch]$Verbose,
    [switch]$Help
)

if ($Help) {
    Write-Host "Usage: ./scripts/test.ps1 [options]"
    Write-Host "Options:"
    Write-Host "  -Unit          Run only Unit Testing"
    Write-Host "  -Contract      Run only Contract Testing"
    Write-Host "  -Integration   Run only Integration Testing"
    Write-Host "  -E2E           Run only E2E Testing"
    Write-Host "  -Verbose       Show complete output"
    Write-Host "  -Help          Show help message"
    exit 0
}

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $RepoRoot

if (-not $Unit -and -not $Contract -and -not $Integration -and -not $E2E) {
    $Unit = $true
    $Contract = $true
    $Integration = $true
    $E2E = $true
}

$TotalCount = 0
$PassedCount = 0
$FailedCount = 0

function Run-Suite {
    param(
        [string]$Label,
        [scriptblock]$Command
    )
    $script:TotalCount++

    $output = & $Command 2>&1
    if ($LASTEXITCODE -eq 0) {
        $script:PassedCount++
        Write-Host "  `e[32m✔`e[0m $Label"
        if ($Verbose) {
            $output | ForEach-Object { Write-Host $_ }
        }
    } else {
        $script:FailedCount++
        Write-Host "  `e[31m✖`e[0m $Label"
        $output | ForEach-Object { Write-Host $_ }
        Write-Host ""
    }
}

Write-Host "`n`e[1mRunning Tests...`e[0m`n"

if ($Unit) {
    Run-Suite "Unit Testing" { cargo test --workspace }
}

if ($Integration -or $E2E) {
    cargo build -p api-gateway --quiet
}

if ($Contract) {
    Run-Suite "Contract Testing" { Push-Location test; bun run test:contract; Pop-Location }
}

if ($Integration) {
    Run-Suite "Integration Testing" { Push-Location test; bun run test:integration; Pop-Location }
}

if ($E2E) {
    Run-Suite "E2E Testing" { Push-Location test; bun run test:e2e; Pop-Location }
}

Write-Host ""
if ($FailedCount -eq 0) {
    Write-Host "`e[32m`e[1m✔ All test suites passed ($PassedCount/$TotalCount)`e[0m`n"
    exit 0
} else {
    Write-Host "`e[31m`e[1m✖ $FailedCount of $TotalCount test suites failed`e[0m`n"
    exit 1
}
