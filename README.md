# Turtle 🐢

**Memoria persistente y coordinación multi-agente para CLIs de IA** — local-first, eficiente en tokens, sin dependencias de runtime.

Turtle le da a los CLIs de codificación (Claude Code, Cursor, Codex, Gemini CLI, …) **memoria que persiste entre sesiones** y un **bus de coordinación** entre varios agentes en paralelo, a través de un **servidor MCP** y una **CLI** sobre una base **SQLite local**. Un solo binario autocontenido: sin Node, sin servicios, sin claves de API. Tus datos no salen de tu máquina.

---

## ¿Qué hace?

- **Memoria entre sesiones.** Guardás decisiones/arquitectura/correcciones/convenciones y el agente las recupera en sesiones futuras, en vez de re-derivarlas (y re-pagar los tokens).
- **Recuperación eficiente en tokens.** Búsqueda en **dos etapas**: primero un índice barato (título + resumen, sin contenido) y solo traés el cuerpo completo de lo que de verdad abrís. (Ver *Ahorro de tokens* abajo.)
- **Temas evolutivos con historial.** Una memoria con `topic_key` se **actualiza en vez de duplicarse**, y la versión anterior queda archivada: podés consultar "qué sabíamos del tema en tal momento".
- **Consolidación asistida.** Detecta memorias probablemente duplicadas y te las propone para fusionar (vos decidís; sin IA).
- **Coordinación multi-agente.** Agentes con rótulo, feed de actividad, y mensajes/relevos (handoffs) por rol o difusión.
- **Capa de skills + personas.** Trae un bundle embebido de skills y 9 personas (subagentes); ingiere `skills/` y `agents/` y los indexa para cargarlos bajo demanda.
- **Robusto y portable.** Diagnóstico de salud, export/import JSON, sincronización por fragmentos git, escalonamiento caliente/tibio/frío y supervivencia a la compactación de contexto.

**Multi-CLI por diseño.** Turtle es un MCP provider-agnóstico: lo consume cualquier CLI de modelo frontera. El modelo lo decide el CLI que uses (Claude Code → Claude, Codex → OpenAI, …); Turtle no maneja claves ni proveedores.

---

## Ahorro de tokens

Es el eje de Turtle. El costo de "recordar" en la mayoría de las capas de memoria es alto porque devuelven el **contenido completo** de cada coincidencia en cada búsqueda. Turtle lo evita con tres palancas:

**1. Recuperación en dos etapas (la principal).** `memory_search` devuelve un **índice barato** —id, título, resumen, sin contenido— y solo recuperás el cuerpo completo con `memory_get` de las pocas memorias que abrís.

Medido en datos reales (12 resultados, ~580 caracteres cada uno; `estimate_tokens = chars/4`):

| Etapa | Tokens (12 result.) | Por resultado |
|---|---:|---:|
| **Índice** (por defecto) | **600** | ~50 |
| Compacto (con extracto) | 1 452 | ~121 |
| Completo (contenido full) | 1 767 | ~147 |

→ **−66 %** en el primer golpe. En un recall típico (buscás 12, abrís 2): `600 + 2×147 ≈ 894` tok vs `~1 767` de una capa de una sola etapa → **~50 % menos**, y la brecha crece con el corpus.

**2. Perfiles de herramientas.** Cada servidor MCP inyecta sus esquemas de herramientas en el contexto **por turno**. `turtle mcp --perfil minimo` expone solo el núcleo (6 tools) en vez de las 30 → **~70 % menos** de "impuesto" de definiciones por turno. (El default es `completo` porque el protocolo arranca con coordinación.)

**3. Escalonamiento + presupuesto.** Las memorias transitan caliente → tibio → frío por antigüedad de acceso, manteniendo chica la superficie activa; la búsqueda **recorta por presupuesto de tokens** y nunca te llena el contexto.

---

## Instalación

Turtle es un **binario único**. No necesita Rust ni un compilador para usarse.

### Homebrew (macOS / Linux)

```sh
brew install contactandrewchl-wq/tap/turtle
```

### Linux / macOS (instalador de un comando)

```sh
curl -fsSL https://raw.githubusercontent.com/contactandrewchl-wq/turtle-mcp/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/contactandrewchl-wq/turtle-mcp/main/install.ps1 | iex
```

