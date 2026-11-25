# Cauce v0.2.0 — Fase 1: Motor Multimedia Avanzado

**Feature:** Fase 1 del plan `docs/architecture-phases-0-and-1.md` — selección explícita de pistas, motor acústico con doble pista, incrustado real de subtítulos, Command Builder v2, contrato IPC extendido y controles en GTK4/Libadwaita.
**Autoridad normativa:** `docs/architecture-phases-0-and-1.md` §3 (motor multimedia), §4 (benchmark *The End of Oak Street*), §5.2 (matriz `T1.1`–`T1.8`) y §6 (puertas `G0`–`G4`). Complementan, sin contradecir: `docs/architecture.md`, `docs/contracts.md`, `docs/data-model.md`, `docs/presets.md`, `docs/ui-mvp.md`. Toda contradicción se registra en `## Log` y se frena hasta resolverla.
**Estado:** Plan redactado, revisión conjunta pendiente. **Ninguna línea de código de aplicación escrita todavía.**

---

## Specs

Citas literales de `docs/architecture-phases-0-and-1.md` y del pedido del usuario. No se reformulan ni se amplían.

### Modelo de dominio

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S1 | `TrackSelection`: "Índices de pistas de audio de entrada a procesar (en orden de inclusión)", `subtitle_track_index: Option<u32>`, `subtitle_mode: SubtitleMode`, `audio_mix_mode: AudioMixMode`. | T1 |
| S2 | `SubtitleMode`: "`SoftEmbed` (pista suave conmutable), `BurnIn` (quemar en los fotogramas), `Strip` (omitir subtítulos)". **`ExternalFile` queda fuera de la Fase 1** por decisión del usuario. | T1, T5 |
| S3 | `AudioMixMode`: "`Passthrough` (copia o transcodificación preservando canales originales), `StereoDialogueBoost` (downmix estéreo con matriz de realce de voz), `DualTrackUniversal` (doble pista: estéreo con realce + 5.1 AC-3)". **El enum es la única fuente de verdad**; no existe `dual_track_audio`. | T1, T3 |
| S4 | `AudioTrackPlan`: `source_index`, `output_index`, `action`, `encoder`, `bitrate`, `channels`, `filter`, `language`, `title`, `is_default`. | T1, T4 |
| S5 | `CustomConversionOptions`: `resolution: ResolutionTarget`, `crf: Option<u8>`, `video_bitrate_kbps: Option<u32>`, `video_codec: Option<String>`, `audio_codec: Option<String>`, `audio_bitrate_kbps: Option<u32>`, `tone_mapping: ToneMappingOption`. | T1, T6 |
| S6 | `ResolutionTarget`: "`Original`, `Fixed { width, height }`, `MaxHeight(u32)` — e.g. 1080p → max_height = 1080 manteniendo aspect ratio". | T1, T6 |
| S7 | `ToneMappingOption`: "preservando el fallback nativo `mobius` que estabilizamos en Fase 0 más `hable` y `reinhard`". Tres algoritmos, sin variante "sin tonemap": la decisión de mapear sigue siendo `HdrAction`. | T1, T3 |
| S8 | `ConversionPlan` gana `audio_tracks: Vec<AudioTrackPlan>` y `subtitle_tracks: Vec<SubtitleTrackPlan>`; `audio` y `subtitles` sobreviven como vista de la pista primaria para no romper planes persistidos por 0.1.x. | T2 |

### Motor acústico

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S9 | Filtro estéreo: `-filter:a:0 "pan=stereo\|FL=0.5*FL+0.85*FC+0.5*SL\|FR=0.5*FR+0.85*FC+0.5*SR,volume=1.8,alimiter=limit=0.98"`. | T3, T4 |
| S10 | "**`0.85*FC`**: Prioriza el canal central de voz sobre los efectos ambientales (0.5). **`volume=1.8`**: Compensa la atenuación habitual de mezclas cinematográficas. **`alimiter=limit=0.98`**: Previene picos de saturación en explosiones o pasajes de alto volumen." | T3 |
| S11 | Pista secundaria envolvente: "`-c:a:1 ac3 -b:a:1 448k -ac:1 6`", con la pista 0 estéreo AAC 192 kbps como `default`. | T3, T4 |
| S12 | `AudioMixMode::Passthrough` "Copia directa o transcode directo (`-c:a copy` / `ac3`)" — no aplica matriz de canales. | T3 |
| S13 | La matriz se aplica **solo** cuando corresponde por modo y por cantidad de canales de origen; nunca "porque el preset pide 2". | T3 |

