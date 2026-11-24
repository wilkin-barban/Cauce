# Cauce v0.2.0 — Fase 0: auditoría y remediación de bugs

**Feature:** Fase 0 del plan `docs/architecture-phases-0-and-1.md` — remediación quirúrgica de `BUG-001` a `BUG-006` en `cauce-core`, `cauce-ffmpeg`, `cauce-jobs` y `cauce-daemon`, sin tocar contratos públicos ni los 13 presets del sistema.
**Autoridad normativa:** `docs/architecture-phases-0-and-1.md` §2 (diagnóstico y remediación de cada bug), §5.1 (matriz `T0.1`–`T0.6`) y §6 (puertas `G0`–`G4`). Complementan, sin contradecir: `docs/architecture.md`, `docs/contracts.md`, `docs/presets.md`, `docs/data-model.md`. Si aparece una contradicción, se registra en `## Log` y se frena hasta resolverla.
**Estado:** Plan redactado. Implementación en curso, TDD, commits atómicos sobre `main`.

---

## Specs

Citas literales de `docs/architecture-phases-0-and-1.md` y del pedido del usuario. No se reformulan ni se amplían.

| ID | Requisito (literal) | Tareas |
|---|---|---|
| S1 | BUG-001: "Cuando la fuente tenga $\ge 6$ canales (5.1, 7.1) y el destino sea estéreo (2 canales), se debe aplicar una matriz de mezcla ponderada" con `-af "pan=stereo\|FL=0.5*FL+0.85*FC+0.5*SL\|FR=0.5*FR+0.85*FC+0.5*SR,volume=1.8"`. | T1 |
| S2 | BUG-001: "Habilitar cadena de filtros de audio (`audio_filter_chain`) […] Implementar en `command.rs` la función `audio_filter_chain(&AudioPlan) -> Option<String>` e inyectar el flag `-af` (o `-filter:a`)". | T1 |
| S3 | T0.1: "Pruebas unitarias verifican que `-af` se inyecta correctamente en transcodificación de audio." | T1 |
| S4 | BUG-002: "Prioridad 1: Coincidencia de idioma con `preset.preferred_audio_language` (comparando códigos ISO 639-1 / 639-2: `es`, `spa`, `es-419`, `en`, `eng`)". | T2 |
| S5 | BUG-002: "Prioridad 2: Pista marcada con `is_default == true`. Prioridad 3: Primera pista de audio disponible (`.first()`)." | T2 |
| S6 | BUG-002: "Reflejar en el campo `reasoning` del plan la pista seleccionada, su índice de entrada, idioma y cantidad de canales." | T2 |
| S7 | T0.2: "Fixture con 3 pistas de audio selecciona la pista de idioma solicitada en lugar de la primera." | T2 |
| S8 | BUG-003: "Documentar explícitamente en el plan cuando un subtítulo no es compatible con el contenedor destino y no se puede realizar *burn-in* en CPU sin autorización." | T3 |
| S9 | BUG-003: "Sanitizar la transcodificación de texto plano a `mov_text` en MP4." | T3 |
| S10 | BUG-003: "Asegurar que una falla en la pista de subtítulos no corrompa el proceso de transcodificación de video/audio." | T3 |
| S11 | BUG-004: "Refactorizar `validate_output` para validar que el conjunto de pistas del archivo resultante coincida en cantidad y tipo con las pistas declaradas en el plan de conversión." | T4 |
| S12 | T0.4: "No falla falsamente ante salidas multi-audio y verifica códecs de todas las pistas." | T4 |
| S13 | BUG-005: "Comprobar en `cauce-ffmpeg/toolchain.rs` si el filtro `zscale` está reportado en `Capabilities.filters`." | T5 |
| S14 | BUG-005: "Si `zscale` no está presente, utilizar como fallback el filtro nativo de mapeo de tonos de FFmpeg: `tonemap=mobius,format=yuv420p`" | T5 |
| S15 | BUG-006: "Incorporar en la rutina de inicio de `cauce-daemon` un reconciliador de almacenamiento que identifique archivos temporales `.cauce-*.part.*` asociados a trabajos en estado `interrupted` o `failed` y proporcione una política clara de purga o recuperación." | T6 |
| S16 | G0: "Ninguna modificación de Fase 0 debe alterar los 13 presets del sistema ni romper clientes existentes." | T1–T7 |
| S17 | G1: "Toda matriz de downmix, filtro y selector de pistas debe contar con pruebas unitarias en `cauce-core` que verifiquen el vector `argv` resultante sin requerir ejecución de procesos." | T1–T5 |
| S18 | G4: "El daemon no debe dejar procesos zombis de FFmpeg ni temporales huérfanos tras cancelaciones forzadas o reinicios." | T6 |
| S19 | Pedido del usuario: "verificado que no rompa los 13 presets existentes […] implementación TDD de la Fase 0 directo sobre main con commits atómicos (≤400 líneas, Conventional Commits y sin Co-Authored-By)". | T1–T7 |

