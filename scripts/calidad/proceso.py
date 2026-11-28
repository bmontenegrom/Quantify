import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from statistics import median

ROOT = Path(__file__).resolve().parents[2]


def run(*args):
    for _ in range(4):
        r = subprocess.run(args, capture_output=True, encoding="utf-8", cwd=ROOT)
        if r.returncode == 0:
            return r.stdout
    r.check_returncode()


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def commit_date(rev):
    return ts(run("git", "show", "-s", "--format=%cI", rev).strip())


def main():
    sha = run("git", "rev-parse", "--short=7", sys.argv[1]).strip()
    hasta = commit_date(sha)
    desde = commit_date(sys.argv[2]) if len(sys.argv) > 2 else None
    out = next((ROOT / "docs" / "calidad" / "datos").glob(f"*-{sha}"))

    def in_window(t):
        return (desde is None or t > desde) and t <= hasta

    prs = json.loads(run("gh", "pr", "list", "--state", "merged", "--base", "development", "--limit", "1000",
                         "--json", "number,createdAt,mergedAt,additions,deletions"))
    prs = [p for p in prs if in_window(ts(p["mergedAt"]))]
    runs = json.loads(run("gh", "run", "list", "--workflow", "ci.yml", "--limit", "1000",
                          "--json", "conclusion,createdAt,status"))
    runs = [r for r in runs if r["status"] == "completed" and r["conclusion"] not in ("cancelled", "skipped")
            and in_window(ts(r["createdAt"]))]
    horas = [(ts(p["mergedAt"]) - ts(p["createdAt"])).total_seconds() / 3600 for p in prs]
    resumen = {
        "desde": desde.isoformat() if desde else None,
        "hasta": hasta.isoformat(),
        "prs_mergeadas": len(prs),
        "horas_merge_mediana": round(median(horas), 1) if horas else None,
        "lineas_cambiadas": sum(p["additions"] + p["deletions"] for p in prs),
        "ci_corridas": len(runs),
        "ci_verde_pct": round(100 * sum(r["conclusion"] == "success" for r in runs) / len(runs), 1) if runs else None,
    }
    (out / "proceso.json").write_text(json.dumps(resumen, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(resumen, indent=1))


main()