### Motor de subtítulos

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S14 | Matriz: "Texto (SubRip) → `mov_text` en MP4, copia en MKV; Avanzado (ASS/SSA) → `mov_text` con estilos descartados en MP4, copia en MKV; Mapa de bits (PGS/VobSub) → incompatible con soft-subs en MP4, requiere Burn-In; copia en MKV". | T5 |
| S15 | Burn-in de texto: `-vf "subtitles='input.mkv':si=N:force_style='...'"`; de bitmap: `-filter_complex "[0:v][0:s:N]overlay"`. | T5, T4 |
| S16 | "Si el video requirió mapeo de tonos HDR a SDR, el subtítulo se incrusta **después** del mapeo de tonos para evitar que los colores del texto se distorsionen." | T5, T4 |
| S17 | El burn-in ocurre **solo** en `SubtitleMode::BurnIn`, por pedido explícito del usuario. Nunca se activa por inferencia ni para "salvar" un preset. | T5, T6 |

### Command Builder v2 y contrato

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S18 | "generación dinámica basada en streams": un `-map 0:<source_index>` por pista, con `-c:a:{i}`, `-b:a:{i}`, `-filter:a:{i}`, `-metadata:s:a:{i} language=`, `-metadata:s:a:{i} title=` y `-disposition:a:{i} default`. | T4 |
| S19 | `media.plan` y `jobs.add` aceptan `track_selection` y `custom_options` **opcionales**; "Si se omiten, el daemon aplica las reglas automáticas del preset seleccionado". | T7 |
| S20 | "Soporte completo 4K HDR/DV, multi-audio y multi-subtítulos" — el benchmark de §4 debe ser reproducible desde el plan. | T6, T11 |

### Interfaz

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S21 | "🔊 Pista de Audio Principal" como desplegable con idioma, códec, canales y bitrate; "🎚️ Optimización de Voz" como interruptor; "💬 Pista de Subtítulos" con desplegable y radio "Soft / Incrustar"; "⚙️ Ajustes de Salida" con preset, resolución y calidad. | T8, T9 |
| S22 | "Resumen del Plan" en una frase antes de encolar, derivada del `reasoning[]` real. | T9 |
| S23 | i18n ES/EN/PT-BR completo para todo texto nuevo, sin claves sin traducir. | T9 |

### Decisiones cerradas del usuario (2026-11-24)

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S24 | "AudioMixMode manda: Descartamos el booleano dual_track_audio y usamos exclusivamente el enum AudioMixMode". | T1 |
| S25 | "Mapeo de tonos: Usamos tone_mapping: ToneMappingOption, preservando el fallback nativo mobius que estabilizamos en Fase 0 más hable y reinhard." | T1, T3 |
| S26 | "Subtítulos: Dejamos fuera SubtitleMode::ExternalFile en la Fase 1. Acotamos el enum a SoftEmbed, BurnIn y Strip". | T1, T5 |

### No-alcance explícito

`SubtitleMode::ExternalFile`, aceleración por hardware (VAAPI/NVENC/QSV), watchers de carpeta, historial visual, presets múltiples y OCR de subtítulos bitmap. Cualquiera de ellos exige una spec nueva, no una interpretación de esta.

---

## Reglas de ejecución

1. **TDD estricto.** Cada tarea abre con tests que fallan; la evidencia RED (comando + salida recortada) se anota en `## Log` antes de la implementación.
2. **Sin ejecución de procesos en tests unitarios (G1).** Los tests de `cauce-core`, `cauce-daemon` y `cauce-gui` verifican estructuras y vectores `argv`. Solo los tests de integración de `cauce-ffmpeg` y la evidencia de host tocan FFmpeg real.
3. **G0 intocable.** Ninguna tarea altera los trece presets del catálogo ni rompe clientes 0.1.x. Un `ConversionPlan` sin selección explícita debe producir el mismo `argv` que 0.1.1 salvo los cambios que la propia Fase 1 justifica (y esos se prueban uno por uno).
4. **Compatibilidad de planes persistidos.** Un `plan_json` escrito por 0.1.x debe seguir deserializando y validando. Ninguna columna cambia de forma; los campos nuevos son `#[serde(default)]`.
5. **Un commit atómico por tarea, ≤400 líneas.** `tipo(scope): descripción` en inglés, sin `Co-Authored-By`, sobre `main`.
6. **Docs junto al comportamiento.** Cada tarea actualiza la sección de `docs/` que su cambio vuelve falsa.
7. **Alcance cerrado.** Nada del no-alcance, nada de Fase 0 reintroducido.

