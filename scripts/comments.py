import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXCLUDED = ("graphify-out/", "tests/golden/", "data/")


def rust_spans(src):
    spans = []
    i, n = 0, len(src)
    ident = re.compile(r"[A-Za-z0-9_]")
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j == -1 else j
            spans.append((i, j))
            i = j
        elif src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            spans.append((i, j))
            i = j
        elif c in "rbc" and (i == 0 or not ident.match(src[i - 1])):
            m = re.match(r"(?:br|cr|r)(#*)\"", src[i:])
            if m:
                close = '"' + m.group(1)
                j = src.find(close, i + m.end())
                i = n if j == -1 else j + len(close)
            elif src.startswith('b"', i) or src.startswith('c"', i):
                i = skip_quoted(src, i + 1, '"')
            elif src.startswith("b'", i):
                i = skip_quoted(src, i + 1, "'")
            else:
                i += 1
        elif c == '"':
            i = skip_quoted(src, i, '"')
        elif c == "'":
            if i + 1 < n and src[i + 1] == "\\":
                i = skip_quoted(src, i, "'")
            elif i + 2 < n and src[i + 2] == "'":
                i += 3
            else:
                i += 1
        else:
            i += 1
    return spans


def skip_quoted(src, i, quote):
    j = i + 1
    while j < len(src):
        if src[j] == "\\":
            j += 2
        elif src[j] == quote:
            return j + 1
        else:
            j += 1
    return j


REGEX_PREV = set("(,=:[!&|?{};+-*%<>~^")
REGEX_KEYWORDS = {"return", "typeof", "case", "do", "else", "in", "of", "new", "delete", "void", "throw", "yield", "await"}


def js_spans(src):
    spans = []
    i, n = 0, len(src)
    prev = ""
    stack = []
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j == -1 else j
            spans.append((i, j))
            i = j
        elif src.startswith("/*", i):
            j = src.find("*/", i + 2)
            j = n if j == -1 else j + 2
            spans.append((i, j))
            i = j
        elif c in "\"'":
            i = skip_quoted(src, i, c)
            prev = "a"
        elif c == "`":
            i = skip_template(src, i + 1, stack)
            prev = "a"
        elif c == "}" and stack and stack[-1] == 0:
            stack.pop()
            i = skip_template(src, i + 1, stack)
            prev = "a"
        elif c == "/" and (prev == "" or prev in REGEX_PREV or prev in REGEX_KEYWORDS):
            j, in_class = i + 1, False
            while j < n and src[j] != "\n":
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == "[":
                    in_class = True
                elif src[j] == "]":
                    in_class = False
                elif src[j] == "/" and not in_class:
                    break
                j += 1
            j += 1
            while j < n and src[j].isalpha():
                j += 1
            i = j
            prev = "a"
        elif c.isalnum() or c in "_$":
            m = re.match(r"[A-Za-z0-9_$]+", src[i:])
            word = m.group(0)
            prev = word if word in REGEX_KEYWORDS else "a"
            i += len(word)
        elif c.isspace():
            i += 1
        else:
            if stack:
                if c == "{":
                    stack[-1] += 1
                elif c == "}":
                    stack[-1] -= 1
            prev = c if c not in ")]" else "a"
            i += 1
    return spans


def skip_template(src, i, stack):
    while i < len(src):
        if src[i] == "\\":
            i += 2
        elif src[i] == "`":
            return i + 1
        elif src.startswith("${", i):
            stack.append(0)
            return i + 2
        else:
            i += 1
    return i


def css_spans(src):
    spans, i, n = [], 0, len(src)
    while i < n:
        if src[i] in "\"'":
            i = skip_quoted(src, i, src[i])
        elif src.startswith("/*", i):
            j = src.find("*/", i + 2)
            j = n if j == -1 else j + 2
            spans.append((i, j))
            i = j
        else:
            i += 1
    return spans


def html_spans(src):
    return [(m.start(), m.end()) for m in re.finditer(r"<!--.*?-->", src, re.S)]


def hash_spans(src):
    spans, offset = [], 0
    for number, line in enumerate(src.split("\n")):
        if not (number == 0 and line.startswith("#!")):
            quote = None
            for k, ch in enumerate(line):
                if quote:
                    if ch == quote:
                        quote = None
                elif ch in "\"'":
                    quote = ch
                elif ch == "#" and (k == 0 or line[k - 1].isspace()):
                    spans.append((offset + k, offset + len(line)))
                    break
        offset += len(line) + 1
    return spans


