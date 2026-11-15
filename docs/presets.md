# Cauce — Catálogo canónico de presets

**Estado:** normativo para v0.1  
**Origen:** catálogo de FF-Converter `v1.1.1`, adaptado a Cauce y a CPU en el MVP.  
**Nombres ES/EN/PT-BR:** ratificados para v0.1.  
**Relacionado:** [`contracts.md`](contracts.md), [`parity.md`](parity.md)

## Reglas del catálogo

- Los presets del sistema son inmutables y se identifican por `id`.
- Los nombres se localizan en ES/EN/PT-BR desde v0.1; PT-BR usa portugués de Brasil.
- En v0.1 todos los presets se resuelven a codificadores CPU. Las preferencias GPU se conservan como metadatos para v0.3.
- Un preset de audio declara `video_codec: none` y elimina el video.
- `video_quality` se interpreta como CRF para CPU (`libx264`, `libx265`, `libsvtav1`, `libvpx-vp9`).
- `audio_bitrate` es un valor FFmpeg validado (`192k`, `320k`, etc.).
- `faststart` solo tiene efecto en contenedores MP4/M4A.
- `preserve_metadata` y `subtitle_policy` son explícitos; no se infieren del contenedor.
- Un preset que pida hardware se resolverá a CPU y la UI mostrará “aceleración no disponible en v0.1”, sin fallar.

## Catálogo

| ID | Nombre ES / EN | Contenedor | Video | Audio | Límite | Calidad | Notas |
|---|---|---|---|---|---|---|---|
| `system-smarttv-compat` | Smart TV — Máxima Compatibilidad / Maximum Compatibility | `mp4` | `h264` | `aac` | 1920×1080, 60 fps | CRF 21, audio 192k | Perfil high, yuv420p, 8-bit, HDR→SDR, subtítulos convertibles, 2 canales |
| `system-smarttv-hevc` | Smart TV — Calidad Eficiente / Efficient Quality | `mkv` | `h265` | `aac` | 3840×2160 | CRF 24, audio 324k | hasta 10-bit (preserva si la fuente ya es 10-bit), HDR preservado, subtítulos preservados |
| `system-youtube-social` | YouTube & Redes Sociales / YouTube & Social | `mp4` | `h264` | `aac` | 1920×1080 | CRF 18, audio 320k | Faststart, perfil high, yuv420p, HDR→SDR, sin subtítulos, 2 canales |
| `system-archival-master` | Archivo / Máxima Calidad / Archival Master | `mkv` | `h265` | `copy` | Sin límite | CRF 18 | CPU, audio original, HDR preservado, subtítulos preservados |
| `system-fast-compress` | Compresión Rápida / Fast Compression | `mp4` | `h265` | `aac` | 1920×1080 | CRF 26, audio 160k | Faststart, HDR→SDR, sin subtítulos, 2 canales |
| `system-audio-mp3` | Extraer Audio — MP3 320k / Extract Audio — MP3 320k | `mp3` | `none` | `mp3` | — | Audio 320k | Conserva metadatos; sin video |
| `system-audio-flac` | Extraer Audio — FLAC / Extract Audio — FLAC | `flac` | `none` | `flac` | — | Lossless | Conserva metadatos; sin video |
| `system-apple-music-export` | Exportación Apple Music / Apple Music Export | `m4a` | `none` | `aac` | — | Audio 256k | Metadatos limpios; sin video |
| `system-samsung-tv` | Samsung TV — 4K / Samsung TV — 4K | `mp4` | `h265` | `aac` | 3840×2160, 60 fps | CRF 22, audio 256k | hasta 10-bit (preserva si la fuente ya es 10-bit), HDR preservado, subtítulos convertibles, 6 canales, faststart |
| `system-lg-tv` | LG TV — HEVC 4K / LG TV — HEVC 4K | `mkv` | `h265` | `aac` | 3840×2160, 60 fps | CRF 22, audio 256k | hasta 10-bit (preserva si la fuente ya es 10-bit), HDR preservado, subtítulos preservados, 6 canales |
| `system-android-tv` | Android TV / Google TV | `mkv` | `h265` | `aac` | 3840×2160, 60 fps | CRF 23, audio 256k | hasta 10-bit (preserva si la fuente ya es 10-bit), HDR preservado, subtítulos preservados |
| `system-audio-alac` | Extraer Audio — ALAC / Extract Audio — ALAC | `m4a` | `none` | `alac` | — | Lossless | Conserva metadatos; sin video |
| `system-audio-wav` | Extraer Audio — WAV PCM / Extract Audio — WAV PCM | `wav` | `none` | `pcm_s16le` | — | PCM 16-bit | Conserva metadatos; sin video |