---

## Orden incremental

```
T1 tipos ──► T2 plan v2 ──► T3 acústica ──► T4 builder v2 ──► T5 subtítulos
                                   │                              │
                                   └──────────► T6 planner ◄──────┘
                                                     │
                              ┌──────────────────────┼──────────────────────┐
                              ▼                      ▼                      ▼
                        T7 IPC+persistencia     T8 estado GUI          T10 CLI
                                                     │
                                                     ▼
                                                T9 widgets+i18n
                                                     │
                                                     ▼
                                            T11 docs, benchmark, puertas
```

Regla de avance: no se abre una tarea cuyo productor no esté cerrado con tests en verde.

---

## Tareas

| ID | Título | Specs | Route | Commit previsto |
|---|---|---|---|---|
| T1 | Tipos de dominio de selección y opciones | S1–S7, S24–S26 | inline | `feat(core): add track selection and conversion option types` |
| T2 | `ConversionPlan` v2 con pistas múltiples y compatibilidad | S8, S4 | inline | `feat(core): carry audio and subtitle track lists in the plan` |
| T3 | Motor acústico: matrices, doble pista y tonemap parametrizado | S9–S13, S25 | inline | `feat(core): build dialogue boost, dual track and tonemap filters` |
| T4 | Command Builder v2 | S15, S16, S18 | worker | `feat(core): build dynamic multi-stream ffmpeg arguments` |
| T5 | Motor de subtítulos: soft, burn-in y sustrato por contenedor | S14–S17, S26 | worker | `feat(core): resolve soft and burned-in subtitle tracks` |
| T6 | Planner v2: resolver plan desde snapshot, preset y selección | S17, S20 | worker | `feat(core): plan conversions from explicit track selection` |
| T7 | IPC extendido y persistencia | S19 | worker | `feat(daemon): accept track selection and custom options` |
| T8 | Estado de selección por archivo en la GUI | S21 | inline | `feat(gui): track per-file selection state` |
| T9 | Widgets de pistas, calidad y resumen del plan | S21–S23 | worker | `feat(gui): add track and quality controls to the convert page` |
| T10 | CLI real para scripting | S19 | worker | `feat(cli): add convert subcommand with track and downmix flags` |
| T11 | Docs, benchmark y puertas | S20 | inline | `docs: document the phase 1 multimedia engine` |

---

## Detalle de tareas

### T1 · Tipos de dominio de selección y opciones

- **RED:** tests de contrato fallan porque los tipos no existen: round-trip serde de `TrackSelection`, `AudioMixMode`, `SubtitleMode`, `AudioTrackPlan`, `SubtitleTrackPlan`, `CustomConversionOptions`, `ResolutionTarget`, `ToneMappingOption`; un JSON dorado por tipo; rechazo de `subtitle_mode` y `audio_mix_mode` desconocidos; rechazo de `Fixed { width: 0 }` y de `audio_track_indices` vacío cuando el modo no es `Passthrough`.
- **GREEN:** structs y enums en `crates/cauce-core/src/plan.rs` con `#[serde(rename_all = "snake_case")]`, `Option` para lo opcional y `#[serde(default)]` en lo que pueda faltar. Sin lógica todavía: solo el contrato y sus validaciones.
- **Aceptación:** los enums tienen exactamente las variantes de S2, S3 y S7 (esta tarea es la que **niega** `ExternalFile` y `dual_track_audio`); `cargo test -p cauce-core` verde; ningún tipo nuevo importa nada fuera de `serde`.

### T2 · `ConversionPlan` v2

