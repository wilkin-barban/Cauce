# Cauce 0.1.1 — notas de la versión

**Estado:** publicada. Release de estabilización que cierra la Fase 0 de
[`docs/architecture-phases-0-and-1.md`](../docs/architecture-phases-0-and-1.md):
seis defectos auditados, corregidos y verificados sobre el paquete instalado en
el host.

**Artefacto:** `cauce_0.1.1-1_amd64.deb`
**SHA-256:** `d0f5c5fff927fccdec4ef711488f3d9c3f9ae8273c28d2c66a13e0d1a8779330`

## Qué arregla

### El audio de las películas ya no desaparece

Era el defecto más grave y el más silencioso. Cuando un preset pedía dos canales
sobre una fuente 5.1 o 7.1, Cauce solo emitía `-ac 2`. FFmpeg dejaba entonces la
pista AAC con una disposición PCE que los reproductores de Linux leen como
`channel_layout=unknown`, y el canal central —donde está el 100 % del diálogo en
cine y series— se perdía. El resultado era una película muda con música y
efectos de fondo.

Ahora el plan declara un filtro cerrado y el constructor emite una matriz de
mezcla que sube el central por encima de los efectos:

```text
pan=stereo|FL=0.5*FL+0.85*FC+0.5*SL|FR=0.5*FR+0.85*FC+0.5*SR,volume=1.8
```

Además, la prohibición absoluta de filtros de audio desapareció: `normalize` ya
no aborta el trabajo con `PLAN_INVALID`, se traduce a
`loudnorm=I=-16:TP=-1.5:LRA=11`.

### Selección de pistas por idioma

`plan_audio` y `plan_subtitles` tomaban la primera pista sin mirar nada más, así
que un archivo multiidioma siempre se convertía al audio original (normalmente
inglés) y el resto se descartaba en silencio. Ahora resuelven la prioridad
documentada: idioma preferido del preset (con alias `es`/`spa`/`es-419` y
descarte de la región), luego la pista marcada por defecto, luego la primera.

### Un subtítulo ya no mata la conversión

Un contenedor sin soporte de subtítulos devolvía `PLAN_INVALID` y abortaba
también el video y el audio. Ahora degrada a quitar la pista con una línea de
razonamiento. Una pista bitmap (PGS/VobSub) descartada nombra el contenedor y
aclara que no hay OCR ni burn-in; las pistas ASS/SSA avisan que sus estilos no
sobreviven al convertirse a `mov_text`.

### Validación de salida completa

`validate_output` comparaba contra la primera pista y solo miraba que la familia
no estuviera vacía. Ahora contrasta la cantidad de pistas de cada familia con lo
que declara el plan y verifica el códec y el límite de resolución de todas.

### Tone mapping sin `zscale`

`REQUIRED_FILTERS` exigía `zscale`, de modo que la propia puerta de capacidades
rechazaba los FFmpeg compilados sin `libzimg`. `zscale` pasa a ser una capacidad
del host y, cuando falta, la cadena cae a `tonemap=mobius,format=yuv420p` en vez
de romper el trabajo.

### Temporales huérfanos

Tras un corte abrupto, los archivos `.cauce-<trabajo>.part.<ext>` quedaban para
siempre. El daemon los reconcilia al arrancar: solo borra el `temp_path`
persistido de trabajos `interrupted`, `failed` o `cancelled`, y solo si el nombre
coincide exactamente con el patrón del trabajo, para que una fila corrupta no
pueda borrar un archivo del usuario.

### Cola bloqueada tras un corte durante la cancelación

**Hallado en la validación de host de esta misma release**, no en la auditoría
original. Si el daemon moría con un trabajo en `cancelling`, la fila quedaba
inmortal: la barrida de arranque no incluía ese estado, `active_count` sí lo
contaba y `jobs.remove` lo rechazaba por activo. El planificador no volvía a
arrancar ningún trabajo, para siempre, sin ningún mensaje.

Se reprodujo en el host (tres filas `cancelling` de una sesión anterior
bloqueando la cola) y se corrigió: `cancelling` también pasa a `interrupted` al
arrancar. La regla quedó reflejada en `docs/contracts.md` §4 y
`docs/data-model.md` §4.

