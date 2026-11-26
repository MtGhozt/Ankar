> **Read first (update, 2026-11-25):** this prompt was written before prompts 09-16, so line numbers and some module names are stale. Read `AGENTS.md` and `docs/harness/` first and follow their rules. Find code by `rg` fragments, never by line number. Skip any step that is already done, and say so in the report. Since the window is now chromeless with a sidebar, open the stats panel from the existing stats icon in the tab bar as a right split pane, the same way the file viewer and diff pane open, instead of a floating modal. Update `LESSONS.md` / `FEATURES.md` / `ROADMAP.md` at the end. Don't push.

Mae, ahora sí: **arreglar el panel de stats** (`src/stats/`, commit `4f0f2ab`). Lo revisé entero. La **capa de datos sirve** (lee `opencode.db` en solo lectura, en un thread, con tests), así que **no se rehace**: se arregla en su lugar. Lo que sí se rehace es **la UI** (`stats/panel.rs`), porque hoy es un modal flotante que se desborda, no scrollea, no tiene cómo cerrarse con el mouse y no se parece al de midu. Este prompt va **después** de `07-estabilizacion.md` y `07b-pruebas.md`, sobre la rama `estabilizacion`. Un commit por bug, paso por paso.

## Reglas obligatorias
- Después de **cada** paso: `cargo build --release`, `cargo clippy -- -D warnings`, `cargo build` (debug) y `cargo test`. Todo en verde antes del commit.
- **Nunca escribas archivos con PowerShell** (`Set-Content`, `Out-File`, `>`, `>>`, `tee`: meten UTF-16/BOM). Todo con el editor, **UTF-8 sin BOM, LF**. La terminal solo para `cargo`, `git` y lectura.
- **Privacidad (no negociable):**
  - Solo lectura. **Nunca** escribas en la base de OpenCode ni en nada de `~\.cursor`.
  - **Nunca** abras `auth.json`, `cli-config.json`, ni las carpetas `log/` o `snapshot/` de OpenCode. **No** leas el `store.db` de cursor.
  - **Nunca** leas el texto de los mensajes o prompts: solo metadatos (tiempos, tokens, costo, modelo, proveedor, agente).
- **Nada de red, telemetría, cuentas ni IA.** No instalés nada (ni crates nuevos) sin preguntarle a Matias. Este prompt no necesita crates nuevos.
- **No hagas push.** Matias pushea.
- Las líneas citadas son de la rama `estabilizacion` en `56b0458` (después del 07; `src/stats/` solo cambió por el borrado del probe). El 07b corre algunas líneas de `app.rs`: **buscá siempre por el fragmento** con `rg`.
- Commits en inglés con prefijo `fix(stats N): ...`.

## Paso 1 — Privacidad: cursor-agent lee el texto de tus prompts
`stats/cursor_agent.rs:99-113` abre `~\.cursor\chats\*\*\prompt_history.json` y lo parsea entero (`fs::read_to_string` + `serde_json`) **solo para contar** cuántos prompts hay. Ese archivo es **el texto de tus prompts**. Viola la regla.
- **Fix:** borrá ese bloque completo. Para cursor-agent quedan solo `meta.json` (`createdAtMs`/`updatedAtMs`, `cursor_agent.rs:59-88`). Si ese `meta.json` trae algún campo de texto (título, etc.), **no lo uses**.
- En `opencode.rs:369-392` (consulta v1) el `SELECT` trae `message.data` como columna 1 y **no se usa**: sacala del `SELECT` (y corré los índices de `r.get` de abajo). Solo `json_extract` de campos numéricos/ids.

**Verificar:** `rg -n "prompt_history|read_to_string" src/stats` solo muestra `meta.json` y `stats.json`. Tests en verde.

Commit: `fix(stats 1): never read prompt text from cursor chats or OpenCode message bodies`.

## Paso 2 — Loader: cambios de rango que se pierden y memoria que no se suelta (`stats/loader.rs`)
Hoy la carga **ya** va en un thread (`loader.rs:92-112`, `"ankar-stats"`, con `catch_unwind`), bien. Bugs:
- **Cambio de rango perdido** (confirmado): `request()` hace `if self.inflight { return; }` (`loader.rs:73-75`). Si cambiás de 30 a 7 días mientras carga, el pedido de 7 días **se tira** y te quedás viendo 30 con el chip en 7.
  - **Fix:** contador `generation: u64`. Cada `request` incrementa y manda `(generation, range)` al thread. Si hay uno en curso, guardá `pending = Some(range)` y lanzalo cuando llegue el resultado. En `poll()` descartá resultados con `generation` vieja.
