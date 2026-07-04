#!/bin/bash

echo "========================================"
echo "  Rust/ESP32 Build Artifact Cleaner"
echo "  Platform: Linux / macOS"
echo "========================================"
echo ""

delete_dir() {
    local name="$1"
    local count=0
    while IFS= read -r dir; do
        [ -z "$dir" ] && continue
        rm -rf "$dir"
        echo "  [DEL] $dir"
        count=$((count + 1))
    done < <(find . -type d -name "$name" 2>/dev/null)
    echo "  -> $name: $count removed"
}

delete_file() {
    local name="$1"
    local count=0
    while IFS= read -r file; do
        [ -z "$file" ] && continue
        rm -f "$file"
        echo "  [DEL] $file"
        count=$((count + 1))
    done < <(find . -type f -name "$name" 2>/dev/null)
    echo "  -> $name: $count removed"
}

echo "[1/5] Cleaning target directories..."
delete_dir "target"

echo "[2/5] Cleaning .DS_Store files..."
delete_file ".DS_Store"

echo "[3/5] Cleaning .git directories..."
delete_dir ".git"

echo "[4/5] Cleaning .gitignore files..."
delete_file ".gitignore"

echo "[5/5] Cleaning Cargo.lock files..."
delete_file "Cargo.lock"

echo ""
echo "========================================"
echo "  Done!"
echo "========================================"
