#!/bin/sh
if [ "$1" = --version ]; then
    echo '2.1.292 (Claude Code)'
    exit 0
fi
if [ -n "$FERRITE_VISUAL_TEST_ARGV" ]; then
    printf '%s\n' "$*" > "$FERRITE_VISUAL_TEST_ARGV"
fi
printf '%s\n' "$FERRITE_VISUAL_TEST_OUTPUT"
exec cat > "$FERRITE_VISUAL_TEST_HOST"