Cubre Linux (x86-64 y ARM64, estáticos con musl), macOS (Apple Silicon e Intel) y Windows x86-64. El instalador descarga el binario del último [Release](https://github.com/contactandrewchl-wq/turtle-mcp/releases) (v0.1.0) y lo deja en el `PATH`.

### Desde el código (cualquier SO)

Requiere la toolchain de Rust ([rustup](https://rustup.rs)):

```sh
git clone https://github.com/contactandrewchl-wq/turtle-mcp
cd turtle-mcp
cargo install --path crates/turtle-cli --locked
```

**Compilador de C por plataforma:** macOS → Command Line Tools de Xcode; Linux → `build-essential`; Windows → `stable-x86_64-pc-windows-gnu` (mingw, sin Visual Studio) **o** `-msvc` (Build Tools de VS con el workload de C++).

Verificá: en una terminal nueva, `turtle --version`.

---

## Puesta en marcha paso a paso

1. **Instalá** (arriba) y comprobá: `turtle --version`.
2. **Instalá en tu CLI todo de una** (registra el MCP, inyecta el protocolo, instala las personas
   y **siembra el bundle embebido —21 skills + 9 personas— en la base**):

   ```sh
   turtle install            # menú: detecta los clientes instalados y elegís
   turtle install claude-code   # o directo por nombre
   ```

   Soporta `claude-code`, `claude-desktop`, `cursor`, `windsurf`, `gemini-cli` y `codex`.

   > `turtle install` = sembrar el bundle **+** `turtle setup`. Si usaste solo `turtle setup` (que
   > **no** siembra) y `turtle stats` muestra **0 skills**, corré `turtle skills seed`.
3. **Reiniciá tu CLI** para que levante el servidor MCP.
4. **Probá la memoria**:

   ```sh
   turtle guardar "Usamos rmcp para el MCP" "Turtle expone el servicio por MCP por stdio." -t decision
   turtle buscar rmcp
   ```
5. **Diagnóstico** (opcional): `turtle doctor`.

---

## Registrar en tu cliente y revisar los settings

`turtle install <cliente>` siembra el bundle de skills/personas y luego corre `turtle setup`.
`turtle setup <cliente>` (sin sembrar) hace tres cosas, todas **idempotentes** (fusiona, no pisa lo ajeno):

- Escribe la entrada del servidor MCP en la config del cliente, con la **ruta absoluta** del binario. Equivale a:

  ```json
  { "command": "turtle", "args": ["mcp"] }
  ```
- Inyecta el **protocolo de uso** de Turtle en las instrucciones del cliente (en Claude Code, `~/.claude/CLAUDE.md`; en Codex, `~/.codex/AGENTS.md`), en un bloque marcado y reemplazable.
- En Claude Code, además instala las **9 personas** como subagentes (`~/.claude/agents/`) y cablea los hooks de sesión.

**Dónde viven las cosas:**

| Qué | Dónde |
|---|---|
| Base de datos (tu memoria) | carpeta de datos del usuario, o `--db` / `$TURTLE_DB` |
| Config MCP de Claude Code | `~/.claude.json` |
| Config MCP de Codex | `~/.codex/config.toml` (tabla `[mcp_servers.turtle]`) |
| Overrides de modelo por persona | `~/.turtle/models.conf` |

**Revisar / verificar:**

```sh
turtle --version                       # binario en el PATH
turtle doctor                          # esquema, integridad, índices FTS, duplicados
turtle stats                           # totales por proyecto y tipo
cat ~/.claude.json | grep -A2 turtle   # ver la entrada MCP registrada (Claude Code)
```

**Quitar Turtle de un cliente** (no borra tu memoria): `turtle uninstall <cliente>`.

---

## Uso (CLI)

El dato principal va como argumento y el **proyecto se autodetecta** del repo/carpeta actual (override con `-p` o `$TURTLE_PROJECT`).

**Memoria**

```sh
turtle guardar "Servidor MCP con rmcp" "Turtle expone el servicio por MCP y CLI." -s "MCP + CLI"
turtle guardar "Esquema del contrato" "v2 evolucionado" --topic api/contrato   # tema evolutivo (upsert)
turtle buscar rmcp                 # índice barato; -g busca en todos los proyectos
turtle buscar rmcp -v compacto     # verbosidad: indice (def.) · compacto · completo
turtle ver <id>                    # contenido completo
turtle ctx trabajar en el MCP      # contexto del proyecto actual
turtle guardar "Regla" "..." -i pinned   # importancia: pinned / normal / ephemeral
git log -1 | turtle guardar "Último commit"   # el contenido también entra por stdin
```

**Historial temporal y consolidación**

```sh
turtle historial <id>              # versiones anteriores de un tema (más reciente primero)
turtle duplicados                  # propone memorias duplicadas para consolidar (sin IA)
turtle relacionar <id_b> <id_a> duplicate   # replaces · conflicts · relates · duplicate
turtle comparar <id_a> <id_b>      # contenido de ambas, para decidir
```

**Sesiones y coordinación**

```sh
turtle sesion iniciar implementar sesiones -a dev   # muestra el contexto inicial (deltas)
turtle sesion cerrar <id>                           # resumen automático de lo hecho
turtle mensaje "revisá el endpoint" -a backend --de frontend   # relevo dirigido
turtle bandeja backend                              # bandeja de un rol (pendientes)
turtle checkpoint "voy por el paso 3 de 5"          # sobrevive a la compactación
turtle actividad                                    # feed de actividad por agente
```

**Modelo por persona** (Claude Code, según tu subscripción):

```sh
turtle modelos                     # menú interactivo: elegí persona y modelo
turtle modelos set ada=opus charles=haiku   # directo
turtle modelos reset               # volver a los modelos por defecto
```

**Skills, salud y portabilidad**

```sh
turtle skills importar             # ingiere skills/ y agents/ (proyecto + ~/.claude)
turtle skills seed                 # carga el bundle embebido (21 skills + 9 personas)
turtle doctor --reparar            # repara índices FTS desincronizados
turtle exportar -p proy --salida backup.json   # respaldo JSON abierto; importar no duplica
turtle sync exportar .turtle/mem   # un <id>.json por memoria (versionable en git sin conflictos)
```

---

## Servidor MCP

`turtle mcp` habla MCP por stdio; cualquier cliente compatible se conecta. Expone **30 herramientas**, entre ellas `memory_save`, `memory_search`, `memory_get`, `memory_history`, `memory_duplicates`, `context_get`, `session_start`/`session_close`, `message_send`/`inbox`, `relation_add`, `skills_search`/`skill_get`, y más. Las herramientas con proyecto lo **autodetectan** del directorio (o `$TURTLE_PROJECT`) cuando se omite.

Para gastar menos tokens de definición por turno: `turtle mcp --perfil minimo` (o `TURTLE_MCP_PROFILE=minimo`) deja solo el núcleo de 6 tools (~70 % menos); `completo` (por defecto) expone las 30.

### Línea de estado (Claude Code)

`turtle statusline` imprime rama de git + modelo + consumo estimado de tokens. En `~/.claude/settings.json`:

```json
{ "statusLine": { "type": "command", "command": "turtle statusline" } }
```

```
🐢 feat/viajes · Sonnet 4.6 · 5h 15.3M tok · sem 17.0M tok
```

### Hooks de sesión (presencia automática)

`turtle setup claude-code` cablea dos hooks para que Turtle quede presente sin pedirlo:

- `turtle hook session-start` — al iniciar, inyecta las memorias recientes del proyecto.
- `turtle hook prompt-submit` — antes de responder, inyecta las memorias relevantes al pedido.

A mano, en `~/.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "turtle hook session-start" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "turtle hook prompt-submit" }] }]
  }
}
```

---

## Desarrollo

Requiere [rustup](https://rustup.rs). Verificación local:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Estructura:

```
crates/
  turtle-core/      Tipos del dominio
  turtle-data/      SQLite, migraciones, FTS5
  turtle-embed/     (stub opcional)
  turtle-usage/     Medición de consumo de tokens
  turtle-service/   Casos de uso (recuperación en 2 etapas, consolidación)
  turtle-mcp/       Servidor MCP
  turtle-cli/       Binario `turtle`
```

---

## Licencia

MIT — ver [`LICENSE`](LICENSE).
