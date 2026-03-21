#!/usr/bin/env python3
"""A simple echo tool for testing."""

import argparse
import sys

def main():
    parser = argparse.ArgumentParser(description="Echo a message to stdout")
    parser.add_argument("--message", type=str, required=True, help="Message to echo [required]")
    parser.add_argument("--repeat", type=int, default=1, help="Number of times to repeat [default: 1]")
    parser.add_argument("--uppercase", action="store_true", help="Convert to uppercase")

    args = parser.parse_args()

    msg = args.message
    if args.uppercase:
        msg = msg.upper()

    for _ in range(args.repeat):
        print(msg)

if __name__ == "__main__":
    main()
