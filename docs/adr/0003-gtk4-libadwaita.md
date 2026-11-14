# ADR 0003 — Interfaz GTK4/Libadwaita

- **Estado:** Aceptado
- **Fecha:** 2026-11-14
- **Decisores:** Wilkin Barban
- **Ámbito:** interfaz de usuario nativa

## Contexto

FF-Converter usa Electron, React y una interfaz web. Cauce busca una app nativa de Ubuntu, con buen arranque, integración con GNOME, accesibilidad y un `.deb` sin Chromium.

## Decisión

La GUI será una aplicación Rust con `gtk4-rs` y `libadwaita`, usando `AdwApplicationWindow`, `AdwNavigationSplitView`, widgets nativos, notificaciones del escritorio y el selector de archivos del portal.

La GUI será un cliente del daemon; no contendrá lógica de FFmpeg ni SQLite.

## Consecuencias

- Integración visual y de accesibilidad con GNOME.
- Menor tamaño y consumo que Electron.
- Requiere dependencias GTK4/Libadwaita en el `.deb`.
- La UI se implementa con widgets, no con HTML/CSS.
- El diseño debe respetar el tema claro/oscuro y el acento del sistema.

## Alternativas descartadas

- **Tauri:** sigue usando una capa web y no es tan nativa como GTK4.
- **Slint/Iced:** opciones válidas, pero menos alineadas con GNOME y con la experiencia de Segmenta.

## Validación

- La GUI abre bajo GNOME/Wayland y bajo Xvfb en CI.
- El selector de archivos usa el portal.
- El tema y el idioma cambian sin reiniciar.
- No hay dependencia de Node, Electron ni Chromium.
