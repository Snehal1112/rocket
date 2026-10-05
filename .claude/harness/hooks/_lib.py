"""Shared helpers for the harness hooks."""
import json
import os
import shlex
import sys
import time

WRAPPERS = {"env", "sudo", "time", "command", "nice", "nohup", "exec"}


def split_segments(cmd):
    """Split on unquoted separators and skip heredoc bodies."""
    segs, cur = [], []
    quote = None
    i, n = 0, len(cmd)
    heredocs = []
    while i < n:
        c = cmd[i]
        if quote:
            cur.append(c)
            if c == "\\" and quote == '"' and i + 1 < n:
                cur.append(cmd[i + 1])
                i += 1
            elif c == quote:
                quote = None
            i += 1
            continue
        if c == "\\" and i + 1 < n:
            cur.append(c)
            cur.append(cmd[i + 1])
            i += 2
            continue
        if c in "'\"":
            quote = c
            cur.append(c)
            i += 1
            continue
        if cmd.startswith("<<", i) and not cmd.startswith("<<<", i):
            j = i + 2
            if j < n and cmd[j] == "-":
                j += 1
            while j < n and cmd[j] in " \t":
                j += 1
            k = j
            while k < n and (cmd[k].isalnum() or cmd[k] in "_'\""):
                k += 1
            word = cmd[j:k].strip("'\"")
            if word:
                heredocs.append(word)
            cur.append(cmd[i:k])
            i = k
            continue
        if c == "\n":
            segs.append("".join(cur))
            cur = []
            i += 1
            # Skip heredoc bodies until each delimiter line is seen.
            while heredocs and i < n:
                end = cmd.find("\n", i)
                line = cmd[i:] if end < 0 else cmd[i:end]
                i = n if end < 0 else end + 1
                if line.strip() == heredocs[0]:
                    heredocs.pop(0)
            continue
        if cmd.startswith("&&", i) or cmd.startswith("||", i):
            segs.append("".join(cur))
            cur = []
            i += 2
            continue
        if c in ";|&":
            segs.append("".join(cur))
            cur = []
            i += 1
            continue
        cur.append(c)
        i += 1
    segs.append("".join(cur))
    return segs


def tokens_of(seg):
    """Tokenize one segment and drop env assignments and wrappers."""
    try:
        toks = shlex.split(seg)
    except ValueError:
        return []
    while toks and (
        toks[0] in WRAPPERS
        or ("=" in toks[0] and toks[0].split("=")[0].isidentifier())
    ):
        toks = toks[1:]
    return toks


def git_args(toks):
    """Return the git subcommand and its args, skipping global options."""
    rest = toks[1:]
    while rest:
        if rest[0] in ("-C", "-c", "--git-dir", "--work-tree"):
            rest = rest[2:]
        elif rest[0].startswith("-"):
            rest = rest[1:]
        else:
            break
    if not rest:
        return None, []
    return rest[0], rest[1:]


def read_payload():
    """Return (command, cwd) for a Bash payload, or None to allow."""
    try:
        data = json.load(sys.stdin)
        if data.get("tool_name") != "Bash":
            return None
        cmd = (data.get("tool_input") or {}).get("command") or ""
        if not isinstance(cmd, str):
            return None
        return cmd, data.get("cwd") or os.getcwd()
    except Exception:
        return None


def log_deny(rule, cmd):
    """Append one line to the guard log. Never raises."""
    try:
        d = os.environ.get("HARNESS_LOG_DIR", "")
        if not d:
            return
        os.makedirs(d, exist_ok=True)
        first = cmd.replace("\n", " ").replace("\t", " ")[:80]
        stamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")
        with open(os.path.join(d, "guard.log"), "a") as f:
            f.write("%s\t%s\t%s\n" % (stamp, rule, first))
    except OSError:
        pass


def deny(rule, cmd, reason):
    """Log, print the deny JSON and exit 2."""
    log_deny(rule, cmd)
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": reason,
    }}, indent=2))
    sys.exit(2)
