# Changelog

Todas las versiones notables de Cauce. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto usa
[Versionado Semántico](https://semver.org/lang/es/).

## [0.1.1] — 2026-11-24

Release de estabilización. Cierra la Fase 0 de
[`docs/architecture-phases-0-and-1.md`](docs/architecture-phases-0-and-1.md)
(seis defectos auditados) sin cambiar contratos persistidos ni los trece presets
del sistema.

### Corregido

- **Audio mudo en estéreo (`BUG-001`).** Un preset que pedía dos canales sobre
  una fuente 5.1/7.1 solo emitía `-ac 2`: FFmpeg dejaba la pista AAC con
  disposición PCE, los reproductores la leían como `channel_layout=unknown` y el
  canal central —donde vive el diálogo— desaparecía. El plan ahora declara un
  filtro cerrado (`AudioFilter::StereoDialogueBoost`) y el constructor emite
  `pan=stereo|FL=0.5*FL+0.85*FC+0.5*SL|FR=0.5*FR+0.85*FC+0.5*SR,volume=1.8`.
- **Filtros de audio prohibidos (`BUG-001`).** `normalize` dejó de abortar el
  trabajo con `PLAN_INVALID`: se traduce a `loudnorm=I=-16:TP=-1.5:LRA=11`, y se
  rechaza explícitamente cuando el plan copia el audio o el host no trae el
  filtro, en vez de ignorar la bandera.
- **Selección ciega de pistas (`BUG-002`).** `plan_audio` y `plan_subtitles`
  tomaban `.first()` sin mirar nada más. Ahora resuelven la prioridad
  documentada: idioma preferido del preset (con alias `es`/`spa`/`es-419` y
  descarte de la región), luego la pista marcada por defecto, luego la primera.
  El `reasoning[]` nombra índice, idioma y cantidad de canales.
- **Subtítulos que mataban la conversión (`BUG-003`).** Un contenedor sin
  soporte de subtítulos devolvía `PLAN_INVALID` y abortaba también video y
  audio. Ahora degrada a quitar la pista con una línea de razonamiento. Una pista
  bitmap descartada nombra el contenedor y aclara que no hay OCR ni burn-in, y
  las pistas ASS/SSA avisan que sus estilos no sobreviven al convertirse a
  `mov_text`.
- **Validación de salida incompleta (`BUG-004`).** `validate_output` comparaba
  contra `audio_streams.first()` y solo miraba que la familia no estuviera
  vacía. Ahora contrasta la cantidad de pistas de cada familia con lo que declara
  el plan y verifica el códec y el límite de resolución de todas las pistas.
- **Tone mapping dependiente de `zscale` (`BUG-005`).** `REQUIRED_FILTERS`
  exigía `zscale`, así que la puerta de capacidades rechazaba justo los FFmpeg
  sin `libzimg`. `zscale` pasa a ser una capacidad del host y, cuando falta, la
  cadena cae a `tonemap=mobius,format=yuv420p`.
- **Temporales huérfanos (`BUG-006`).** Tras un corte abrupto, los archivos
  `.cauce-<trabajo>.part.<ext>` quedaban para siempre. El daemon ahora los
  reconcilia al arrancar: solo borra el `temp_path` persistido de trabajos
  `interrupted`, `failed` o `cancelled`, y solo si el nombre coincide
  exactamente con el patrón del trabajo, para que una fila corrupta no pueda
  borrar un archivo del usuario.

- **Cola bloqueada tras un corte durante la cancelación (hallado en la
  validación de host de esta misma release).** Si el daemon moría con un trabajo
  en `cancelling`, la fila quedaba inmortal: la barrida de arranque no la
  incluía, `active_count` la contaba y `jobs.remove` la rechazaba, así que el
  planificador no volvía a arrancar ningún trabajo. Ahora `cancelling` también
  pasa a `interrupted` al arrancar (`docs/contracts.md` §4 y
  `docs/data-model.md` §4).

### Notas de compatibilidad

- `AudioPlan` gana `filter`, opcional y omitido del JSON cuando no hay filtro:
  un plan persistido por 0.1.0 se sigue leyendo igual.
- `build_ffmpeg_args` recibe un cuarto argumento, `&HostFilters`. Es una función
  interna del workspace; no forma parte del protocolo IPC.
- Los trece presets del sistema no cambian ni un campo.

### Limitaciones conocidas

- **`preferred_audio_language` no se puede elegir todavía.** La selección por
  idioma funciona y está probada, pero ningún preset del catálogo declara esa
  preferencia y el preset propio no expone el campo. Un archivo multiidioma sigue
  tomando la pista por defecto o la primera hasta que la Fase 1 abra la selección
  de pistas en la GUI y la CLI.
- **Verificación acústica pendiente.** La matriz de downmix está fijada en los
  tests a nivel de `argv`; la medición de diálogo entre −24 y −18 LUFS sobre
  audio real necesita el archivo de referencia de 22.6 GB y una escucha atenta.
- **`progress.percent` persistido** sigue guardando el último muestreo en un
  trabajo `completed` en vez de 100 (deuda heredada de 0.1.0; la GUI no lo
  muestra y el estado sí es correcto).
- Fuera de alcance por decisión: watchers, historial visual, presets ilimitados,
  aceleración por hardware y burn-in de subtítulos.

## [0.1.0] — 2026-11-14

Primera versión pública. Conversor nativo para Ubuntu 26.04 con FFmpeg del
sistema, daemon propio sobre socket Unix, cola persistente en SQLite WAL, trece
presets del sistema, progreso real, validación de salida y GUI GTK4/Libadwaita
en español, inglés y portugués de Brasil.
