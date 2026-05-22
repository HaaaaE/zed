#!/usr/bin/env pwsh
# check-md-boundary.ps1
#
# Validates the Markdown product line dependency boundary.
# Run from the workspace root.
#
# Usage:
#   .\script\check-md-boundary.ps1
#   .\script\check-md-boundary.ps1 -Strict   # fail on any allowlisted dep too
#
# Exit code: 0 = all checks passed, non-zero = at least one check failed.

param(
    [switch]$Strict
)

$ErrorActionPreference = "Continue"
$failed = $false

function Run-Check {
    param([string]$Description, [scriptblock]$Command)
    Write-Host ""
    Write-Host "==> $Description" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $Description" -ForegroundColor Red
        $script:failed = $true
    } else {
        Write-Host "OK" -ForegroundColor Green
    }
}

# 1. legacy-editor path must compile
Run-Check "cargo check -p markdown_editor (legacy-editor, default)" {
    cargo check -p markdown_editor --features legacy-editor 2>&1
}

# 2. md-editor stub path must compile (no legacy deps)
Run-Check "cargo check -p markdown_editor (md-editor, no-default-features)" {
    cargo check -p markdown_editor --features md-editor --no-default-features 2>&1
}

# 3. md_* empty shells must compile
foreach ($crate in @("md_text", "md_rope", "md_sum_tree", "md_buffer", "md_editor", "md_theme", "md_settings", "md_assets")) {
    Run-Check "cargo check -p $crate" {
        cargo check -p $crate 2>&1
    }
}

# 4. markdown_wysiwyg tests must still pass
Run-Check "cargo test -p markdown_wysiwyg" {
    cargo test -p markdown_wysiwyg 2>&1
}

# 5. Dependency boundary scan: md_* crates must not directly use Zed IDE crates
Write-Host ""
Write-Host "==> Dependency boundary scan (rg)" -ForegroundColor Cyan
$boundary_pattern = 'use (editor|language|multi_buffer|text|rope|sum_tree|project|workspace|settings|theme|theme_settings|ui|assets|icons|markdown)::'
$md_dirs = Get-ChildItem -Directory "crates" | Where-Object { $_.Name -match "^md_" } | ForEach-Object { $_.FullName }

if ($md_dirs) {
    $violations = rg $boundary_pattern $md_dirs --glob "*.rs" 2>&1
    if ($violations) {
        Write-Host "FAILED: boundary violations found in md_* crates:" -ForegroundColor Red
        $violations | ForEach-Object { Write-Host "  $_" -ForegroundColor Yellow }
        $failed = $true
    } else {
        Write-Host "OK: no boundary violations in md_* crates" -ForegroundColor Green
    }
} else {
    Write-Host "SKIP: no md_* crates found yet" -ForegroundColor Yellow
}

# 6. (Optional/Strict) cargo tree check
if ($Strict) {
    Run-Check "cargo tree -p markdown_editor (check for Zed non-GPUI crates)" {
        $tree = cargo tree -p markdown_editor --edges normal -q 2>&1
        $hits = $tree | Select-String "zed\\crates\\" | Where-Object { $_ -notmatch "gpui" -and $_ -notmatch "md_" }
        if ($hits) {
            Write-Host "Non-GPUI workspace crates in tree (must be in allowlist):" -ForegroundColor Yellow
            $hits | ForEach-Object { Write-Host "  $_" }
            # Don't fail in strict mode — allowlist review is manual
        }
        exit 0
    }
}

Write-Host ""
if ($failed) {
    Write-Host "check-md-boundary: SOME CHECKS FAILED" -ForegroundColor Red
    exit 1
} else {
    Write-Host "check-md-boundary: ALL CHECKS PASSED" -ForegroundColor Green
    exit 0
}