## Verificación en el host

Todo lo siguiente se ejecutó sobre el `.deb` instalado con `apt-get install`, no
sobre `cargo run`.

| Comprobación | Resultado |
|---|---|
| Instalación | `dpkg -s cauce` → `Version: 0.1.1-1`, `Status: install ok installed` (actualizado sobre 0.1.0-1) |
| Gate de empaquetado | `packaging/check-package.sh` → `check-package: OK` |
| Binarios | `/usr/bin/cauce-daemon`, `cauce-gui`, `cauce-cli` en 755; unidades en `/usr/lib/systemd/user/`, metainfo y `.desktop` en su sitio |
| Servicio y socket | `systemctl --user` → `cauce.socket` y `cauce.service` `active`; directorio `0700`, socket `0600` |
| Versión del daemon en ejecución | `system.capabilities` sobre el socket real → `daemon_version: 0.1.1` |
| Entorno FFmpeg | `ffmpeg 8.0.1`, 226 codificadores, 558 filtros, `ffmpeg_missing: []` |
| Smoke de GUI | `CAUCE_SMOKE=1 xvfb-run -a cauce-gui` → `window-ready`, exit 0, el daemon sobrevive al cierre |
| Barrida de arranque | las tres filas `cancelling` heredadas pasaron a `interrupted` y la cola se destrabó |
| Conversión real 5.1 → estéreo | fuente AC-3 5.1 de 3 s, preset `system-smarttv-compat` → `completed`; salida AAC **2 canales, `channel_layout=stereo`** (antes: `unknown`) |
| Nivel de la salida | `volumedetect` medio −10.7 dB, pico −7.3 dB; EBU R128 integrado −8.3 LUFS, sin clipping |

Nota sobre ese último punto: la fuente de la prueba es un tono sintético de
amplitud plena, no diálogo. Sirve para probar que la cadena corre, que el central
sobrevive y que no hay saturación, pero **no** valida la ventana de −24 a −18 LUFS
que pide `G2`, que necesita una mezcla cinematográfica real.

## Puertas de calidad

`cargo fmt --check` limpio · `cargo clippy --workspace --all-targets -D warnings`
con 0 warnings · `cargo test --workspace`: 61 binarios de test, **350 pruebas, 0
fallos**. Los trece presets del sistema no cambian ni un campo.

## Compatibilidad

- `AudioPlan` gana `filter`, opcional y omitido del JSON cuando no hay filtro: un
  plan persistido por 0.1.0 se sigue leyendo igual.
- `build_ffmpeg_args` recibe un cuarto argumento dentro del workspace. No forma
  parte del protocolo IPC.
- La tabla de transiciones de `docs/contracts.md` §4 suma `cancelling →
  interrupted`.

## Limitaciones conocidas

- **El idioma preferido todavía no se puede elegir.** La selección funciona y
  está probada, pero ningún preset del catálogo declara
  `preferred_audio_language` y el preset propio no expone el campo. Un archivo
  multiidioma sigue tomando la pista por defecto o la primera hasta que la Fase 1
  abra la selección de pistas en la GUI y la CLI.
- **`progress.percent` en trabajos completados** guarda el último muestreo en vez
  de 100. La GUI pinta la barra llena y el estado correcto; el dato de la API no.
- **Burn-in y OCR de subtítulos** siguen fuera de alcance (Fase 1).
- **`cauce-cli` es un marcador de posición**: el binario se instala, pero su
  interfaz pública queda para una fase posterior.
- Fuera de alcance por decisión: watchers, historial visual, presets ilimitados y
  aceleración por hardware.

## Instalación

Requisitos: Ubuntu 26.04 amd64 con FFmpeg 8.0.x del sistema.

```bash
sudo apt install ./cauce_0.1.1-1_amd64.deb
systemctl --user enable --now cauce.socket
systemctl --user status cauce.socket      # active (listening)
cauce-gui
```

El daemon queda vivo aunque se cierre la ventana; las conversiones siguen. La
base de datos y los logs viven en `$XDG_DATA_HOME/cauce/`.