Fuera de alcance de esta fase (Fase 1): `TrackSelection`, `CustomConversionOptions`, Command Builder v2, downmix de doble pista, burn-in de subtítulos, `alimiter`, cambios de protocolo IPC y UI. `G2` (verificación acústica `-24`…`-18` LUFS sobre archivo real) se ejecuta en la puerta de release del host, no como test unitario.

---

## Reglas de ejecución

1. **TDD estricto.** Cada tarea abre con tests que fallan. La evidencia RED (comando + salida recortada) se anota en `## Log` antes de escribir la implementación. Sin RED registrado, no hay GREEN.
2. **Un commit atómico por tarea, en `main`, ≤400 líneas.** Convención `tipo(scope): descripción` en inglés. Prohibido: ramas de feature, worktrees, `Co-Authored-By`, trailers de IA.
3. **Sin cambios de contrato (S16).** No se renombran campos serializados, no se agregan variantes obligatorias a enums de contrato y no se altera ningún `id` de preset. Un cambio de contrato exige ADR y sale de esta fase.
4. **Sin ejecución de procesos en tests (S17).** Los tests de `cauce-core` verifican el vector `argv` y las estructuras de plan; nunca invocan `ffmpeg`.
5. **Compatibilidad de presets verificada al cierre de cada tarea.** Si un cambio altera el `argv` de alguno de los 13 presets sin que el bug lo exija, es regresión y se corrige antes del commit (S16, S19).
6. **Alcance cerrado.** Nada de Fase 1 adelantada. Si una remediación necesita un tipo nuevo, se agrega el mínimo indispensable en `plan.rs` y se registra en el `## Log`.

---

## Tareas

| ID | Norma | Descripción | Specs | Route | Commit previsto |
|---|---|---|---|---|---|
| T1 | T0.1, T0.3 | `audio_filter_chain` en `command.rs` + matriz de downmix estéreo con realce de diálogo; levantar la prohibición de filtros de audio. | S1, S2, S3 | inline | `feat(core): add audio filter chain with dialogue boost downmix` |
| T2 | T0.2 | Selección de audio y subtítulos por idioma preferido, luego `is_default`, luego primera pista, con `reasoning` explícito. | S4, S5, S6, S7 | inline | `fix(core): select audio and subtitle tracks by preferred language` |
| T3 | BUG-003 | Diagnóstico de subtítulos incompatibles en el plan, saneamiento de `mov_text` y desacople de fallas de subtítulos. | S8, S9, S10 | inline | `fix(core): report incompatible subtitle tracks instead of failing` |
| T4 | T0.4 | `validate_output` contrasta cantidad y códec de todas las pistas contra el plan. | S11, S12 | inline | `fix(core): validate every output track against the plan` |
| T5 | T0.5 | Chequeo de `zscale` en `Capabilities.filters` y fallback nativo `tonemap=mobius`. | S13, S14 | inline | `fix(ffmpeg): fall back to native tonemap when zscale is missing` |
| T6 | T0.6 | Reconciliador de temporales `.cauce-*.part.*` huérfanos al arrancar el daemon. | S15, S18 | inline | `feat(daemon): reconcile orphan temp files on startup` |
| T7 | G0, G1 | Puertas: presets intactos, `fmt`, `clippy`, suite completa, documentación de la fase. | S16, S17, S19 | inline | `docs(odd): record phase 0 remediation evidence` |

---

## Tests unitarios planificados

Nombre en inglés, cuerpo en `#[cfg(test)] mod tests` del módulo correspondiente. Ningún test invoca procesos.