- **Si el thread no arranca**, `inflight` queda en `true` para siempre (`.ok()` en `loader.rs:112`). Si `spawn` falla, `inflight = false` y un warning en el snapshot.
- **Memoria:** el caché (`cache`) y `StatsPanel.snapshot` viven para siempre aunque cierres el panel. Agregá `StatsPanel::close()` que haga `snapshot = None`, `loader.clear_cache()` y `filter_agent = None`, y usalo en **todos** los caminos de cierre.
- `loader.rs:257`: `let _ = PathBuf::new();` → borralo (y el `use` si queda sin uso).
- **Lock largo del store de Ankar:** `compute()` hace `ankar.lock()` y adentro llama a `snapshot_for_range` (`loader.rs:181`, `235`), que por **cada hora guardada** (hasta 180 días × agentes) llama a `parse_hour_key` → `TimeZone::system()` (`ankar.rs:216-232`). Mientras tanto la UI espera el mismo mutex en `app.rs:141-143` cada frame. **Fix:** dentro del lock solo cloná `data.hours` (`let hours = store.hours_clone();`), soltá el lock y procesá afuera. Calculá `TimeZone::system()` **una vez** por carga y pasalo como parámetro (igual en `opencode.rs` `bump_activity`/`local_day`, `opencode.rs:533-555`, que hoy la piden **por fila**).
- **Doble pasada a la base:** para los porcentajes se vuelve a correr toda la carga del período anterior (`loader.rs:230-247`), o sea 2× OpenCode + 2× cursor. **Fix:** una sola consulta de `prev_since` a `until`, y repartí cada fila en "actual" o "anterior" según su `time_created`. Lo mismo para el recorrido de `~\.cursor\chats`.

**Verificar:** abrí stats, cambiá de rango 5 veces rápido: queda **el último** que tocaste. Cerrá el panel y medí RAM (Paso 6).

Commits: `fix(stats 2a): generation guard + pending request in stats loader`, `fix(stats 2b): free stats cache on close`, `fix(stats 2c): short ankar lock and one tz lookup per load`, `fix(stats 2d): single pass for current and previous window`.

## Paso 3 — El tiempo de Ankar casi nunca se cuenta (`stats/ankar.rs`)
- **Confirmado:** `tick()` (`ankar.rs:69-118`) hace `self.last_tick = now` **antes** de `dt.round()`; con la app redibujando cada 100 ms, `dt ≈ 0.1` → `round()` = 0 → `return`. **Ese tiempo se pierde.** Busy/waiting solo suma en frames separados por ≥ 0.5 s.
  - **Fix:** acumulador fraccional `pending_secs: f64`. Sumá `dt` (si `0 < dt <= 10`), y cuando `pending_secs >= 1.0` sacá la parte entera y aplicala. Nada de `round()`.
- **Agente mal nombrado:** `agent_key` (`ankar.rs:194-210`) usa `tab.command`, que para los launchers es `"powershell.exe -NoLogo -Command opencode"` → la fila del agente sale como `powershell.exe -nologo -command opencode`. Y las pestañas de PowerShell normales se cuentan como agentes con el nombre de la carpeta.
  - **Fix:** solo contá pestañas de launcher (`profile_id == "command"`), con clave = nombre del launcher en minúsculas (`tab.title`/`session_name` al crearla, `layout.rs:186-196`). Las shells comunes **no** suman al panel de agentes.
- **Usá el estado real del agente:** desde el 07b, las pestañas de OpenCode tienen `agent_state` (de OSC 7501: `working`/`idle`). Si la pestaña lo tiene, busy = tiempo en `working` y waiting = tiempo en `idle`; la heurística de `tab.status` queda solo para agentes que no mandan OSC 7501.
- **`starts` inflado:** suma 1 cada vez que la pestaña vuelve de Waiting a Active (o sea, cada vez que hay output después de 30 s quieta). **Fix:** `starts` = 1 por pestaña de launcher **abierta** (primera vez que se ve su `tab.id`), no por transición.
- **`dirty = true` siempre:** se marca aunque no se sumó nada (`ankar.rs:111`). Marcalo solo si cambió algo.
- **Escritura en el hilo de la UI:** `flush()` escribe `stats.json` (JSON pretty) cada 60 s **dentro del frame y con el mutex tomado** (`ankar.rs:115-125`). **Fix:** serializá a `Vec<u8>` (compacto, no pretty) dentro del lock, soltalo y escribí en un `std::thread::spawn` con `atomic_write`. En `on_exit` (`app.rs:437-440`) sí escribí sincrónico (una vez).
- La lectura de `stats.json` al arrancar (`app.rs:47`, `AnkarStatsStore::default()`) está bien así (es una vez).

