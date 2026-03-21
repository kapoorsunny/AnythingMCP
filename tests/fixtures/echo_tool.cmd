@echo off
REM Echo a message to stdout
if "%1"=="--help" (
    echo usage: echo_tool.cmd [--message MESSAGE] [--repeat REPEAT] [--uppercase]
    echo.
    echo Echo a message to stdout
    echo.
    echo options:
    echo   --help             show this help message and exit
    echo   --message MESSAGE  Message to echo [required]
    echo   --repeat REPEAT    Number of times to repeat [default: 1]
    echo   --uppercase        Convert to uppercase
    exit /b 0
)

set MESSAGE=
set REPEAT=1
set UPPERCASE=0

:parse
if "%1"=="" goto :run
if "%1"=="--message" (
    set MESSAGE=%~2
    shift
    shift
    goto :parse
)
if "%1"=="--repeat" (
    set REPEAT=%~2
    shift
    shift
    goto :parse
)
if "%1"=="--uppercase" (
    set UPPERCASE=1
    shift
    goto :parse
)
shift
goto :parse

:run
if "%UPPERCASE%"=="1" (
    for /L %%i in (1,1,%REPEAT%) do echo %MESSAGE%
) else (
    for /L %%i in (1,1,%REPEAT%) do echo %MESSAGE%
)
