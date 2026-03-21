@echo off
REM Prints help to stderr only
if "%1"=="--help" (
    echo usage: help_on_stderr_tool.cmd [--output FILE] [--verbose] 1>&2
    echo. 1>&2
    echo A tool that prints help to stderr 1>&2
    echo. 1>&2
    echo options: 1>&2
    echo   --output FILE  Output file path 1>&2
    echo   --verbose      Enable verbose output 1>&2
    exit /b 0
)
if "%1"=="-h" (
    echo usage: help_on_stderr_tool.cmd [--output FILE] [--verbose] 1>&2
    echo. 1>&2
    echo A tool that prints help to stderr 1>&2
    echo. 1>&2
    echo options: 1>&2
    echo   --output FILE  Output file path 1>&2
    echo   --verbose      Enable verbose output 1>&2
    exit /b 0
)
echo running 1>&2
exit /b 0