def is_doctest_fence(text):
    return text.strip() in ("```", "```rust")


def rust_docs(lines):
    out, i = [], 0
    while i < len(lines):
        if lines[i].lstrip().startswith("///") and not lines[i].lstrip().startswith("////"):
            j = i
            while j < len(lines) and lines[j].lstrip().startswith("///") and not lines[j].lstrip().startswith("////"):
                j += 1
            block = lines[i:j]
            texts = [l.lstrip()[3:] for l in block]
            texts = [t[1:] if t.startswith(" ") else t for t in texts]
            if any(is_doctest_fence(t) for t in texts):
                indent = block[0][: len(block[0]) - len(block[0].lstrip())]
                summary = next(
                    (t.strip() for t in texts if t.strip() and not t.startswith("#") and not t.startswith("```")),
                    None,
                )
                kept = [f"{indent}/// {summary}", f"{indent}///"] if summary else []
                inside = False
                for t in texts:
                    if t.strip().startswith("```"):
                        kept.append(f"{indent}/// {t.strip()}")
                        inside = not inside
                    elif inside:
                        kept.append(f"{indent}/// {t}".rstrip())
                out.extend(kept)
            i = j
        else:
            out.append(lines[i])
            i += 1
    return out


def strip(src, spans):
    pieces, last = [], 0
    for start, end in spans:
        pieces.append(src[last:start])
        pieces.append("\x00" if "\n" not in src[start:end] else "\x00\n" * src[start:end].count("\n"))
        last = end
    pieces.append(src[last:])
    lines = []
    for original, line in zip(src.split("\n"), "".join(pieces).split("\n")):
        if "\x00" in line:
            line = line.replace("\x00", "").rstrip()
            if not line.strip():
                continue
        lines.append(line)
    text, collapsed, blank = "\n".join(lines), [], False
    for line in text.split("\n"):
        if not line.strip():
            if blank:
                continue
            blank = True
        else:
            blank = False
        collapsed.append(line)
    return "\n".join(collapsed)


def process(path, src):
    suffix = path.suffix
    if suffix == ".rs":
        return strip_rust(src)
    if suffix in (".js", ".mjs"):
        return strip(src, js_spans(src))
    if suffix == ".css":
        return strip(src, css_spans(src))
    if suffix == ".html":
        return strip(src, html_spans(src))
    return strip(src, hash_spans(src))


def is_doc_line(line):
    stripped = line.lstrip()
    return stripped.startswith("///") and not stripped.startswith("////")


def strip_rust(src):
    original = src.split("\n")
    marker = "\x01DOC"
    masked_lines = [marker + str(k) if is_doc_line(line) else line for k, line in enumerate(original)]
    spans = rust_spans("\n".join(" " * len(l) if l.startswith(marker) else l for l in masked_lines))
    rebuilt = strip("\n".join(masked_lines), spans)
    out = []
    pending = []
    for line in rebuilt.split("\n"):
        if line.startswith(marker):
            pending.append(original[int(line[len(marker):])])
            continue
        if pending:
            out.extend(rust_docs(pending))
            pending = []
        out.append(line)
    out.extend(rust_docs(pending))
    return "\n".join(out)


def files():
    listed = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    for name in listed.split("\n"):
        if not name or name.startswith(EXCLUDED):
            continue
        path = ROOT / name
        if path.suffix in (".rs", ".js", ".mjs", ".css", ".html", ".yml", ".yaml", ".sh", ".ps1", ".toml") or path.name in (
            "Dockerfile",
            ".gitattributes",
            ".gitignore",
            ".dockerignore",
            ".env.example",
        ):
            yield path


def main():
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--check", action="store_true")
    group.add_argument("--fix", action="store_true")
    mode = "--fix" if parser.parse_args().fix else "--check"
    dirty = []
    for path in files():
        src = path.read_text(encoding="utf-8")
        result = process(path, src)
        if result != src:
            dirty.append(path.relative_to(ROOT).as_posix())
            if mode == "--fix":
                path.write_text(result, encoding="utf-8", newline="")
    if mode == "--check" and dirty:
        print("Comentarios que no son doctests en:\n  " + "\n  ".join(dirty))
        sys.exit(1)
    print(f"{len(dirty)} archivo(s) {'corregidos' if mode == '--fix' else 'con comentarios'}")


if __name__ == "__main__":
    main()
