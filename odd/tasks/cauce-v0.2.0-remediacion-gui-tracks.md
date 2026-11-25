# Cauce v0.2.0 — Remediación de UX: pistas por archivo en la página Convertir

**Feature:** Reemplazar el bloque global de pistas por una edición por archivo, en un diálogo modal, sin cambiar la versión: se mantiene `v0.2.0` y se reemplaza el artefacto publicado.
**Autoridad normativa:** pedido del Tech Lead (L1) sobre `docs/ui-mvp.md` §4 y las pautas de Libadwaita ya usadas en la app. Complementa `docs/contracts.md` §2.2 y §6, que **no** cambian.
**Estado:** Plan redactado. Implementación pendiente de este documento.

---

## El problema

En v0.2.0 la página Convertir muestra un bloque `tracks_group` **global** al pie de la lista. Ese bloque edita la selección de `accepted.first()` —el primer archivo aceptado— mientras la UI sugiere que aplica a todos. Con varios videos:

- el usuario cree que elige el audio de la cola entera;
- en realidad solo cambia el del primero;
- el resto se encola con los defaults del probe, sin que nadie lo haya decidido.

Es un defecto de UX **y** de corrección: la interfaz afirma algo que el código no hace.

---

## Specs

Citas literales del pedido del usuario. No se reformulan ni se amplían.

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S1 | "Eliminar ese tracks_group estático global del fondo de la página de conversión." | T2 |
| S2 | "En cada fila de video (file_row en la lista de archivos seleccionados), si el video soporta opciones de pistas/configuración, agregar un botón claro (Editar o botón con icono de opciones/edición)." | T3 |
| S3 | "Al hacer clic en ese botón, abrir un diálogo/ventana modal limpio, simple, bien diseñado e integrado en GTK4 / Libadwaita […] exclusivo para las opciones de ESE video en particular." | T4 |
| S4 | "En esa ventana, el usuario debe poder seleccionar: Pista de audio deseada (si hay varias)." | T4 |
| S5 | "Switch de realce de voz (voice enhancement)." | T4 |
| S6 | "Pista de subtítulos." | T4 |
| S7 | "En las opciones de subtítulos DEBE incluirse una opción clara y explícita para NO usar subtítulos (Sin subtítulos / SubtitleMode::None). Si el audio ya está en el idioma del usuario, no hay por qué forzar subtítulos." | T4 |
| S8 | "La ventana tiene que ser sencilla, intuitiva, limpia y siguiendo las pautas de diseño de Libadwaita." | T4 |
| S9 | Commits atómicos en `main`, Conventional Commits, "SIN Co-Authored-By ni menciones a IA". | Todas |
| S10 | "nos mantenemos en v0.2.0, nada de bump a 0.2.1" — se reemplaza el artefacto publicado. | T7 |
| S11 | "pruebas bajo xvfb-run" y "suite completa de pruebas" antes de empaquetar. | T6, T7 |
| S12 | "Dejar el árbol limpio en main." | T7 |

### Decisión sobre `SubtitleMode::None`

El pedido nombra `SubtitleMode::None`. El enum persistido ya tiene `Strip`, que significa exactamente eso y está en `docs/contracts.md` §2.2, en el payload IPC y en los planes guardados. Agregar un `None` equivalente crearía dos valores para el mismo estado y rompería el contrato para ganar nada.

**La opción de UI se llama "Sin subtítulos" y escribe `SubtitleMode::Strip`.** Lo que se implementa es la intención del pedido —una opción explícita y visible de no usar subtítulos, primera en la lista— no un alias de contrato. Queda registrado en el `## Log` (L2) para que la diferencia no parezca un olvido.

---

## Reglas de ejecución

1. **TDD.** Cada tarea abre con tests que fallan; la evidencia RED se anota en `## Log`.
2. **Sin tocar el contrato.** `SubtitleMode`, `TrackSelection` y `ConversionPlan` no cambian. Si un test necesitara un campo nuevo, se frena y se registra.
3. **Un commit atómico por tarea**, ≤400 líneas, `tipo(scope):`, sin `Co-Authored-By`.
4. **Verificación sin GUI real** cuando sea posible: los tests de vista corren con GTK bajo `xvfb-run`, igual que el resto de `convert_view`.
5. **La versión no se toca.** Cero cambios en `Cargo.toml`, metainfo o `CHANGELOG` por esta remediación salvo la nota de reemplazo.

---

## Tareas