## Nombres PT-BR

| ID | Nome em português (Brasil) |
|---|---|
| `system-smarttv-compat` | Smart TV — Compatibilidade Máxima |
| `system-smarttv-hevc` | Smart TV — Qualidade Eficiente |
| `system-youtube-social` | YouTube e Redes Sociais |
| `system-archival-master` | Arquivo / Qualidade Máxima |
| `system-fast-compress` | Compressão Rápida |
| `system-audio-mp3` | Extrair Áudio — MP3 320k |
| `system-audio-flac` | Extrair Áudio — FLAC |
| `system-apple-music-export` | Exportação Apple Music |
| `system-samsung-tv` | Samsung TV — 4K |
| `system-lg-tv` | LG TV — HEVC 4K |
| `system-android-tv` | Android TV / Google TV |
| `system-audio-alac` | Extrair Áudio — ALAC |
| `system-audio-wav` | Extrair Áudio — WAV PCM |

## Campos completos

| Campo | Valor por defecto / semántica |
|---|---|
| `device_family` | `generic`, `samsung`, `lg`, `android-tv`, `web`, `archive`, `audio` |
| `hardware_preference` | `auto` salvo archivo/audio; en v0.1 se resuelve a CPU |
| `faststart` | `true` en MP4/M4A; sin efecto en MKV |
| `preserve_metadata` | `true` salvo compresión rápida |
| `subtitle_policy` | `preserve-compatible`, `convert-compatible`, `remove` o `burn-in` |
| `hdr_policy` | `preserve` o `tone-map-sdr` |
| `pixel_format` | `yuv420p` cuando el preset limita a 8-bit |
| `max_bit_depth` | 8, 10 o 12; ausente significa sin restricción explícita |
| `audio_channels` | 2 o 6 cuando el preset lo exige |
| `preferred_audio_language` | Opcional; si está definido, prioriza esa pista |

## Resolución en v0.1

1. Se evalúa compatibilidad con el snapshot.
2. Se calcula `copy` o `transcode` por video, audio y subtítulos.
3. Se selecciona CPU: `libx264`, `libx265`, `libsvtav1`, `libvpx-vp9`.
4. Se selecciona audio: `aac`, `libmp3lame`, `flac`, `alac`, `pcm_s16le`, `libopus`.
5. Se conserva `hardware_preference` en el preset, pero no se usa para elegir un encoder en v0.1.
6. Si el plan requiere hardware para una función no disponible en CPU, se rechaza con `FFMPEG_FEATURE_MISSING`; no se degrada silenciosamente una calidad crítica.

## Pruebas obligatorias

- Cada uno de los 13 presets produce un `ConversionPlan` válido para un fixture de video/audio.
- Los presets de audio producen `video: remove`.
- `system-smarttv-compat` y `system-youtube-social` producen `faststart` en MP4.
- `system-archival-master` conserva audio cuando el contenedor lo permite.
- `system-samsung-tv`, `system-lg-tv` y `system-android-tv` preservan HDR y conservan 10-bit cuando la fuente ya es 10-bit: `ConversionPlan.VideoPlan` (`contracts.md` §2.2) no declara bit depth, así que el plan no puede forzar 10-bit sobre una fuente de 8-bit.
- Un preset con `hardware_preference: auto` en v0.1 termina en CPU y lo declara en `reasoning`.
- Cambiar un preset no muta otro preset ni el catálogo del sistema.
