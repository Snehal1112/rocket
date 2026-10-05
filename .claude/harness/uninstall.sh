#!/usr/bin/env bash
# Remove the harness settings entries. Usage: uninstall.sh [--purge] [--root <dir>]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="${HARNESS_ROOT:-$(cd "$here/../.." && pwd)}"
purge=""
while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="$2"; shift 2 ;;
    --purge) purge="--purge"; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

# Python loads fully before purge deletes its own files.
PYTHONDONTWRITEBYTECODE=1 python3 "$here/_manage.py" uninstall "$root" $purge
