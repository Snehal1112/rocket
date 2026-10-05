"""Install and uninstall logic for the harness. Called by install.sh and uninstall.sh."""
import json
import os
import re
import shutil
import sys



def hook_cmd(name):
    """Resolve the hook per checkout and do nothing when the file is absent."""
    return (
        'h="$(git rev-parse --show-toplevel 2>/dev/null)/.claude/harness/hooks/%s"; '
        'if [ -f "$h" ]; then bash "$h"; fi' % name
    )


GUARD_CMD = hook_cmd("bash-guard")
GATE_CMD = hook_cmd("commit-gate")
HOOK_MARK = "/.claude/harness/hooks/"
DENY = [
    "Bash(cargo test --workspace:*)",
    "Bash(git push --force:*)",
    "Bash(git push -f:*)",
]
BEGIN = "<!-- harness:begin -->\n"
END = "<!-- harness:end -->\n"
LINK = "- [harness.md](harness.md)\n"
BLOCK = BEGIN + LINK + END


def paths(root):
    c = os.path.join(root, ".claude")
    return {
        "settings": os.path.join(c, "settings.json"),
        "shortcuts": os.path.join(c, "rules", "00-shortcuts.md"),
        "harness": os.path.join(c, "harness"),
        "manifest": os.path.join(c, "harness", "manifest.json"),
        "backup": os.path.join(c, "harness", "backup", "settings.json.pre-harness"),
        "skill": os.path.join(c, "skills", "resume-plan"),
        "rule": os.path.join(c, "rules", "harness.md"),
    }


def load_settings(p):
    with open(p["settings"], encoding="utf-8") as f:
        raw = f.read()
    return json.loads(raw), raw.endswith("\n")


def save_settings(p, data, newline):
    text = json.dumps(data, indent="\t", ensure_ascii=False)
    if newline:
        text += "\n"
    with open(p["settings"], "w", encoding="utf-8") as f:
        f.write(text)


def is_harness_hook(h):
    return HOOK_MARK in (h.get("command") or "")


def install(root):
    p = paths(root)
    if not os.path.exists(p["settings"]):
        print("No settings.json at %s, nothing to install into." % p["settings"])
        return 1
    man = {"hooks": [], "deny": [], "created": [], "shortcuts": False}
    if os.path.exists(p["manifest"]):
        with open(p["manifest"], encoding="utf-8") as f:
            man.update(json.load(f))
    if not os.path.exists(p["backup"]):
        os.makedirs(os.path.dirname(p["backup"]), exist_ok=True)
        shutil.copyfile(p["settings"], p["backup"])
    data, newline = load_settings(p)

    def created(key):
        if key not in man["created"]:
            man["created"].append(key)

    if "hooks" not in data:
        data["hooks"] = {}
        created("hooks")
    if "PreToolUse" not in data["hooks"]:
        data["hooks"]["PreToolUse"] = []
        created("hooks.PreToolUse")
    all_hooks = [h for e in data["hooks"]["PreToolUse"] for h in e.get("hooks", [])]
    for name, cmd, extra in (
        ("bash-guard", GUARD_CMD, {}),
        ("commit-gate", GATE_CMD, {"timeout": 600}),
    ):
        found = [h for h in all_hooks if HOOK_MARK + name in (h.get("command") or "")]
        if found:
            # Migrate old-form commands in place so nothing is duplicated.
            for h in found:
                old = h["command"]
                if old != cmd:
                    h["command"] = cmd
                    man["hooks"] = [cmd if m == old else m for m in man["hooks"]]
            continue
        hook = {"type": "command", "command": cmd}
        hook.update(extra)
        data["hooks"]["PreToolUse"].append({"matcher": "Bash", "hooks": [hook]})
        man["hooks"].append(cmd)
    if "permissions" not in data:
        data["permissions"] = {}
        created("permissions")
    if "deny" not in data["permissions"]:
        data["permissions"]["deny"] = []
        created("permissions.deny")
    for d in DENY:
        if d not in data["permissions"]["deny"]:
            data["permissions"]["deny"].append(d)
            man["deny"].append(d)
    save_settings(p, data, newline)

    if os.path.exists(p["shortcuts"]):
        with open(p["shortcuts"], encoding="utf-8") as f:
            text = f.read()
        if BEGIN not in text:
            lines = text.splitlines(keepends=True)
            idx = max((i for i, l in enumerate(lines) if l.startswith("- [")), default=-1)
            if idx >= 0 and not lines[idx].endswith("\n"):
                lines[idx] += "\n"
            lines.insert(idx + 1 if idx >= 0 else len(lines), BLOCK)
            with open(p["shortcuts"], "w", encoding="utf-8") as f:
                f.write("".join(lines))
            man["shortcuts"] = True
    with open(p["manifest"], "w", encoding="utf-8") as f:
        json.dump(man, f, indent="\t")
        f.write("\n")
    print("Installed harness. Added hooks: %d, deny entries: %d." % (len(man["hooks"]), len(man["deny"])))
    return 0