**Verificar:** dejá una pestaña de OpenCode trabajando 2 minutos con stats cerrado, abrí stats en `7 days`: `AGENT TIME` muestra ~2 min y la fila de agente dice `opencode`. `stats.json` en `%APPDATA%`/carpeta de config de Ankar se actualiza sin trabones.

Commits: `fix(stats 3a): fractional tick accumulator`, `fix(stats 3b): count only launcher tabs, keyed by launcher name`, `fix(stats 3c): flush stats.json off the UI thread`.

## Paso 4 — De modal flotante a página completa (`app.rs` + `stats/panel.rs`)
Causas confirmadas de lo que ve Matias:
- **"No puedo scrollear":** `panel.rs` es un `egui::Area` centrado de tamaño fijo (`panel.rs:69-97`, `screen*0.92 × 0.90`, tope 1100×780) **sin `ScrollArea` exterior**. Solo modelos/agentes tienen scroll interno.
- **"Algo se dibuja abajo":** la fila de abajo calcula `bot_h = ui.available_height() - 36.0` con `.max(180.0)` (`panel.rs:277-298`) y **después** se agregan los warnings (uno por línea) y `Updated …s ago · Everything is read locally…` (`panel.rs:301-319`). Con 1–2 warnings (por ejemplo `cursor-agent: no local chats folder`) eso pasa el alto fijo y se pinta **fuera del borde del panel**, encima de la terminal oscurecida.
- **"No puedo salir":** el `esc` de arriba (`panel.rs:122`) es un `label`, **no es botón**; no hay X; el fondo oscuro (`panel.rs:80-81`) es solo pintura (no captura clics), así que clic afuera no cierra y **los clics pasan** a la sidebar/pestañas de abajo. Esc por teclado sí cierra en el código (`panel.rs:64-67`), pero **la terminal de abajo sigue con el foco** y también recibe la Esc y todo lo que tecleás (OpenCode reacciona). Verificá Esc igual.
- Además se dibuja **encima** de todo lo demás: sidebar, pestañas y panes se siguen dibujando y procesando cada frame debajo.
- **El cartel rojo de egui** (`First use of ScrollArea ID … / Widget is above this text / ID clashes`, solo en build debug): el panel crea dos `ScrollArea::vertical()` **sin `id_salt`** (`panel.rs:354` agentes y `panel.rs:832` modelos) dentro de `Frame`s/`horizontal` que egui numera igual en algunos frames, más el `Area` con `Id::unique("ankar_stats_panel")` (`panel.rs:83`) que se crea encima de todo. En release no se ve, pero es el mismo bug de layout. **Fix:** no queda ningún `ScrollArea` interno (ver Paso 5) y la única `ScrollArea` de la página lleva `.id_salt("stats_page")`. Con `cargo run` (debug) **no** tiene que aparecer ningún texto rojo de egui.
- **Clics y teclas que pasan para atrás** (Matias lo confirmó: con stats abierto puede escribir en OpenCode y tocar la sidebar): el fondo oscuro es solo `painter.rect_filled` (`panel.rs:80-81`), no captura nada, y la terminal conserva el foco. Con la vista `View::Stats` de abajo eso desaparece porque nada de atrás se dibuja ni recibe input; **verificalo explícitamente**.

