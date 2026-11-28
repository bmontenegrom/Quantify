#!/usr/bin/env bash
set -u

METODO=v1
TOKEI=15.0.0
RCA=0.0.25
LLVM_COV=0.9.2
MUTANTS=27.1.0
MACHETE=0.10.1
AUDIT=0.23.0
CODEQL_V=2.27.3
KNIP=6.49.0
MADGE=8.0.0
SCAN=5.1.0
LIGHTHOUSE=13.6.0
AXE=4.14.1
SEMGREP=semgrep/semgrep:1.181.1
MUTANTS_JOBS=4
MUTANTS_MAX=2h

PASOS=${PASOS:-tamano,tests,e2e,clippy,complejidad,cobertura,mutantes,deps,politica,audit,sast,ux,sonar}
paso() { [[ ",$PASOS," == *",$1,"* ]]; }

REPO=$(cd "$(dirname "$0")/../.." && pwd)
CODEQL=${CODEQL:-/c/Repositorio/calidad-tools/codeql/codeql.exe}
SONAR_URL=${SONAR_URL:-http://localhost:9000}
PORT=${PORT:-18080}

falla=0
version() {
  local salida; salida=$("${@:2}" 2>&1 | head -1)
  [[ "$salida" == *"$1"* ]] || { echo "versión incorrecta: '${*:2}' da '$salida', se espera $1"; falla=1; }
}
paso tamano && version "$TOKEI" tokei --version
paso complejidad && version "$RCA" rust-code-analysis-cli --version
paso cobertura && version "$LLVM_COV" cargo llvm-cov --version
paso mutantes && version "$MUTANTS" cargo mutants --version
paso deps && version "$MACHETE" cargo machete --version
paso audit && version "$AUDIT" cargo audit --version
paso sast && version "$CODEQL_V" "$CODEQL" version
[ $falla = 0 ] || exit 1

git -C "$REPO" fetch -q origin
SHA=$(git -C "$REPO" rev-parse --short=7 "${1:-origin/development}") || exit 1
FECHA=$(git -C "$REPO" show -s --format=%cs "$SHA")
OUT="$REPO/docs/calidad/datos/$FECHA-$SHA"
CRUDO="$OUT/crudo"
WT=$(mktemp -d)/quantify-$SHA
mkdir -p "$CRUDO"

git -C "$REPO" worktree add --detach "$WT" "$SHA" >/dev/null || exit 1
cd "$WT" || exit 1
printf '%s\n%s\n%s\n' "$METODO" "$(git show -s --format='%H%n%cI%n%s' HEAD)" "$(uname -srm); $(rustc --version); node $(node --version)" > "$OUT/commit.txt"
npm ci --no-audit --no-fund > "$CRUDO/npm-ci.log" 2>&1

if paso tamano; then
  echo ">> tamaño"
  tokei src static -o json > "$CRUDO/tokei.json"
fi

if paso tests; then
  echo ">> tests"
  cargo test --locked > "$CRUDO/cargo-test.log" 2>&1
fi

if paso e2e; then
  echo ">> e2e"
  npm run test:e2e > "$CRUDO/e2e.log" 2>&1
fi

if paso clippy; then
  echo ">> clippy"
  cargo clippy --all-targets --locked --message-format=json 2>/dev/null | grep '"reason":"compiler-message"' > "$CRUDO/clippy.json"
  cargo clippy --all-targets --locked --message-format=json -- -W clippy::pedantic 2>/dev/null | grep '"reason":"compiler-message"' > "$CRUDO/clippy-pedantic.json"
fi

if paso complejidad; then
  echo ">> complejidad"
  rm -rf "$CRUDO/rca" && mkdir -p "$CRUDO/rca"
  rust-code-analysis-cli -m -O json -p src -p static -X 'src/**/tests.rs' -X 'src/**/golden_tests.rs' -X 'static/constants.json' -o "$CRUDO/rca" > /dev/null 2>&1
fi

if paso cobertura; then
  echo ">> cobertura"
  cargo llvm-cov --locked --lcov --output-path "$CRUDO/rust.lcov" > "$CRUDO/llvm-cov.log" 2>&1
  cargo llvm-cov report --json --summary-only --output-path "$CRUDO/rust-cov.json" >> "$CRUDO/llvm-cov.log" 2>&1
  node --test --experimental-test-coverage \
    --test-reporter=spec --test-reporter-destination="$CRUDO/node-test.txt" \
    --test-reporter=lcov --test-reporter-destination="$CRUDO/js.lcov" > /dev/null 2>&1
  sed -i -e 's#[\]#/#g' -e "s#^SF:.*/quantify-$SHA/#SF:#" "$CRUDO/rust.lcov" "$CRUDO/js.lcov"
fi

if paso mutantes; then
  echo ">> mutantes (hasta $MUTANTS_MAX)"
  cargo mutants --list --file src/uncertainty.rs --file 'src/computation/**' > "$CRUDO/mutants-list.txt" 2>&1
  cargo mutants --file src/uncertainty.rs --file 'src/computation/**' \
    --shuffle --cargo-test-arg=--lib --timeout-multiplier 3 --jobs "$MUTANTS_JOBS" --output "$CRUDO" > "$CRUDO/mutants.log" 2>&1 &
  mutants=$!
  ( sleep "$MUTANTS_MAX"; taskkill //F //T //PID "$(cat /proc/$mutants/winpid)" > /dev/null 2>&1 ) &
  vigia=$!
  wait $mutants
  kill $vigia 2> /dev/null
fi

if paso deps; then
  echo ">> dependencias y ciclos"
  cargo machete > "$CRUDO/machete.txt" 2>&1
  printf '{"entry":["static/app.js","tests/*.test.js","tests/e2e/*.mjs"],"project":["static/**/*.js","tests/**/*.{js,mjs}"]}' > knip.json
  npx -y "knip@$KNIP" --reporter json --no-exit-code > "$CRUDO/knip.json" 2> "$CRUDO/knip.log"
  npx -y "madge@$MADGE" --circular --json static/ > "$CRUDO/madge.json" 2> "$CRUDO/madge.log"
fi

if paso politica; then
  echo ">> política de comentarios"
  python scripts/comments.py --check > "$CRUDO/comments.txt" 2>&1
fi

if paso audit; then
  echo ">> auditoría de dependencias"
  cargo audit --json > "$CRUDO/cargo-audit.json" 2> "$CRUDO/cargo-audit.log"
  npm audit --omit=dev --json > "$CRUDO/npm-audit.json" 2>&1
fi

if paso sast; then
  echo ">> semgrep"
  MSYS_NO_PATHCONV=1 docker run --rm -v "$(cygpath -m "$WT"):/src" -w /src "$SEMGREP" semgrep scan \
    --config p/rust --config p/javascript --config p/secrets --config p/owasp-top-ten \
    --metrics=off --no-git-ignore --json -o crudo-semgrep.json src static > "$CRUDO/semgrep.log" 2>&1
  mv crudo-semgrep.json "$CRUDO/semgrep.json" 2>/dev/null
  echo ">> codeql"
  db=$(mktemp -d)
  "$CODEQL" database create "$db" --db-cluster --language=rust,javascript --build-mode=none --source-root=. --overwrite > "$CRUDO/codeql.log" 2>&1
  for l in rust javascript; do
    "$CODEQL" database analyze "$db/$l" "codeql/$l-queries:codeql-suites/$l-security-and-quality.qls" \
      --format=sarif-latest --output="$CRUDO/codeql-$l.sarif" >> "$CRUDO/codeql.log" 2>&1
  done
  rm -rf "$db"
fi

if paso ux; then
  echo ">> lighthouse y axe"
  cargo build --release --locked > "$CRUDO/build-release.log" 2>&1
  npm i --no-save --no-audit --no-fund "lighthouse@$LIGHTHOUSE" "@axe-core/playwright@$AXE" > /dev/null 2>&1
  npx playwright install chromium > /dev/null 2>&1
  datos=$(mktemp -d)
  DATABASE_URL="sqlite:$(cygpath -m "$datos")/quantify.db" UPLOAD_DIR="$datos/uploads" SEED_DEMO_DATA=true \
    APP_BIND_ADDR="127.0.0.1:$PORT" ./target/release/quantify > "$CRUDO/server.log" 2>&1 &
  server=$!
  until curl -sf "http://127.0.0.1:$PORT/" > /dev/null; do sleep 1; done
  node "$REPO/scripts/calidad/ux.mjs" "http://127.0.0.1:$PORT" "$CRUDO" > "$CRUDO/ux.log" 2>&1
  kill $server; wait $server 2>/dev/null
  rm -rf "$datos"
fi

if paso sonar && [ -n "${SONAR_TOKEN:-}" ]; then
  echo ">> sonar"
  npx -y "@sonar/scan@$SCAN" -Dsonar.host.url="$SONAR_URL" -Dsonar.token="$SONAR_TOKEN"     -Dsonar.projectKey=quantify -Dsonar.projectName=Quantify -Dsonar.sourceEncoding=UTF-8     -Dsonar.sources=src,static -Dsonar.tests=src,tests     -Dsonar.exclusions='src/**/tests.rs,src/**/golden_tests.rs'     -Dsonar.test.inclusions='src/**/tests.rs,src/**/golden_tests.rs,tests/*.js,tests/e2e/*.mjs'     -Dsonar.rust.clippy.enabled=false \
    -Dsonar.projectDate="$(TZ=UTC git show -s --date=format-local:'%Y-%m-%dT%H:%M:%S+0000' --format=%cd HEAD)" \
    -Dsonar.projectVersion="$SHA" \
    -Dsonar.rust.clippyReport.reportPaths="$CRUDO/clippy.json" \
    -Dsonar.rust.lcov.reportPaths="$CRUDO/rust.lcov" \
    -Dsonar.javascript.lcov.reportPaths="$CRUDO/js.lcov" \
    -Dsonar.qualitygate.wait=true > "$CRUDO/sonar.log" 2>&1
  m=ncloc,bugs,vulnerabilities,code_smells,security_hotspots,sqale_index,sqale_debt_ratio,sqale_rating,reliability_rating,security_rating,duplicated_lines_density,coverage,line_coverage,branch_coverage,complexity,cognitive_complexity,alert_status
  curl -s -u "$SONAR_TOKEN:" "$SONAR_URL/api/measures/component?component=quantify&metricKeys=$m" > "$CRUDO/sonar.json"
  curl -s -u "$SONAR_TOKEN:" "$SONAR_URL/api/measures/component_tree?component=quantify&qualifiers=DIR&metricKeys=ncloc,duplicated_lines_density&ps=500" > "$CRUDO/sonar-dirs.json"
  curl -s -u "$SONAR_TOKEN:" "$SONAR_URL/api/issues/search?components=quantify&issueStatuses=OPEN,CONFIRMED&ps=1&facets=rules,languages,impactSoftwareQualities,impactSeverities" > "$CRUDO/sonar-issues.json"
fi

cd "$REPO" && git worktree remove --force "$WT"
echo "Listo: $OUT"
