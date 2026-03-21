#!/usr/bin/env python3
"""A complex tool with many flags for testing."""

import argparse
import sys

def main():
    parser = argparse.ArgumentParser(
        description="A complex data processing tool"
    )
    parser.add_argument("--input", type=str, required=True, help="Input file path [required]")
    parser.add_argument("--output", type=str, help="Output file path [default: stdout]")
    parser.add_argument("--format", type=str, default="json", help="Output format [default: json]")
    parser.add_argument("--threads", type=int, default=4, help="Number of threads [default: 4]")
    parser.add_argument("--rate", type=float, default=1.0, help="Processing rate [default: 1.0]")
    parser.add_argument("--dry-run", action="store_true", help="Perform a dry run")
    parser.add_argument("-v", "--verbose", action="store_true", help="Enable verbose output")

    args = parser.parse_args()
    print(f"Processing {args.input}")

if __name__ == "__main__":
    main()
