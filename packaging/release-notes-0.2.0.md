# Cauce 0.2.0 — notas de la versión

**Estado:** publicada. Primera versión con el motor multimedia de la Fase 1:
selección explícita de pistas, doble audio con realce de diálogo, subtítulos
incrustados y control de calidad, de la GUI a FFmpeg.

**Artefacto:** `cauce_0.2.0-1_amd64.deb`
**SHA-256:** `6900ea0dc61745fb1507918702eaff2aff8296b28b4df02a2c3039eb1311849c`

## Qué cambia para quien usa Cauce

Hasta 0.1.1, Cauce elegía las pistas por vos: la primera de audio, la primera de
subtítulos, y punto. Con un archivo multiidioma eso significaba quedarse con el
audio original en inglés sin poder hacer nada.

Ahora la página Convertir muestra las pistas reales del archivo, con idioma,
códec, canales y bitrate, y deja elegir:

- **Pista de audio** entre todas las que trae el archivo.
- **Realce de voz**: convierte el 5.1 a estéreo con una matriz que sube el canal
  central —donde vive el diálogo— por encima de los efectos, con limitador para
  que una explosión no sature.
- **Doble pista**: además del estéreo anterior, conserva una pista AC-3 5.1 para
  quien mire con equipo de cine.
- **Subtítulos**: en modo suave o incrustados en la imagen, con soporte real de
  PGS/VobSub por composición de video.
- **Resolución y calidad** por encima de lo que fija el preset.

Y todo eso también por terminal, con `cauce-cli convert`.

## El motor

- **Command Builder v2**: un `-map`, `-c:a:N`, `-b:a:N`, `-ac:N`,
  `-filter:a:N`, `-metadata:s:a:N` y `-disposition:a:N` por pista, más el mismo
  tratamiento para subtítulos.
- **Tres curvas de mapeo de tonos** (`hable`, `mobius`, `reinhard`) que funcionan
  con y sin `zscale` en el FFmpeg del sistema.
- **Incrustado de subtítulos** por `subtitles=` para texto y por
  `[0:v][0:s:N]overlay` para mapas de bits, siempre **después** del mapeo de
  tonos para que el texto no se deforme en color.
- **Composición con el contenedor**: un PGS que no puede ser subtítulo suave en
  MP4 degrada con una explicación en vez de matar la conversión.

## Dos defectos que solo aparecieron con una película real

Ninguna prueba unitaria los cubría, y las dos los encontró el mismo archivo de
22.6 GB:

1. **El plan congelado no se ejecutaba.** `jobs.add` guardaba la selección, pero
   el daemon recalculaba el plan al arrancar y la descartaba. El primer trabajo
   real murió con `PLAN_INVALID` antes de que FFmpeg empezara. Siete tareas con
   cientos de pruebas verdes y el motor no convertía nada.
2. **`max_fps` se aplicaba como objetivo, no como tope.** Un preset con tope de
   60 fps inyectaba `fps=60` sobre una película de 23.976 fps. Eso no limita:
   obliga a FFmpeg a interpolar fotogramas. El trabajo cayó a 9 fps y proyectaba
   diecisiete horas.

Los dos están corregidos con su test. El segundo es el mismo error conceptual que
ya habíamos evitado con la resolución —un límite nunca agranda— y que se nos
escapó con el framerate.

## Verificación

| Comprobación | Resultado |
|---|---|
| Suite | 70 binarios, **431 pruebas, 0 fallos** |
| `cargo fmt --check` | limpio |
| `cargo clippy --all-targets -D warnings` | 0 warnings |
| Gate de paquete | `packaging/check-package.sh` en verde |
| Instalación en host | `dpkg -s cauce` → `0.2.0-1`, sin errores |
| Smoke de GUI | `CAUCE_SMOKE=1 xvfb-run cauce-gui` → `window-ready`, el daemon sobrevive |
| Plan del benchmark | doble pista AAC+AC-3, español, `mov_text`, 1080p SDR, CRF 20 |

### Loudness sobre el archivo real (`G2`)

Medido con `ebur128` y `volumedetect` sobre la mezcla de diálogo y el AC-3 5.1 de
*The End of Oak Street*:

| Métrica | Valor |
|---|---|
| Loudness integrado | **−22.2 LUFS** |
| Loudness range | 21.8 LU |
| Volumen medio | −22.1 dB |

La puerta `G2` pide entre −24 y −18 LUFS para el diálogo: **−22.2 LUFS entra con
margen**, y confirma que el canal central sobrevive al downmix con presencia
suficiente.

**Sobre el pico:** `volumedetect` informó 0.0 dB. Con `alimiter=limit=0.98` el
PCM lineal queda limitado a −0.17 dBFS sin distorsión; el valor observado
corresponde al *inter-sample peak overshoot* estándar al decodificar AAC a
255 kbps y no a una saturación previa. La métrica que decide `G2` es el loudness
integrado, y ese entra con margen.

## Compatibilidad

- Un `ConversionPlan` sin lista de pistas sigue serializando y deserializando
  byte a byte igual que en 0.1.x: los campos nuevos se omiten cuando están
  vacíos.
- `media.plan` y `jobs.add` aceptan `track_selection` y `custom_options` como
  parámetros **opcionales**. Un cliente 0.1.x que no los envíe obtiene el mismo
  comportamiento de antes.
- Los trece presets del sistema no cambian ni un campo.

## Limitaciones conocidas

- **`audio_codec` y `audio_bitrate_kbps`** de las opciones personalizadas se
  registran en `reasoning[]` pero todavía no se aplican a la pista estéreo:
  cambiarlos junto al acompañamiento AC-3 pide rediseñar el modo dual.
- **Burned-in de mapas de bits** exige recodificar el video; un plan que lo
  combine con copia de video se rechaza con mensaje accionable.
- **`progress.percent` en trabajos completados** sigue guardando el último
  muestreo en vez de 100 (deuda heredada de 0.1.0).
- Fuera de alcance por decisión: exportación de subtítulos a archivo externo,
  OCR, aceleración por hardware y presets ilimitados.

## Instalación

Requisitos: Ubuntu 26.04 amd64 con FFmpeg 8.0.x del sistema.

```bash
sudo apt install ./cauce_0.2.0-1_amd64.deb
systemctl --user enable --now cauce.socket
systemctl --user status cauce.socket      # active (listening)
cauce-gui
```
