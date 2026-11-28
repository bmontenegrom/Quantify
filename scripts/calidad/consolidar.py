import csv
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "scripts"))
from comments import is_doctest_fence

HISTORIAL = ROOT / "docs" / "calidad" / "historial.csv"
MOTOR = ("src/uncertainty.rs", "src/computation/")
CICLO, COGN = 10, 15


def leer(path):
    return path.read_text(encoding="utf-8", errors="replace") if path.exists() else None


def leer_json(path):
    texto = leer(path)
    if not texto:
        return None
    try:
        return json.loads(texto)
    except json.JSONDecodeError:
        return None


def tamano(c):
    t = leer_json(c / "tokei.json")
    if not t:
        return None
    return {lang: t[lang]["code"] for lang in ("Rust", "JavaScript", "CSS", "HTML") if lang in t}


def tests(c):
    out = {}
    log = leer(c / "cargo-test.log")
    if log:
        partes = re.split(r"^\s*Doc-tests ", log, maxsplit=1, flags=re.M)
        resultados = lambda s: re.findall(r"test result: \w+\. (\d+) passed; (\d+) failed", s or "")
        unit = resultados(partes[0])
        doc = resultados(partes[1] if len(partes) > 1 else "")
        out["rust"] = sum(int(p) + int(f) for p, f in unit)
        out["rust_fallidos"] = sum(int(f) for _, f in unit)
        out["doctests"] = sum(int(p) + int(f) for p, f in doc)
        out["doctests_fallidos"] = sum(int(f) for _, f in doc)
    node = leer(c / "node-test.txt")
    if node:
        out["node"] = int(re.search(r"ℹ tests (\d+)", node).group(1))
        out["node_fallidos"] = int(re.search(r"ℹ fail (\d+)", node).group(1))
    e2e = leer(c / "e2e.log")
    if e2e:
        out["e2e_ok"] = len(re.findall(r"^✓ .*: OK", e2e, re.M))
        out["e2e_fallidos"] = len(re.findall(r"^✗ ", e2e, re.M))
    return out or None


def clippy(c, nombre):
    texto = leer(c / nombre)
    if texto is None:
        return None
    lints = Counter()
    vistos = set()
    for linea in texto.splitlines():
        m = json.loads(linea)["message"]
        if m["level"] != "warning" or not m.get("code"):
            continue
        span = next((s for s in m["spans"] if s["is_primary"]), {})
        clave = (m["code"]["code"], span.get("file_name"), span.get("line_start"), span.get("column_start"))
        if clave not in vistos:
            vistos.add(clave)
            lints[m["code"]["code"]] += 1
    return {"total": sum(lints.values()), "por_lint": dict(lints.most_common())}


def funciones(espacio):
    for hijo in espacio.get("spaces", []):
        if hijo["kind"] == "function":
            anidadas = [h for h in hijo["spaces"] if h["kind"] == "function"]
            propia = lambda m: int(hijo["metrics"][m]["sum"] - sum(h["metrics"][m]["sum"] for h in anidadas))
            yield hijo["name"], hijo["start_line"], propia("cyclomatic"), propia("cognitive")
        yield from funciones(hijo)


def complejidad(c):
    rca = c / "rca"
    if not rca.exists():
        return None
    out = {}
    for lang, ext in (("rust", ".rs"), ("js", ".js")):
        fs = []
        for f in rca.rglob(f"*{ext}.json"):
            datos = leer_json(f)
            if datos:
                fs += [(datos["name"], *fn) for fn in funciones(datos)]
        if not fs:
            continue
        out[lang] = {
            "funciones": len(fs),
            "ciclo_gt10": sum(f[3] > CICLO for f in fs),
            "cogn_gt15": sum(f[4] > COGN for f in fs),
            "ciclo_max": max(f[3] for f in fs),
            "cogn_max": max(f[4] for f in fs),
            "peores": [{"archivo": Path(a).as_posix(), "funcion": n, "linea": l, "ciclomatica": cy, "cognitiva": co}
                       for a, n, l, cy, co in sorted(fs, key=lambda f: -f[4])[:10]],
        }
    return out or None


def cobertura(c):
    out = {}
    rust = leer_json(c / "rust-cov.json")
    if rust:
        t = rust["data"][0]["totals"]
        out["rust"] = {k: round(t[k]["percent"], 1) for k in ("lines", "regions", "functions")}
    node = leer(c / "node-test.txt")
    if node and "all files" in node:
        lineas, ramas, funcs = re.search(r"all files\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)", node).groups()
        cargados = len(re.findall(r"ℹ\s+\S+\.js\s*\|", node))
        out["js"] = {"lines": float(lineas), "branches": float(ramas), "functions": float(funcs), "archivos_cargados": cargados}
    return out or None