**Fix:**
- `AnkarApp.view: View { Terminals, Stats }`. Con `Stats` **no** se dibujan la sidebar, la barra de pestañas, el visor ni los panes: solo un `CentralPanel` con `Frame::NONE.fill(#000000)` y la página. Las sesiones siguen vivas con `set_visible(false)` + `release_snapshot()`, igual que las pestañas ocultas (`app.rs:159-168`). `TerminalView::show` **no** se llama, así que la terminal no recibe teclas.
- Abrir: Ctrl+Shift+I (`app.rs:837-838`), el ícono de la barra de pestañas (`open_stats`, `app.rs:328-339`) y la acción de la paleta (`app.rs:100-101`). Hoy `self.stats.draw(ui)` se llama al final de todo (`app.rs:434`), **encima** de la UI que se sigue dibujando.
- Cerrar: **Esc**, el chip `esc` (clickeable) y Ctrl+Shift+I otra vez. Esc primero quita el filtro de agente si hay; si no, cierra. Al cerrar: `StatsPanel::close()` (Paso 2) y la terminal enfocada recupera el foco sola.
- `handle_shortcuts` con `view == Stats`: solo Ctrl+Shift+I y Esc. Nada de Ctrl+Shift+T/W/etc.
- **Toda la página en `ScrollArea::vertical().auto_shrink([false, false])`**. Nada de `set_min_size/set_max_size` ni `available_height() - N`: las alturas son fijas por sección (Paso 5) y el scroll hace el resto. Abajo de todo: warnings, la línea de privacidad, 24 px de margen, **y nada más**.
- `draw_contents` clona el snapshot entero **cada frame** (`panel.rs:101-104`). Usá una referencia (`let Some(snap) = self.snapshot.as_ref()` y pasá `&StatsSnapshot`).
- Mientras carga, repintá cada 100 ms (`panel.rs:57-60` ya lo hace) **solo** mientras `is_loading()`; con datos listos, nada de repaint periódico.

Commit: `fix(stats 4): full-screen stats view with scroll, clickable esc, terminals paused`.

## Paso 5 — Diseño 1:1 con midu (`stats/panel.rs`)
Referencia: `docs/design/midterm-spec/ref/midu-stats.jpg` (2048×1172, es 2×: dividí sus píxeles entre 2). Ancho de referencia 1024 lógicos; si la ventana es más ancha, las columnas son proporcionales, con márgenes laterales de 22.

**Colores** (reemplazan las `const` de `panel.rs:12-23`)
| Token | Valor |
|---|---|
| Fondo | `#000000` |
| Tarjeta | `#0C0C0C`, borde 1 px `#111111`, radio 8 |
| Texto | `#E7E7E7` |
| Texto muted | `#838383` |
| Separadores | `#191919` |
| Input (barras) | `#3E3E3E` |
| Output (barras) | `#4A9FFF` |
| Heatmap | 5 pasos de `#212121` a `#C2C2C2` |
| Badge positivo | texto `#5BAF78` sobre `#0D2D15`, radio 4 |
| Badge negativo | texto `#E5695B` sobre `#2A0F0F` |
| Proveedores | Anthropic `#BB5E4C`, Google `#4178D3`, OpenAI `#30987F`, xAI `#9A9A9A`, otros `#5A5A5A` |

Tipografía: Inter. Títulos de sección 11 px MAYÚSCULAS muted. Números de tarjeta 26 px. Cifras de tablas alineadas a la derecha con ancho fijo por columna.

**Encabezado**
- `Agent usage` 14 px `#E7E7E7` en (22, 8); debajo, el rango (`Oct 23 – today`) 12 px muted.
- A la derecha: control segmentado `7 days | 30 days | 90 days | All`, alto 22, fondo `#101010`, radio 6, activo `#242424` con texto `#E7E7E7`, resto muted, 12 px. (Hoy son `Frame`s sueltos en RTL, `panel.rs:121-158`.)
- A 18 px: chip `esc` 29×17, fondo `#0E0E0E`, borde `#1E1E1E`, radio 4, 11 px muted, **clickeable, cierra**, con cursor de mano.

**4 tarjetas KPI** (alto 105, gap 11)
- **Etiquetas visibles**: `INPUT TOKENS`, `OUTPUT TOKENS`, `TASKS`, `AGENT TIME` en 11 px muted en (12, 16). Hoy el parámetro `_title` **no se usa** (`panel.rs:453`) y la etiqueta solo sale en el tooltip: dibujala.
- Badge de delta arriba a la derecha: verde si sube, rojo si baja, **sin badge** si no hay período anterior (`All`). Hoy siempre es verde.
- Número grande en y+32; sublínea 11 px muted en y+60 (`98% from cache` / `22.9K per turn` / `2,674 turns` / `up to N at once`).
- Mini barras de y+73 a y+94: **una por día del rango** (máx. 30, días sin datos = barra vacía; hoy solo salen los días con datos), `#3A3A3A`, gap 2.