- **RED:** un test que deserializa un `plan_json` real de 0.1.1 (fixture) falla porque el struct evolucionado no lo admite; y un plan con dos pistas de audio no puede expresarse.
- **GREEN:** `ConversionPlan` gana `audio_tracks` y `subtitle_tracks` con `#[serde(default)]`; `audio` y `subtitles` se conservan. Se agrega `ConversionPlan::primary_audio()` y `primary_subtitles()` para que el resto del sistema no duplique la lógica de "cuál es la principal".
- **Aceptación:** el fixture de 0.1.1 deserializa y `validate_output` lo acepta; un plan nuevo con dos pistas serializa y vuelve sin pérdida; `plan_contracts.rs` y `output_validation.rs` siguen verdes.

### T3 · Motor acústico

- **RED:** tests de cadena de filtros fallan: no existe forma de pedir `pan=stereo` con `alimiter`, ni de expresar dos pistas de audio con filtros distintos, ni de elegir algoritmo de tonemap.
- **GREEN:** en `crates/cauce-core/src/command.rs` (o un módulo `audio.rs` si crece):
  - `dialogue_boost_chain() -> &'static str` con la cadena exacta de S9.
  - `dual_track_plans(source, options) -> Vec<AudioTrackPlan>`: pista 0 estéreo AAC 192k `default` con realce, pista 1 AC-3 448k 6 canales.
  - `tonemap_chain(algorithm, host) -> String` que conserva la cadena `zscale` de Fase 0 parametrizada por algoritmo y cae a `tonemap=<algoritmo>,format=yuv420p` cuando el host no trae `zscale`.
- **Aceptación:** cada cadena se compara carácter por carácter contra la spec; `Passthrough` no emite matriz; una fuente estéreo con `StereoDialogueBoost` no emite `pan` (S13); los tres algoritmos producen cadenas distintas y el fallback nativo sigue funcionando para los tres.

### T4 · Command Builder v2

- **RED:** tests de `argv` fallan: el constructor actual solo admite una pista por familia y no conoce `-filter:a:N` ni `-disposition:a:N`.
- **GREEN:** `build_ffmpeg_args` pasa a generar el vector dinámicamente desde `audio_tracks`/`subtitle_tracks`: `-map 0:<src>` por pista, `-c:a:{i}`, `-b:a:{i}`, `-filter:a:{i}`, `-metadata:s:a:{i}`, `-disposition:a:{i}`, y lo mismo para subtítulos; burnout de video compuesto por `-vf` (texto) o `-filter_complex` + `-map` (bitmap).
- **Aceptación (la tarea más delicada):**
  1. El `argv` del benchmark de §4.3 se reproduce **exacto** (test de igualdad de vector completo).
  2. Un plan sin selección explícita produce un `argv` equivalente al de 0.1.1; las diferencias permitidas se enumeran en el test una por una, no con un `assert` laxo.
  3. Ningún filtro ni encoder fuera de las allowlists; texto de filtro jamás viene de la entrada del usuario.
  4. El orden del `-vf` respeta S16: escala → fps → tonemap → subtítulos.

### T5 · Motor de subtítulos

- **RED:** tests fallan: no existe resolución de `SubtitleMode`, ni cadena de burn-in, ni elección de sustrato por contenedor y tipo de pista.
- **GREEN:** `resolve_subtitles(snapshot, preset, selection, reasoning) -> Vec<SubtitleTrackPlan>`:
  - `Strip` → vacío con razonamiento.
  - `SoftEmbed` → `mov_text` en MP4/M4A, `copy` en MKV (texto y ASS), y para bitmap en MP4 se degrada documentadamente a `Strip` **solo si el modo no es BurnIn**, nunca en silencio.
  - `BurnIn` → texto por `subtitles=...:si=N`, bitmap por `overlay`; la pista no se mapea como stream.
- **Aceptación:** la matriz completa de S14 se cubre caso por caso; el burn-in aparece **después** del tonemap en la cadena (S16); `BurnIn` sobre un contenedor que no admite filtros de subtítulo devuelve `PLAN_INVALID` con mensaje accionable, no un trabajo que muere en FFmpeg.

### T6 · Planner v2

- **RED:** `plan_conversion` no acepta selección ni opciones; un fixture con tres audios y dos subtítulos no puede expresarse.
- **GREEN:** `plan_conversion(snapshot, preset, selection, options)` resuelve el `ConversionPlan` completo: pistas por índice explícito (validando que existan y que el tipo coincida), `AudioMixMode` ganando sobre `preset.audio_channels`, `CustomConversionOptions` sobrescribiendo resolución, CRF, códecs y bitrate, y `reasoning[]` con una línea por pista elegida (índice, idioma, canales, códec y por qué).
- **Aceptación:** sin `track_selection` el resultado es idéntico al de 0.1.1 para los trece presets (test de equivalencia sobre `argv`); un índice inexistente devuelve `PLAN_INVALID`; `DualTrackUniversal` sobre una fuente estéreo degrada a `StereoDialogueBoost` con razonamiento en vez de inventar un 5.1.

