#!/bin/bash
# Prints help output to stderr only (not stdout)
if [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "A tool that prints help to stderr" >&2
    echo "" >&2
    echo "Options:" >&2
    echo "  --input <FILE>   Input file [required]" >&2
    echo "  --output <FILE>  Output file" >&2
    exit 0
fi
echo "running..." >&2
exit 0