def mutantes(c):
    m = leer_json(c / "mutants.out" / "outcomes.json")
    if not m:
        return None
    out = {k: m.get(k, 0) for k in ("caught", "missed", "timeout", "unviable")}
    lista = leer(c / "mutants-list.txt")
    out["total"] = len(lista.splitlines()) if lista else None
    evaluados = out["caught"] + out["missed"]
    out["sin_evaluar"] = out["total"] - sum(out[k] for k in ("caught", "missed", "timeout", "unviable")) if lista else None
    out["score"] = round(100 * out["caught"] / evaluados, 1) if evaluados else None
    return out


def deps(c):
    out = {}
    machete = leer(c / "machete.txt")
    if machete is not None:
        out["rust_sin_uso"] = re.findall(r"^\t(\S+)", machete, re.M)
    knip = leer_json(c / "knip.json")
    if knip:
        tipos = Counter()
        for issue in knip.get("issues", []):
            for k, v in issue.items():
                if isinstance(v, (list, dict)) and v:
                    tipos[k] += len(v)
        out["knip"] = dict(tipos)
    madge = leer_json(c / "madge.json")
    if madge is not None:
        out["ciclos_js"] = len(madge)
    return out or None


def pub_sin_doctest(sha):
    archivos = subprocess.run(["git", "ls-tree", "-r", "--name-only", sha, "src"], capture_output=True,
                              encoding="utf-8", cwd=ROOT).stdout.split()
    faltan = []
    for a in archivos:
        if not a.endswith(".rs") or not a.startswith(MOTOR) or a.endswith("tests.rs"):
            continue
        lineas = subprocess.run(["git", "show", f"{sha}:{a}"], capture_output=True, encoding="utf-8",
                                cwd=ROOT).stdout.splitlines()
        for i, linea in enumerate(lineas):
            if linea.strip() == "#[cfg(test)]":
                break
            m = re.match(r"\s*pub fn (\w+)", linea)
            if not m:
                continue
            j = i - 1
            while j >= 0 and lineas[j].strip().startswith("#["):
                j -= 1
            doc = []
            while j >= 0 and lineas[j].strip().startswith("///"):
                doc.append(lineas[j].strip()[3:])
                j -= 1
            if not any(is_doctest_fence(t) for t in doc):
                faltan.append(f"{a}:{i + 1} {m.group(1)}")
    return faltan


def politica(c, sha):
    out = {"pub_sin_doctest": pub_sin_doctest(sha)}
    texto = leer(c / "comments.txt")
    if texto is not None:
        out["archivos_con_comentarios"] = len(re.findall(r"^  \S", texto, re.M))
    return out


def sarif(path):
    s = leer_json(path)
    if not s:
        return None
    resultados = [r for run in s["runs"] for r in run.get("results", [])]
    return {"total": len(resultados), "por_regla": dict(Counter(r["ruleId"] for r in resultados).most_common())}


def seguridad(c):
    out = {}
    ca = leer_json(c / "cargo-audit.json")
    if ca:
        out["cargo_audit"] = {"vulnerabilidades": ca["vulnerabilities"]["count"],
                              "ids": [v["advisory"]["id"] for v in ca["vulnerabilities"]["list"]],
                              "avisos": {k: len(v) for k, v in ca.get("warnings", {}).items()}}
    npm = leer_json(c / "npm-audit.json")
    if npm:
        out["npm_audit"] = npm.get("metadata", {}).get("vulnerabilities")
    sg = leer_json(c / "semgrep.json")
    if sg:
        out["semgrep"] = {"total": len(sg["results"]),
                          "por_regla": dict(Counter(r["check_id"] for r in sg["results"]).most_common())}
    for lang in ("rust", "javascript"):
        r = sarif(c / f"codeql-{lang}.sarif")
        if r:
            out[f"codeql_{lang}"] = r
    return out or None


def ux(c):
    lh = leer_json(c / "lighthouse.json")
    axe = leer_json(c / "axe.json")
    if not lh and not axe:
        return None
    out = {}
    if lh:
        out["lighthouse"] = {p["page"]: p["median"] for p in lh}
    if axe:
        out["axe"] = dict(Counter(v["impact"] for vs in axe.values() for v in vs))
        out["axe_por_pagina"] = {p: [v["id"] for v in vs] for p, vs in axe.items()}
    return out


def sonar(c):
    s = leer_json(c / "sonar.json")
    if not s or "component" not in s:
        return None
    out = {m["metric"]: m.get("value") for m in s["component"]["measures"]}
    issues = leer_json(c / "sonar-issues.json")
    if issues:
        out["issues"] = {f["property"]: {v["val"]: v["count"] for v in f["values"]} for f in issues.get("facets", [])}
    return out


