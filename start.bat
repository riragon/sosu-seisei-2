@echo off

REM Change directory to this script location
cd /d "%~dp0"

REM ============================================================
REM CMake 自動検出
REM ============================================================

set "CMAKE="

REM ---- 1. ローカル cmake フォルダ (ポータブル版) ----
if exist "%~dp0cmake\bin\cmake.exe" (
    set "CMAKE=%~dp0cmake\bin\cmake.exe"
    goto :found
)

REM ---- 2. Visual Studio 2022 BuildTools (64-bit) ----
if exist "%ProgramFiles%\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe" (
    set "CMAKE=%ProgramFiles%\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
    goto :found
)

REM ---- 3. Standalone CMake (64-bit) ----
if exist "%ProgramFiles%\CMake\bin\cmake.exe" (
    set "CMAKE=%ProgramFiles%\CMake\bin\cmake.exe"
    goto :found
)

REM ---- 4. PATH 上の cmake.exe ----
where cmake >NUL 2>&1
if %ERRORLEVEL% equ 0 (
    for /f "delims=" %%I in ('where cmake') do (
        set "CMAKE=%%I"
        goto :found
    )
)

REM ---- 見つからなかった場合 ----
echo.
echo ============================================================
echo [ERROR] CMake (cmake.exe) was not found.
echo ============================================================
echo.
echo Please download and install CMake for Windows (x64) from:
echo   https://github.com/Kitware/CMake/releases/download/v3.31.10/cmake-3.31.10-windows-x86_64.msi
echo.
echo During setup, please CHECK the option:
echo   "Add CMake to the system PATH for all users"
echo.
echo After installation, run start.bat again.
echo ============================================================
echo.
pause
exit /b 1

:found
echo Using CMake at: "%CMAKE%"
echo.
echo Building sosu-seisei-main2 in release mode...
cargo build --release

echo.
echo Build finished. Please check the log above.
pause