def uninstall(root, purge):
    p = paths(root)
    man = None
    if os.path.exists(p["manifest"]):
        with open(p["manifest"], encoding="utf-8") as f:
            man = json.load(f)
    removed = []
    if os.path.exists(p["settings"]):
        data, newline = load_settings(p)
        hooks = data.get("hooks", {})
        pre = hooks.get("PreToolUse")
        if pre is not None:
            keep = []
            for e in pre:
                inner = e.get("hooks", [])
                kept = [h for h in inner if not is_harness_hook(h)]
                for h in inner:
                    if is_harness_hook(h):
                        removed.append("hook: " + h["command"])
                if kept or not inner:
                    e["hooks"] = kept if inner else inner
                    keep.append(e)
            hooks["PreToolUse"] = keep
            if not keep and (man is None or "hooks.PreToolUse" in man["created"]):
                del hooks["PreToolUse"]
            if not hooks and (man is None or "hooks" in man["created"]):
                del data["hooks"]
        perms = data.get("permissions")
        if perms is not None and "deny" in perms:
            for d in DENY:
                if d in perms["deny"] and (man is None or d in man["deny"]):
                    perms["deny"].remove(d)
                    removed.append("deny: " + d)
            if not perms["deny"] and (man is None or "permissions.deny" in man["created"]):
                del perms["deny"]
            if not perms and (man is None or "permissions" in man["created"]):
                del data["permissions"]
        save_settings(p, data, newline)
    if os.path.exists(p["shortcuts"]):
        with open(p["shortcuts"], encoding="utf-8") as f:
            text = f.read()
        new = re.sub(re.escape(BEGIN) + r".*?" + re.escape(END), "", text, flags=re.S)
        if new != text:
            with open(p["shortcuts"], "w", encoding="utf-8") as f:
                f.write(new)
            removed.append("block: harness link in 00-shortcuts.md")
    if os.path.exists(p["manifest"]):
        os.remove(p["manifest"])
        removed.append("file: manifest.json")
    if purge:
        for key in ("harness", "skill"):
            if os.path.isdir(p[key]):
                shutil.rmtree(p[key])
                removed.append("dir: " + p[key])
        if os.path.exists(p["rule"]):
            os.remove(p["rule"])
            removed.append("file: " + p["rule"])
        skills = os.path.dirname(p["skill"])
        if os.path.isdir(skills) and not os.listdir(skills):
            os.rmdir(skills)
    if removed:
        print("Removed:")
        for r in removed:
            print("  " + r)
    else:
        print("Nothing to remove.")
    return 0


if __name__ == "__main__":
    mode, root = sys.argv[1], sys.argv[2]
    if mode == "install":
        sys.exit(install(root))
    sys.exit(uninstall(root, "--purge" in sys.argv[3:]))