| ID | Título | Specs | Commit previsto |
|---|---|---|---|
| T1 | Selección por ruta en el estado: `selection_for(path)` y cola por archivo | S1, S3 | `refactor(gui): address the convert selection by path` |
| T2 | Quitar el bloque global de pistas de la página | S1 | `fix(gui): drop the global track block from the convert page` |
| T3 | Botón de edición por fila con su callback | S2 | `feat(gui): add a per-file options button to the convert rows` |
| T4 | Diálogo modal de pistas para un archivo | S3–S8 | `feat(gui): edit one file's tracks in a modal dialog` |
| T5 | Claves i18n en ES/EN/PT-BR | S8 | `feat(i18n): translate the track dialog` |
| T6 | Pruebas de vista bajo xvfb | S11 | `test(gui): cover the per-file track dialog` |
| T7 | Paquete, host y reemplazo del artefacto | S10–S12 | `build(release): repackage v0.2.0 with the track dialog` |

---

## Detalle de tareas

### T1 · Selección por ruta

- **RED:** `selection_for("/b.mkv")` no existe; `take_queue_request` arma la cola con la selección del primer archivo para todos.
- **GREEN:** `selection_for(path) -> Option<&FileSelection>` y `selection_for_mut(path)`; `take_queue_request` recorre los aceptados y usa **la selección de cada ruta**.
- **Aceptación:** dos archivos con selecciones distintas producen dos requests con `track_selection` distintos; editar B no cambia A.

### T2 · Quitar el bloque global

- **RED:** un test de vista comprueba que el bloque `tracks-group` ya no existe en la página; hoy existe y está visible con archivos.
- **GREEN:** se eliminan `tracks_group`, `audio_row`, `voice_switch`, `subtitle_row` del cuerpo de la página y su cableado en `flows.rs`.
- **Aceptación:** la página no expone ningún control de pistas fuera de las filas; las filas siguen mostrando nombre, duración, resolución y códec.

### T3 · Botón por fila

- **RED:** `file_row` no tiene botón de opciones; el test busca un widget con nombre `file-options` y no lo encuentra.
- **GREEN:** `file_row` agrega un `gtk::Button` con icono `document-edit-symbolic`, nombre de widget `file-options` y tooltip traducido; recibe un callback con la ruta.
- **Aceptación:** cada fila tiene su botón; el callback recibe la ruta de **esa** fila (test con dos filas y dos rutas distintas); una fila sin pistas alternativas sigue mostrando el botón pero deshabilitado, con el motivo en el tooltip.

### T4 · Diálogo modal

- **RED:** no existe módulo; el test no puede abrir nada.
- **GREEN:** `pages/convert/track_dialog.rs` con `present(parent, snapshot, selection, on_change)`, construido con `adw::Dialog` + `adw::ToolbarView` + `adw::PreferencesPage`:
  - `adw::ComboRow` **Pista de audio**, visible solo si hay más de una, con la misma etiqueta de idioma/códec/canales/bitrate que el resto de la app.
  - `adw::ActionRow` + `gtk::Switch` **Realce de voz**, deshabilitado con subtítulo explicativo si la pista elegida tiene menos de dos canales.
  - `adw::ComboRow` **Subtítulos**, cuya **primera entrada es "Sin subtítulos"** (`Strip`), seguida de cada pista real.
  - Cabecera con título del archivo y botón **Cerrar**.
- **Aceptación:** el diálogo es propiedad de un solo archivo; cambiar el audio actualiza el estado de esa ruta y no de otra; elegir "Sin subtítulos" deja `subtitle_mode: strip` en la selección; el switch refleja el `AudioMixMode` actual y lo escribe de vuelta.

### T5 · i18n

- **RED:** el test de paridad de claves falla porque las claves nuevas no existen en los tres catálogos.
- **GREEN:** claves `convert.track_dialog_title`, `convert.track_audio`, `convert.track_voice`, `convert.track_subtitles`, `convert.track_none`, `convert.track_close`, `convert.track_edit`, `convert.track_unavailable` en ES/EN/PT-BR.
- **Aceptación:** paridad exacta de conjuntos; español neutro, sin voseo en la UI.

### T6 · Pruebas de vista

- **RED/GREEN:** los casos de T2–T5 corren bajo `xvfb-run` con el mismo `on_gtk_thread` que ya usa `convert_view.rs`.
- **Aceptación:** la suite de `cauce-gui` queda verde y `CAUCE_SMOKE=1 xvfb-run -a cargo run -p cauce-gui` imprime `window-ready`.

### T7 · Paquete y artefacto