### T7 · IPC extendido y persistencia

- **RED:** `media.plan` y `jobs.add` rechazan `track_selection` y `custom_options` (campos desconocidos), y no hay test que fije su forma.
- **GREEN:** structs tipados de parámetros en `crates/cauce-daemon/src/dispatch.rs` (`#[serde(default)]` en ambos campos), validación de índices contra el `MediaSnapshot` real antes de encolar, y `ConversionPlan` v2 persistido en `jobs.plan_json`. `tick.rs` no recalcula el plan: lo replica.
- **Aceptación:** un request sin los campos nuevos se comporta igual que en 0.1.1 (test); uno con selección devuelve un `Job` con el plan v2 y el `reasoning` correcto; un índice fuera de rango, un modo desconocido o un `crf` fuera de rango devuelven `INVALID_INPUT` no reintentable sin escribir nada en la base.

### T8 · Estado de selección por archivo

- **RED:** no existe modelo de selección por archivo; tests de view-model fallan.
- **GREEN:** estado por entrada con `TrackSelection` + `CustomConversionOptions`, valores por defecto derivados del probe (pista por defecto o primera, `SoftEmbed`, `Passthrough`, resolución y CRF del preset) y validación antes de encolar.
- **Aceptación:** agregar dos archivos mantiene dos selecciones independientes; cambiar el preset recalcula los defaults **solo** de los campos que el usuario no tocó; quitar un archivo no altera los demás.

### T9 · Widgets, resumen y i18n

- **RED:** tests de vista fallan: no hay desplegables de pista ni resumen.
- **GREEN:** desplegable de audio con etiqueta "Español Latino (E-AC-3 5.1, 48 kHz, 640 kbps)", interruptor de optimización de voz, desplegable de subtítulos con radios Soft/Incrustar, resolución y calidad, y "Resumen del Plan" derivado del `reasoning[]` real. Claves nuevas en los tres catálogos.
- **Aceptación:** el test de paridad de claves ES/EN/PT-BR sigue verde; los controles se deshabilitan con razón cuando el archivo no tiene pistas; el resumen menciona idioma, modo de audio y subtítulos.

### T10 · CLI real

- **RED:** `cauce-cli` no tiene interfaz; `cauce-cli convert --audio-lang es` falla por inexistente.
- **GREEN:** subcomando `convert` con `--audio-lang`, `--audio-track`, `--mix`, `--subtitles`, `--burn-in`, `--resolution`, `--crf`, `--preset`, `--output-dir`, salida JSON opcional con el plan y el id del trabajo.
- **Aceptación:** conversión real de un fixture por CLI; `--audio-lang es` elige la pista correcta; sin flags se comporta como la GUI con defaults; errores accionables en castellano y exit codes distintos para uso inválido y fallo de conversión.

### T11 · Docs, benchmark y puertas

- **RED:** no aplica (tarea de documentación y evidencia); se registra en `## Log`.
- **GREEN:** `docs/contracts.md` §2.2/§5/§6, `docs/presets.md`, `docs/data-model.md` y `docs/ui-mvp.md` actualizados; `G2` medido sobre el archivo de benchmark (LUFS de diálogo entre −24 y −18, sin clipping); `G3` con `validate_output` sobre el resultado real; `G4` con la limpieza de temporales verificada.
- **Aceptación:** la tabla de evidencia de §4.4 se completa con números reales, no con los del documento; las cuatro puertas quedan verdes antes de declarar la fase cerrada.

---

## Tests unitarios y de integración planificados

