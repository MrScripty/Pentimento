#!/usr/bin/env bash

set -euo pipefail

message_file="${1:?commit message file is required}"
subject="$(head -n 1 "$message_file")"
pattern='^(feat|fix|refactor|chore|docs|style|test|perf|ci)(\([a-z0-9._-]+\))?!?: .+$'

if [[ ! "$subject" =~ $pattern ]]; then
    echo "Commit subject must follow Conventional Commits: <type>(<scope>): <description>" >&2
    exit 1
fi

if (( ${#subject} > 72 )); then
    echo "Commit subject should stay under 72 characters" >&2
    exit 1
fi