| Tarea | Test | Archivo | RED esperado |
|---|---|---|---|
| T1 | `audio_filter_chain_applies_dialogue_boost_for_surround_source` | `crates/cauce-core/src/command.rs` | `-af` ausente en el `argv` actual |
| T1 | `audio_filter_chain_is_absent_for_stereo_source` | `crates/cauce-core/src/command.rs` | No hay cadena de filtros que devolver |
| T1 | `audio_filter_chain_never_downmixes_when_target_keeps_surround` | `crates/cauce-core/src/command.rs` | Sin filtros en el `argv` |
| T1 | `normalize_is_no_longer_rejected` | `crates/cauce-core/src/command.rs` | `valid()` devuelve `Err` por la allowlist |
| T1 | `argv_keeps_seventeen_preset_shapes_without_audio_filter` | `crates/cauce-core/src/command.rs` | Sin `-af` para fuentes estéreo: debe seguir sin `-af` |
| T2 | `audio_selection_prefers_preferred_language_over_first_track` | `crates/cauce-core/src/compat.rs` | Selecciona la pista 0 (inglés) |
| T2 | `audio_selection_matches_iso639_alpha2_and_alpha3_aliases` | `crates/cauce-core/src/compat.rs` | Sin normalización de códigos |
| T2 | `audio_selection_falls_back_to_default_disposition` | `crates/cauce-core/src/compat.rs` | Ignora `is_default` |
| T2 | `audio_selection_falls_back_to_first_track` | `crates/cauce-core/src/compat.rs` | Sin regresión de comportamiento |
| T2 | `audio_reasoning_reports_index_language_and_channels` | `crates/cauce-core/src/compat.rs` | `reasoning` vacío |
| T2 | `subtitle_selection_prefers_preferred_language` | `crates/cauce-core/src/compat.rs` | Selecciona la pista 0 |
| T3 | `incompatible_bitmap_subtitle_is_reported_not_silently_dropped` | `crates/cauce-core/src/compat.rs` | Descarta sin avisar |
| T3 | `text_subtitle_is_encoded_as_mov_text_in_mp4` | `crates/cauce-core/src/command.rs` | Sin saneamiento verificable |
| T3 | `subtitle_burn_in_still_refuses_without_authorization` | `crates/cauce-core/src/command.rs` | Sin test que fije la negativa |
| T4 | `validate_output_accepts_multiple_audio_tracks` | `crates/cauce-core/src/output.rs` | Falso positivo/negativo con la primera pista |
| T4 | `validate_output_rejects_audio_track_count_mismatch` | `crates/cauce-core/src/output.rs` | No compara cantidad |
| T4 | `validate_output_checks_every_audio_codec` | `crates/cauce-core/src/output.rs` | Solo mira `first()` |
| T5 | `tonemap_uses_zscale_when_capabilities_report_it` | `crates/cauce-core/src/command.rs` | Sin consulta de capacidades |
| T5 | `tonemap_falls_back_to_native_mobius_without_zscale` | `crates/cauce-core/src/command.rs` | Asume `zscale` siempre presente |
| T6 | `reconcile_removes_temp_files_of_interrupted_jobs` | `crates/cauce-jobs/src/` | No existe reconciliador |
| T6 | `reconcile_keeps_temp_files_of_active_jobs` | `crates/cauce-jobs/src/` | No existe reconciliador |
| T6 | `reconcile_ignores_files_outside_the_temp_pattern` | `crates/cauce-jobs/src/` | No existe reconciliador |

---

## Log

**L1** — Pedido del usuario (literal, 2026-11-24):

> "Che, tenemos el documento normativo de arquitectura en docs/architecture-phases-0-and-1.md para encarar la evolución técnica hacia la versión v0.2.0 de Cauce en dos fases consecutivas (Fase 0: Auditoría y Remediación de Bugs BUG-001 a BUG-006; Fase 1: Motor Multimedia Avanzado con TrackSelection y Downmix). Necesito que: 1. Estudies a fondo docs/architecture-phases-0-and-1.md y explores el estado actual del proyecto (crates/cauce-core/src/command.rs, compat.rs, output.rs, etc.). 2. Redactes el documento ODD formal para la Fase 0 en odd/tasks/cauce-v0.2.0-fase0-auditoria.md (con specs S# basadas en los criterios de aceptación de BUG-001 a BUG-006, tareas T#, tests unitarios planificados y registro en log). 3. Una vez redactado el plan y verificado que no rompa los 13 presets existentes, arrancá con la implementación TDD de la Fase 0 directo sobre main con commits atómicos (≤400 líneas, Conventional Commits y sin Co-Authored-By). Tené presente que al terminar la fase vamos a pasar todas las puertas de calidad, compilar release, probar el .deb en el host local y publicar la release con sincronización en Portafolio y perfil. Avisame apenas tengas el ODD redactado y listo para avanzar."

**L2** — Decisión de alcance: `G2` (verificación acústica LUFS) no puede ejecutarse como test unitario ni en CI sin hardware de audio ni el archivo de benchmark de 22.6 GB. Se difiere a la puerta de release en el host, donde se medirá sobre la conversión real. Los tests de esta fase fijan la matriz de filtros en el `argv`, que es lo que `G1` exige.
