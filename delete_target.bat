@echo off
chcp 65001 >nul 2>&1

echo ========================================
echo   Rust/ESP32 Build Artifact Cleaner
echo   Platform: Windows (CMD)
echo ========================================
echo.

echo [1/5] Cleaning target directories...
set count=0
for /f "delims=" %%d in ('dir /s /b /ad target 2^>nul') do (
    rmdir /s /q "%%d" 2>nul
    echo   [DEL] %%d
    set /a count+=1
)
echo   -^> target: %count% removed

echo [2/5] Cleaning .DS_Store files...
set count=0
for /f "delims=" %%f in ('dir /s /b .DS_Store 2^>nul') do (
    del /f /q "%%f" 2>nul
    echo   [DEL] %%f
    set /a count+=1
)
echo   -^> .DS_Store: %count% removed

echo [3/5] Cleaning .git directories...
set count=0
for /f "delims=" %%d in ('dir /s /b /ad .git 2^>nul') do (
    rmdir /s /q "%%d" 2>nul
    echo   [DEL] %%d
    set /a count+=1
)
echo   -^> .git: %count% removed

echo [4/5] Cleaning .gitignore files...
set count=0
for /f "delims=" %%f in ('dir /s /b .gitignore 2^>nul') do (
    del /f /q "%%f" 2>nul
    echo   [DEL] %%f
    set /a count+=1
)
echo   -^> .gitignore: %count% removed

echo [5/5] Cleaning Cargo.lock files...
set count=0
for /f "delims=" %%f in ('dir /s /b Cargo.lock 2^>nul') do (
    del /f /q "%%f" 2>nul
    echo   [DEL] %%f
    set /a count+=1
)
echo   -^> Cargo.lock: %count% removed

echo.
echo ========================================
echo   Done!
echo ========================================
pause