- `cargo build --release --workspace`, `cargo deb -p cauce-daemon --no-build -o dist/`, `packaging/check-package.sh`, `sudo dpkg -i dist/cauce_0.2.0-1_amd64.deb`, smoke en vivo, y `gh release upload --clobber v0.2.0 …`.
- **Aceptación:** `dpkg -s cauce` sigue diciendo `0.2.0-1`; el asset del release tiene el hash nuevo; el árbol queda limpio.

---

## Criterios de aceptación verificables

1. La página Convertir **no** contiene un bloque de pistas fuera de las filas.
2. Cada fila tiene un botón de opciones que abre un diálogo con el nombre del archivo en el título.
3. El diálogo permite elegir audio, realce de voz y subtítulos, incluida la opción explícita **Sin subtítulos**.
4. Con dos archivos cargados, cambiar el audio en uno **no** modifica el otro, verificado por test.
5. Los tres catálogos tienen el mismo conjunto de claves.
6. `cargo test --workspace`, `clippy -D warnings` y `fmt --check` en verde; `window-ready` bajo Xvfb.
7. El `.deb` reinstalado en el host responde `daemon_version 0.2.0` y el asset publicado coincide con el construido.

---

## Log

**L1** — Pedido del usuario (literal, 2026-11-25):

> "Che, tenemos que remediar un problema de UX en la página de conversión de Cauce GUI sin crear una nueva release (nos mantenemos en v0.2.0, nada de bump a 0.2.1). El problema detectado: Al seleccionar varios videos para convertir, las opciones de pistas de audio y subtítulos aparecen abajo como un bloque global (tracks_group). Eso confunde al usuario porque parece que aplicaran a todos los videos seleccionados, cuando en realidad cada video tiene sus propios tracks (y en el código se tomaba solo accepted.first()). Además, algunos videos tienen múltiples audios/subtítulos y otros no. Lo que necesitamos implementar: 1. Eliminar ese tracks_group estático global del fondo de la página de conversión. 2. En cada fila de video (file_row en la lista de archivos seleccionados), si el video soporta opciones de pistas/configuración, agregar un botón claro (Editar o botón con icono de opciones/edición). 3. Al hacer clic en ese botón, abrir un diálogo/ventana modal limpio, simple, bien diseñado e integrado en GTK4 / Libadwaita (usando por ejemplo adw::PreferencesDialog o adw::Window) exclusivo para las opciones de ESE video en particular. 4. En esa ventana, el usuario debe poder seleccionar: Pista de audio deseada (si hay varias). Switch de realce de voz (voice enhancement). Pista de subtítulos. 5. REQUISITO CRÍTICO DE SUBTÍTULOS: En las opciones de subtítulos DEBE incluirse una opción clara y explícita para NO usar subtítulos (Sin subtítulos / SubtitleMode::None). Si el audio ya está en el idioma del usuario, no hay por qué forzar subtítulos. 6. La ventana tiene que ser sencilla, intuitiva, limpia y siguiendo las pautas de diseño de Libadwaita. […] 9. Una vez implementado y testeado con la suite completa de pruebas: Compilar en release […] Empaquetar el .deb […] Probar e instalar en el host local […] Actualizar el release asset en GitHub reemplazando el .deb existente […] Dejar el árbol limpio en main."

**L2** — `SubtitleMode::None` se implementa como la opción de UI **"Sin subtítulos"** escribiendo `SubtitleMode::Strip`, que ya significa eso en el contrato persistido. Agregar una segunda variante equivalente duplicaría el estado y rompería `docs/contracts.md` §2.2 sin beneficio. La intención del pedido —una opción explícita, visible y primera en la lista— se cumple; el nombre del enum no cambia.

**L3** — **Hallazgo que cambia el alcance de T1.** `jobs.add` acepta `inputs: [...]` con **una sola** `track_selection` para todo el lote (`docs/contracts.md` §6). Con archivos de pistas distintas, ese contrato no puede expresar la selección por archivo que pide el usuario. La página hoy lo esquiva mandando la selección del primero para todos, que es justo el defecto reportado.
Dos caminos: extender el protocolo a selecciones por entrada, o encolar **una llamada por archivo**. Se elige lo segundo: el wire no cambia, la semántica queda correcta —una selección pertenece a un archivo— y no se toca un contrato publicado en una release que ya está en producción y que por decisión no se versiona.
Consecuencia que se asume y se documenta: la cola pasa a hacer N llamadas en vez de una, y un fallo a mitad del lote deja los anteriores encolados. La UI informa cuántos entraron y cuántos fallaron, en vez de fingir atomicidad que el protocolo nunca tuvo.
