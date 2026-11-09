#!/bin/sh
if [ "$1" = --version ]; then
    echo 'codex-cli 0.160.1'
    exit 0
fi
printf '%s\n' "$FERRITE_VISUAL_TEST_OUTPUT"
exec cat > "$FERRITE_VISUAL_TEST_HOST"
