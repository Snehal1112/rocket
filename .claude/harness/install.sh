#!/usr/bin/env bash
# Install the harness into a project. Idempotent. Usage: install.sh [--root <dir>]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="${HARNESS_ROOT:-$(cd "$here/../.." && pwd)}"
while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

PYTHONDONTWRITEBYTECODE=1 python3 "$here/_manage.py" install "$root"