def fila(r):
    g = lambda *ks: _get(r, ks)
    lh = (r.get("ux") or {}).get("lighthouse") or {}
    axe = (r.get("ux") or {}).get("axe") or {}
    return {
        "fecha": r["fecha"], "commit": r["commit"][:7], "metodo": r["metodo"],
        "loc_rust": g("tamano", "Rust"), "loc_js": g("tamano", "JavaScript"),
        "loc_css": g("tamano", "CSS"), "loc_html": g("tamano", "HTML"),
        "tests_rust": g("tests", "rust"), "tests_doctests": g("tests", "doctests"),
        "tests_node": g("tests", "node"), "tests_e2e": g("tests", "e2e_ok"),
        "rust_ciclo_gt10": g("complejidad", "rust", "ciclo_gt10"), "rust_cogn_gt15": g("complejidad", "rust", "cogn_gt15"),
        "js_ciclo_gt10": g("complejidad", "js", "ciclo_gt10"), "js_cogn_gt15": g("complejidad", "js", "cogn_gt15"),
        "clippy": g("clippy", "defecto", "total"), "clippy_pedantic": g("clippy", "pedantic", "total"),
        "cov_rust_lineas": g("cobertura", "rust", "lines"), "cov_rust_regiones": g("cobertura", "rust", "regions"),
        "cov_js_lineas": g("cobertura", "js", "lines"), "cov_js_ramas": g("cobertura", "js", "branches"),
        "mutation_score": g("mutantes", "score"), "mutantes_sin_evaluar": g("mutantes", "sin_evaluar"),
        "deps_rust_sin_uso": _len(g("deps", "rust_sin_uso")), "ciclos_js": g("deps", "ciclos_js"),
        "comentarios_fuera_politica": g("politica", "archivos_con_comentarios"),
        "pub_sin_doctest": _len(g("politica", "pub_sin_doctest")),
        "vuln_rust": g("seguridad", "cargo_audit", "vulnerabilidades"),
        "vuln_npm": g("seguridad", "npm_audit", "total"),
        "semgrep": g("seguridad", "semgrep", "total"),
        "codeql": _sum(g("seguridad", "codeql_rust", "total"), g("seguridad", "codeql_javascript", "total")),
        "lh_perf_min": min((p.get("performance") for p in lh.values()), default=None),
        "lh_a11y_min": min((p.get("accessibility") for p in lh.values()), default=None),
        "axe_criticas_serias": _sum(axe.get("critical"), axe.get("serious")) if axe else None,
        "sonar_coverage": g("sonar", "coverage"), "sonar_duplicacion": g("sonar", "duplicated_lines_density"),
        "sonar_code_smells": g("sonar", "code_smells"), "sonar_bugs": g("sonar", "bugs"),
        "sonar_vulnerabilidades": g("sonar", "vulnerabilities"), "sonar_hotspots": g("sonar", "security_hotspots"),
        "sonar_deuda_ratio": g("sonar", "sqale_debt_ratio"), "sonar_mantenibilidad": g("sonar", "sqale_rating"),
        "sonar_fiabilidad": g("sonar", "reliability_rating"), "sonar_seguridad": g("sonar", "security_rating"),
        "prs": g("proceso", "prs_mergeadas"), "horas_merge": g("proceso", "horas_merge_mediana"),
        "ci_verde_pct": g("proceso", "ci_verde_pct"),
    }


def _get(d, ks):
    for k in ks:
        if not isinstance(d, dict) or d.get(k) is None:
            return None
        d = d[k]
    return d


def _len(x):
    return None if x is None else len(x)


def _sum(*xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) if xs else None


def main():
    dir_ = Path(sys.argv[1]).resolve()
    c = dir_ / "crudo"
    metodo, sha, fecha, asunto, sistema = (leer(dir_ / "commit.txt").splitlines() + [""] * 5)[:5]
    r = {
        "metodo": metodo, "commit": sha, "fecha": fecha[:10], "asunto": asunto, "sistema": sistema,
        "tamano": tamano(c), "tests": tests(c), "complejidad": complejidad(c),
        "clippy": {"defecto": clippy(c, "clippy.json"), "pedantic": clippy(c, "clippy-pedantic.json")},
        "cobertura": cobertura(c), "mutantes": mutantes(c), "deps": deps(c), "politica": politica(c, sha),
        "seguridad": seguridad(c), "ux": ux(c), "sonar": sonar(c), "proceso": leer_json(dir_ / "proceso.json"),
    }
    (dir_ / "resumen.json").write_text(json.dumps(r, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    nueva = fila(r)
    filas = []
    if HISTORIAL.exists():
        with open(HISTORIAL, encoding="utf-8") as f:
            filas = [x for x in csv.DictReader(f) if x["commit"] != nueva["commit"]]
    filas = sorted(filas + [nueva], key=lambda x: x["fecha"])
    with open(HISTORIAL, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=list(nueva))
        w.writeheader()
        w.writerows(filas)
    print(json.dumps(nueva, indent=1, ensure_ascii=False))


main()
