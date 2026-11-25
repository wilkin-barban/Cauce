# Changelog

Todas las versiones notables de Cauce. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto usa
[Versionado Semántico](https://semver.org/lang/es/).

## [0.2.0] — 2026-11-25

Motor multimedia. La selección de pistas deja de ser automática y pasa a ser una
decisión del usuario, de punta a punta: GUI, CLI, IPC y plan persistido.

### Añadido

- **Selección explícita de pistas** (`TrackSelection`): índice de audio, idioma,
  subtítulo y modo de subtítulos, con validación en el borde (lista vacía solo
  para `passthrough`, sin duplicados, máximo cuatro pistas).
- **Opciones personalizadas** (`CustomConversionOptions`): resolución, CRF,
  codificadores, bitrate y curva de mapeo de tonos, aplicadas sobre el preset y
  siempre con su línea en `reasoning[]`.
- **Motor acústico de doble pista** (`dual_track_universal`): pista 0 estéreo
  AAC 192k con realce de diálogo y limitador, pista 1 AC-3 448k 5.1 sin filtro.
- **Incrustado real de subtítulos**: texto por `subtitles=` y mapas de bits
  (PGS/VobSub) por composición `overlay`, siempre después del mapeo de tonos.
- **Tres curvas de tone mapping**: `hable`, `mobius` y `reinhard`, con fallback
  nativo cuando el host no trae `zscale`.
- **Command Builder v2**: un `-map`, `-c:a:N`, `-b:a:N`, `-ac:N`, `-filter:a:N`,
  `-metadata:s:a:N` y `-disposition:a:N` por pista.
- **`track_selection` y `custom_options`** en `media.plan` y `jobs.add`, ambos
  opcionales: omitirlos reproduce el pedido de 0.1.x.
- **Controles en la página Convertir**: desplegable de audio etiquetado con
  idioma/códec/canales/bitrate, interruptor de realce de voz, desplegable de
  subtítulos con Suave/Incrustar, resolución, calidad y resumen del plan.
- **CLI `cauce-cli convert`** con `--audio-lang`, `--audio-track`, `--mix`,
  `--subtitles`, `--burn-in`, `--resolution`, `--crf` y `--json`.

### Corregido

- **El plan congelado se ejecuta** (`fix(daemon)`): `start()` recalculaba el plan
  al arrancar y descartaba la selección; el primer trabajo real moría con
  `PLAN_INVALID`. Solo se recalcula una fila sin plan persistido.
- **`max_fps` es un tope, no un objetivo**: una película de 23.976 fps con un
  preset de 60 fps recibía `fps=60`, lo que obliga a FFmpeg a interpolar frames.
  Un reencode de 4K pasaba de minutos a horas.
- **Validación de salida por pista**: una salida de doble audio (AAC + AC-3) se
  rechazaba comparando todas las pistas contra un solo códec esperado.
- **Test intermitente** en `cauce-ffmpeg`: dos tests compartían directorio
  temporal y se borraban los binarios falsos entre sí.

### Rendimiento

- *The End of Oak Street* (22.6 GB, HEVC 4K HDR, 6 audios, 36 subtítulos): el
  plan sale completo —pista española, doble audio AAC+AC-3, subtítulos español
  en `mov_text`, 1080p SDR— y la conversión arrancó de verdad. Esa corrida se
  canceló al detectar el defecto de `max_fps` documentado arriba; con el tope
  corregido no se repitió de punta a punta antes de esta publicación.

### Limitaciones conocidas

- **`audio_codec` y `audio_bitrate_kbps`** de las opciones personalizadas se
  registran en `reasoning[]` pero todavía no se aplican a la pista estéreo:
  cambiarlos junto al acompañamiento AC-3 exige rediseñar el modo dual.
- **Pico de audio sin confirmar**: la medición de `G2` dio −22.2 LUFS integrado
  (dentro de la ventana), pero el pico medido fue 0.0 dB, cuando el limitador a
  0.98 debería dejarlo en ≈ −0.17 dB. Pendiente de verificar en 0.2.1.
- **`progress.percent` en trabajos completados** sigue guardando el último
  muestreo en vez de 100 (deuda heredada).
- Fuera de alcance: aceleración por hardware, OCR de subtítulos y exportación de
  subtítulos a archivo externo.

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