**TOKENS BY DAY** (izquierda, ~57 % del ancho, alto 135)
- A la derecha, muted: `29 of 30 days with agents · 402M/day`.
- Cero horizontal a ~60 % del alto: **input** (gris) hacia arriba, **output** (azul) hacia abajo, **escalas separadas**.
- A la izquierda: máximo de input (`1.3B` + `input`) arriba y de output (`6.2M` + `output` en azul) abajo. Hoy faltan.
- Fechas abajo: primera, la del medio y `today`. Tooltip por barra: fecha, input, output.

**WHEN THEY WORK** (derecha, alto ~145)
- `most active Sat 18 h` a la derecha. Grilla 7 × 24 (`M T W T F S S` a la izquierda), celdas ~15.5×12, gap 2. Horas `0 6 12 18` abajo. Leyenda `less ■■■■■ more`. Tooltip `Sat 18:00 · N events`.

**MODELS** (izquierda)
- Encabezado + línea `#191919` + `16 models` muted a la derecha.
- Barra apilada por proveedor, alto 4, radio 2; leyenda `● Anthropic 92%` (punto de color, **sin logos de marca nuevos**).
- Columnas `input output tasks`. Filas de alto 20.5: punto + nombre (12 px) + barra del color del proveedor (alto 6, radio 3, mínimo punto de 4 px) + cifras. Top 6 + fila `N more` muted con la suma del resto. **Sin** `ScrollArea` interno.

**AGENTS** (derecha)
- `click one to see only its usage` muted a la derecha. Columnas `input output time`.
- Filas: logo (usá `logos::agent_visual` del 07 a 14 px) + nombre + barra `#898989` + cifras (`—` si no hay tokens). Hover `#141414` + cursor de mano. **Sin** `ScrollArea` interno.
- **Clic = filtrar toda la página** por ese agente. Hoy solo esconde las otras filas de la tabla de agentes (`panel.rs:357-362`, `429-436`) y el resto de la página no cambia. **Fix:** `filter_agent` va al loader (`request(range, filter)`) y `compute` filtra **todo** (tokens, días, heatmap, modelos) por agente (OpenCode: `$.agent`; Ankar: clave del launcher; cursor: agente `cursor-agent`). Las otras filas quedan al 40 % y aparece `× clear filter` en el encabezado. Otro clic en la misma fila o Esc lo quitan.

**Pie**: warnings 11 px muted + `Everything is read locally. Ankar sends nothing anywhere.` + 24 px. Nada pegado al borde.

**Estados**: cargando sin caché = las mismas cajas con `—` y barras vacías (esqueleto, nunca en blanco). Sin datos = ceros + warnings.

**Rendimiento**: todo con `painter.rect_filled` y textos cortos; nada de texturas ni galleys gigantes. Con la página cerrada no se dibuja ni se calcula nada.

Commits (uno por sección): `fix(stats 5a): header + range control + clickable esc`, `fix(stats 5b): KPI cards with titles and signed badges`, `fix(stats 5c): tokens by day + heatmap`, `fix(stats 5d): models and agents tables, page-wide agent filter`.

## Paso 6 — Verificación en Windows (sin código salvo el doc; pegame los resultados)
1. Ctrl+Shift+I: la página aparece **en menos de 100 ms** con el esqueleto y los datos llegan solos. La ventana nunca dice "No responde".
2. Cambiá de rango 5 veces rápido: no se traba y queda el último.
3. Rueda del mouse hasta abajo: no hay nada pintado fuera de lugar ni pegado al borde.
4. Esc cierra (si hay filtro, primero lo quita). El chip `esc` cierra. Al volver, la terminal recibe teclas sin hacer clic, y **mientras** la página estaba abierta lo que tecleaste **no** llegó a la terminal.
4b. Con la página abierta, clic donde antes estaba la sidebar o las pestañas: no pasa nada atrás. En `cargo run` (debug) no aparece ningún texto rojo de egui (`ID clashes`).
5. Clic en un agente: toda la página cambia a ese agente.
6. Captura a 1024 px de ancho al lado de `midu-stats.jpg`: decime qué quedó distinto a propósito.
7. RAM (solo lectura): `Get-Process ankar | Select-Object @{n='PrivateMB';e={[math]::Round($_.PrivateMemorySize64/1MB,1)}}` con 3 pestañas:
   - página cerrada;
   - abierta en `30 days`;
   - cerrada otra vez (no más de 3–5 MB arriba de la primera).

   Meta igual que en el 07: **< 100 MB Private con 3 pestañas** con la página cerrada. Anotalo en `docs/ram.md` (sección `## Stats page (08)`, con el editor).

Commit: `docs(stats 6): stats page RAM and verification notes`.

No hagas push.