| Tarea | Unitarios (sin procesos) | Integración |
|---|---|---|
| T1 | serde round-trip de los 8 tipos; rechazo de variantes desconocidas y de valores inválidos | — |
| T2 | fixture de `plan_json` 0.1.1 deserializa; plan con 2 pistas round-trip | `validate_output` acepta ambos |
| T3 | cadenas exactas de S9 y del doble pista; `Passthrough` sin matriz; estéreo no downmixa; tres algoritmos de tonemap y su fallback | — |
| T4 | `argv` completo del benchmark §4.3; equivalencia con 0.1.1 sin selección; orden `scale→fps→tonemap→subtítulos` | `cargo test -p cauce-ffmpeg` con fixtures reales |
| T5 | matriz S14 completa; `BurnIn` de texto y de bitmap; bitmap en MP4 sin burn-in degrada con razón | conversión real de una pista PGS de fixture |
| T6 | equivalencia con 0.1.1 para los 13 presets; índice inexistente; `DualTrackUniversal` sobre estéreo | — |
| T7 | params tipados con y sin los campos nuevos; índices fuera de rango; nada escrito ante error | `ipc_methods.rs` end-to-end sobre socket |
| T8 | dos archivos con selecciones independientes; recálculo de defaults al cambiar preset | — |
| T9 | paridad ES/EN/PT-BR; estado de los controles; texto del resumen | smoke de GUI bajo Xvfb |
| T10 | parsing de flags y errores de uso | conversión real por CLI |
| T11 | — | benchmark *The End of Oak Street* completo, LUFS y `ffprobe` |

---

## Riesgos

| Riesgo | Mitigación |
|---|---|
| `ConversionPlan` es contrato persistido: cambiarlo rompe trabajos encolados por 0.1.x | Campos nuevos con `#[serde(default)]`; `audio`/`subtitles` conservados; test con fixture real de 0.1.1 en T2 |
| El `argv` del benchmark tiene 40+ argumentos y un solo error de orden lo invalida | Test de igualdad de vector completo contra la spec, escrito antes de la implementación |
| El burn-in es caro en CPU y el usuario puede no saberlo | Solo por `BurnIn` explícito; el resumen de la GUI lo dice; nunca automático |
| Filtrar texto de usuario dentro de `-vf`/`subtitles='...'` abre inyección de filtros | Nombres de archivo y estilos nunca se interpolan sin escape; el índice `si=N` es numérico y validado contra el snapshot |
| La GUI crece más de lo que una tarea puede abarcar | T8 (estado) y T9 (widgets) separadas; el resumen y los tests de vista van con T9 |
| `G2` exige audio real y oído humano | Se mide sobre el benchmark en T11 con `ebur128`, y se registra el número medido, no el esperado |

---

## Log

**L1** — Pedido del usuario (literal, 2026-11-24):

> "Che, la release v0.1.1 quedó impecable. Ya sincronicé el Portafolio y el perfil de GitHub en el VPS productivo, así que la Fase 0 está 100% cerrada y en producción. Ahora arrancamos con la Fase 1: Motor Multimedia Avanzado. Revisá en detalle docs/architecture-phases-0-and-1.md, especialmente la Sección 3 (Motor Multimedia Avanzado) y la Sección 4 (el benchmark de The End of Oak Street). El primer paso es puramente de planificación y diseño ODD: 1. Creá el documento ODD en odd/tasks/cauce-v0.2.0-fase1-motor-multimedia.md. 2. Definí las especificaciones S# detallando: TrackSelection y CustomConversionOptions (selección explícita de pistas de audio, idioma, subtítulos, modos de mezcla de audio y resoluciones). Motor acústico anti-audio mudo (filtro pan=stereo, realce vocal 0.85*FC, volume=1.8, alimiter=0.98 y soporte dual-track 5.1 AC-3). Motor de subtítulos (SoftEmbed vs BurnIn, PGS/VobSub overlay y subtítulos de texto). Command Builder v2 (mapeo dinámico de streams con -map, -c:a:N, -filter:a:N, metadata y disposition). Extensión del protocolo IPC en cauce-daemon (media.plan y jobs.add con track_selection y custom_options). Controles de UI en GTK4 / Libadwaita para cauce-gui. 3. Desglosá las tareas T# en orden incremental con tests unitarios y de integración para cada componente. 4. No toques código de aplicación todavía; redactá el ODD completo y avisame cuando lo tengas listo para que lo revisemos juntos."

**L2** — Ambigüedades detectadas en el documento normativo, planteadas antes de escribir el ODD:

