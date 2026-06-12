#Requires -Version 5.1

Write-Host "========================================"
Write-Host "  Rust/ESP32 Build Artifact Cleaner"
Write-Host "  Platform: Windows"
Write-Host "========================================"
Write-Host ""

function Remove-ItemsByPattern {
    param(
        [string]$Name,
        [string]$Type
    )
    $count = 0
    if ($Type -eq "dir") {
        Get-ChildItem -Path . -Directory -Filter $Name -Recurse -Force -ErrorAction SilentlyContinue | ForEach-Object {
            Remove-Item -Path $_.FullName -Recurse -Force -ErrorAction SilentlyContinue
            Write-Host "  [DEL] $($_.FullName)"
            $count++
        }
    } else {
        Get-ChildItem -Path . -File -Filter $Name -Recurse -Force -ErrorAction SilentlyContinue | ForEach-Object {
            Remove-Item -Path $_.FullName -Force -ErrorAction SilentlyContinue
            Write-Host "  [DEL] $($_.FullName)"
            $count++
        }
    }
    Write-Host "  -> ${Name}: $count removed"
}

Write-Host "[1/5] Cleaning target directories..."
Remove-ItemsByPattern -Name "target" -Type "dir"

Write-Host "[2/5] Cleaning .DS_Store files..."
Remove-ItemsByPattern -Name ".DS_Store" -Type "file"

Write-Host "[3/5] Cleaning .git directories..."
Remove-ItemsByPattern -Name ".git" -Type "dir"

Write-Host "[4/5] Cleaning .gitignore files..."
Remove-ItemsByPattern -Name ".gitignore" -Type "file"

Write-Host "[5/5] Cleaning Cargo.lock files..."
Remove-ItemsByPattern -Name "Cargo.lock" -Type "file"

Write-Host ""
Write-Host "========================================"
Write-Host "  Done!"
Write-Host "========================================"
