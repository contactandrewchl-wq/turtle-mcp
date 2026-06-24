#!/usr/bin/env bash
# MVP corto: demuestra cómo Turtle (1) despliega/equipa agentes con modelo por tarea y
# (2) cuántos tokens ahorra (recuperación en dos etapas + perfil de herramientas).
# Corre en Linux, macOS y Windows (Git Bash). No toca tu config real: usa una DB temporal.
#
#   bash examples/mvp_tokens_agentes.sh
#
# Requisitos: el binario `turtle` (usa target/release o target/debug si existe; si no, compila).
set -euo pipefail
cd "$(dirname "$0")/.."

BIN=""
for c in ./target/release/turtle ./target/release/turtle.exe ./target/debug/turtle ./target/debug/turtle.exe; do
  [ -x "$c" ] && BIN="$c" && break
done
if [ -z "$BIN" ]; then
  echo "Compilando turtle (release)…"; cargo build --release -p turtle-cli >/dev/null 2>&1
  BIN=$(ls ./target/release/turtle ./target/release/turtle.exe 2>/dev/null | head -1)
fi
echo "binario: $BIN"

TMP="$(mktemp -d)"; export TURTLE_DB="$TMP/mvp.db"; export TURTLE_PROJECT="mvp"
trap 'rm -rf "$TMP"' EXIT
tok(){ "$@" 2>/dev/null | grep -oE '[0-9]+ tokens' | grep -oE '[0-9]+' | head -1; }

echo; echo "════════ 1) EQUIPO + MODELO POR TAREA ════════"
"$BIN" skills seed >/dev/null 2>&1
echo "Turtle siembra 9 personas; cada una declara su modelo (Claude Code lo respeta por subagente):"
for d in agents/*/; do
  [ -f "$d/AGENT.md" ] || continue
  role=$(grep -E "^role:" "$d/AGENT.md" | head -1 | sed 's/role:[[:space:]]*//')
  model=$(grep -E "^[[:space:]]+model:" "$d/AGENT.md" | head -1 | sed 's/.*model:[[:space:]]*//')
  printf "   %-9s  %-13s -> %s\n" "$(basename "$d")" "$role" "$model"
done
echo "   (política: opus 4.8 para codear/arquitectura/razonar; sonnet SOLO para investigar/leer)"

echo; echo "════════ 2) AHORRO POR RECUPERACIÓN EN DOS ETAPAS ════════"
for i in 1 2 3 4 5; do
  "$BIN" guardar "Decisión $i" "Contenido largo $i: rmcp sobre stdio, capa de servicio, presupuesto de tokens en dos etapas, escalonamiento caliente/tibio/frío, FTS5 y WAL para concurrencia. Texto extra para que el contenido completo sea mucho más largo que el resumen." -s "Decisión $i: rmcp + dos etapas" -t decision >/dev/null 2>&1
done
IDX=$(tok "$BIN" buscar rmcp); FULL=$(tok "$BIN" buscar rmcp -v completo)
printf "   índice (etapa 1): %s tok   |   contenido completo (etapa 2): %s tok\n" "$IDX" "$FULL"
awk -v i="$IDX" -v f="$FULL" 'BEGIN{printf "   -> la etapa 1 usa %.0f%% menos tokens\n",(f>0)?100*(f-i)/f:0}'

echo; echo "════════ 3) AHORRO POR PERFIL DE HERRAMIENTAS MCP ════════"
INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"mvp","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
declare -A TOK
for prof in completo minimo; do
  b=$(printf '%s\n' "$INIT" | "$BIN" mcp --perfil "$prof" 2>/dev/null | grep '"id":2' | wc -c)
  TOK[$prof]=$(( b / 4 ))
  n=$(printf '%s\n' "$INIT" | "$BIN" mcp --perfil "$prof" 2>/dev/null | grep '"id":2' | grep -oE '"name":"[a-z_]+"' | wc -l)
  printf "   perfil %-9s %s tools  ~%s tokens de definición\n" "$prof" "$n" "${TOK[$prof]}"
done
awk -v c="${TOK[completo]}" -v m="${TOK[minimo]}" 'BEGIN{printf "   -> el perfil mínimo usa %.0f%% menos definición de tools por turno\n",(c>0)?100*(c-m)/c:0}'

echo; echo "Listo. Turtle es eficiente en tokens y suma equipo + modelo por tarea."