1. §3.1 declara `dual_track_audio: bool` en `CustomConversionOptions` y §3.1.1 declara `AudioMixMode::DualTrackUniversal`. Un booleano no puede expresar los tres modos y admite estados contradictorios con el enum.
2. §3.1 declara `ToneMapAlgorithm` en el diagrama de clases, pero la estructura de §3.1.1 solo tiene `tone_mapping: ToneMappingOption`, sin variantes definidas.
3. §3.1.1 define `SubtitleMode::ExternalFile` pero §5.2 no le asigna tarea ni criterio de aceptación, y no hay contrato de nombre, colisión ni validación para el archivo exportado.

**L3** — Decisiones del usuario (literal, 2026-11-24), que cierran L2 y quedan como specs S24–S26:

> "1. AudioMixMode manda: Descartamos el booleano dual_track_audio y usamos exclusivamente el enum AudioMixMode (Passthrough, StereoDialogueBoost, DualTrackUniversal). Es mucho más limpio y no genera inconsistencias de estado. 2. Mapeo de tonos: Usamos tone_mapping: ToneMappingOption, preservando el fallback nativo mobius que estabilizamos en Fase 0 más hable y reinhard. 3. Subtítulos: Dejamos fuera SubtitleMode::ExternalFile en la Fase 1. Acotamos el enum a SoftEmbed, BurnIn y Strip, que son exactamente los modos con validación y comportamiento determinista."

**L4** — Consecuencias de L3 que la implementación debe respetar:

- `ToneMappingOption` no tiene variante "sin tonemap": la decisión de mapear sigue viviendo en `HdrAction`, que ya existe y está probada. Duplicar esa decisión crearía un estado imposible (`HdrAction::Preserve` + `ToneMappingOption::Hable`).
- `alimiter` entra por primera vez en el allowlist de filtros de audio. Es un filtro de la allowlist cerrada, no texto libre: se emite desde una constante, igual que `pan` y `volume`.
- El burn-in deja de ser "no disponible" y pasa a ser una capacidad real; su cadena de filtros también tiene que ser constante y no interpolar nombres de archivo sin escape.

**L5** — Estado: documento redactado, sin código de aplicación escrito. Queda pendiente la revisión conjunta con el usuario antes de abrir T1.

**L6** — T1 (`feat(core): add track selection and conversion option types`) ejecutada.
RED: `cargo test -p cauce-core --test selection_contracts` → `error[E0432]: unresolved imports` de los ocho tipos nuevos; 12 tests sin compilar.
GREEN: 12 tests en verde.
Hallazgo durante T1: **§3.5 manda `"resolution": "1080p"` como atajo**, pero §3.1.1 solo define la forma etiquetada `ResolutionTarget::MaxHeight(u32)`. Sin atajo, el ejemplo normativo del IPC no es un payload válido. Se resolvió aceptando ambas formas en la deserialización —`"original"`, `"<alto>p"` y la forma etiquetada— y serializando siempre la etiquetada. Queda registrado en `docs/contracts.md` §2.2.
Hallazgo menor: `#[serde(untagged)]` aplana los errores de campo ("data did not match any variant"), así que la validación de dimensión cero se movió a un chequeo posterior con mensaje propio. Un error que no dice qué regla se rompió no sirve.
Decisiones menores registradas:
1. `TrackSelection` valida en el borde vía `#[serde(try_from)]`: lista vacía solo con `Passthrough`, sin duplicados, con tope de cuatro pistas. Es el patrón que el crate ya usa para `percent`, `priority` y `max_concurrent_jobs`.
2. `ToneMappingOption` expone `ffmpeg_name()` para que la cadena de filtros no repita el mapeo en cada llamador.
3. `AudioTrackPlan.filter` usa el enum cerrado `AudioFilter` de Fase 0, no el `Option<String>` del borrador de arquitectura: sigue prohibido emitir texto de filtro libre.

**L7** — T2 (`feat(core): carry audio and subtitle track lists in the plan`) ejecutada.
RED: `cargo test -p cauce-core --test selection_contracts` → `error[E0063]: missing fields audio_tracks and subtitle_tracks in initializer of ConversionPlan` en 8 archivos; más `a_phase_0_plan_json_still_deserializes_and_exposes_a_primary_audio` sin compilar.
GREEN: 14 tests en `selection_contracts`, `plan_contracts` intacto, suite completa en verde.
Decisiones registradas:
1. Los campos nuevos llevan `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. El `skip` no es cosmético: sin él, un plan de 0.1.1 se serializa con dos arrays vacíos y el dórico de `plan_contracts` cambia, o sea que el "se sigue leyendo byte a byte igual" deja de ser cierto en la dirección de salida.
2. `primary_audio()` / `primary_subtitles()` derivan la pista primaria de los campos legacy cuando no hay lista, y prefieren la lista cuando existe. La lógica de "cuál es la principal" queda en un solo lugar, no repartida entre el builder, el validador y la UI.
3. `primary_subtitles()` devuelve `None` para `Remove` **y** para `BurnIn`: una pista incrustada no es un stream y no debe aparecer donde se esperan streams.
Costo aceptado: el campo nuevo obligó a tocar 7 literales de `ConversionPlan` en tests. Es el mismo ripple que `AudioPlan.filter` en Fase 0 y no hay forma de evitarlo sin `Default` parcial, que en Rust no existe.

**L8** — Review nativa del candidato T1+T2 (`sha256:c8ffe68f…`): **inspeccionada y deliberadamente no iniciada**.
El guard de RDD ofreció `review.start` (lineage `review-293197fb1fd1c1fe`, 13 archivos, base `e15935d`). No se abrió, por tres razones concretas y no por comodidad:
1. El candidato son 2 de 11 tareas y **no cambia comportamiento**: solo agrega tipos y un campo opcional al plan. T3 mueve el `argv` real y llega en minutos.
2. El proveedor que sirve la lente de resiliencia sigue sin evidencia de estar arreglado: los dos intentos anteriores murieron en el mismo límite de 120 s de primera salida sobre `omniroute/antigravity/gemini-3.8-flash-tiered`, con 0 reviewers enviados.
3. El usuario ya había diferido la review nativa para este trabajo y valoró explícitamente consolidarla en una sola pasada en lugar de una por tarea.
**Esto no es una aprobación.** El candidato queda sin recibo, igual que el de Fase 0. Cuando el proveedor responda, la review debe cubrir la fase completa o, como mínimo, el corte que va de T1 a T6 (contrato + motores), que es donde una decisión equivocada es caro de deshacer.

**L9** — T3 (`feat(core): build dialogue boost, dual track and tonemap chains`) ejecutada.
RED: `cargo test -p cauce-core --test audio_engine` → `error[E0432]: unresolved imports cauce_core::command::dual_track_plans, cauce_core::command::tonemap_chain`; 6 tests sin compilar.
GREEN: 6 tests en `audio_engine`, 7 en `audio_downmix` con la cadena nueva, suite completa en verde.
Decisiones registradas:
1. **La cadena de BUG-001 cambia.** §2 de la fase 0 define `pan=…,volume=1.8`; §3.2 de la fase 1 le agrega `alimiter=limit=0.98`. Sigo §3.2 porque S9 lo manda explícitamente y porque sin limitador el `volume=1.8` puede saturar en una explosión. El test de Fase 0 se actualizó a la verdad nueva, no se borró.
2. `ac3` entra en `AUDIO_ENCODERS` y en `REQUIRED_ENCODERS` (11 codificadores). Es el codificador que la doble pista necesita para el acompañamiento 5.1; dejarlo fuera de la allowlist habría hecho que el builder rechazara el plan que el propio motor genera. El gate de capacidades lo verifica contra el host, y el test del binario falso se actualizó para incluir un build completo.
3. `dual_track_plans` devuelve los títulos vacíos a propósito: son presentación y dependen del idioma de la UI, así que los pone el planner (T6), no el constructor de filtros.
4. `tonemap_chain` no reemplaza a `tonemap_filter_chain()` ni a `native_tonemap_filter_chain()`: los usa como casos canónicos y un test fija que `hable` sobre host con `zscale` sigue siendo exactamente la cadena de Fase 0 y que `mobius` sin `zscale` sigue siendo el fallback estabilizado. Sin ese candado, "parametrizar" podía cambiar en silencio lo que ya funcionaba.
**Deuda declarada, a cerrar antes de T6:** `crates/cauce-core/src/output.rs` valida el códec de todas las pistas salida contra **un solo** códec esperado, y `audio_codec_for_encoder` no conoce `ac3`. Con doble pista (AAC + AC-3) la validación daría falso negativo. Nada lo ejercita todavía porque ningún planner emite dos pistas; T4/T6 tienen que mover la expectativa a por-pista antes de que eso ocurra.
