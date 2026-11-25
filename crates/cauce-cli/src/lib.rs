//! Cauce automation and conversion CLI (`cauce-cli`).
//!
//! Provides the `convert` subcommand for scriptable media conversions through
//! the running Cauce daemon over its Unix domain socket (`docs/contracts.md` §5).
//!
//! Exit codes:
//! - `0`: Success (`EXIT_SUCCESS`).
//! - `1`: Runtime failure (`EXIT_RUNTIME_ERROR`): daemon socket missing, daemon unavailable,
//!   RPC communication error, or daemon-reported runtime error.
//! - `2`: Usage error (`EXIT_USAGE_ERROR`): invalid command-line arguments, missing required
//!   flags, unknown preset, `--audio-lang` matching no stream, or out-of-range `--crf`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use cauce_core::{
    AudioMixMode, CustomConversionOptions, MediaSnapshot, ResolutionTarget, SubtitleMode,
    ToneMappingOption, TrackSelection,
};
use serde_json::Value;

/// Exit code for successful completion.
pub const EXIT_SUCCESS: i32 = 0;

/// Exit code for runtime failures (missing daemon, socket error, daemon rejection).
pub const EXIT_RUNTIME_ERROR: i32 = 1;

/// Exit code for usage errors (bad flags, missing arguments, invalid parameters).
pub const EXIT_USAGE_ERROR: i32 = 2;

/// Default preset if none is specified on the command line.
pub const DEFAULT_PRESET: &str = "system-smarttv-compat";

/// Errors originating from CLI argument parsing or runtime execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// Command-line usage error (exit code 2).
    Usage(String),
    /// Runtime failure during communication with the daemon (exit code 1).
    Runtime(String),
    /// Help requested by the user (`-h` / `--help`, exit code 0).
    Help(String),
}

impl CliError {
    /// Returns the corresponding process exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Help(_) => EXIT_SUCCESS,
            Self::Usage(_) => EXIT_USAGE_ERROR,
            Self::Runtime(_) => EXIT_RUNTIME_ERROR,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Help(text) => write!(f, "{text}"),
            Self::Usage(msg) => write!(f, "Error: {msg}"),
            Self::Runtime(msg) => write!(f, "Error: {msg}"),
        }
    }
}

impl std::error::Error for CliError {}

/// Parsed options for `cauce-cli convert`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertArgs {
    pub inputs: Vec<PathBuf>,
    pub preset: String,
    pub output_dir: PathBuf,
    pub audio_tracks: Vec<u32>,
    pub audio_lang: Option<String>,
    pub mix: Option<AudioMixMode>,
    pub subtitles: Option<(Option<u32>, SubtitleMode)>,
    pub resolution: Option<ResolutionTarget>,
    pub crf: Option<u8>,
    pub tone_mapping: Option<ToneMappingOption>,
    pub json: bool,
}

impl ConvertArgs {
    /// Whether any flag specifically requested audio or subtitle track selection.
    pub fn has_track_selection(&self) -> bool {
        !self.audio_tracks.is_empty()
            || self.audio_lang.is_some()
            || self.mix.is_some()
            || self.subtitles.is_some()
    }

    /// Whether any flag requested custom conversion options.
    pub fn has_custom_options(&self) -> bool {
        self.resolution.is_some() || self.crf.is_some() || self.tone_mapping.is_some()
    }

    /// Builds a [`TrackSelection`] if any track flag was provided.
    pub fn build_track_selection(&self) -> Result<Option<TrackSelection>, CliError> {
        if !self.has_track_selection() {
            return Ok(None);
        }
        let (sub_idx, sub_mode) = self.subtitles.unwrap_or((None, SubtitleMode::SoftEmbed));
        let mix_mode = self.mix.unwrap_or(AudioMixMode::Passthrough);
        let audio_indices = self.audio_tracks.clone();

        Ok(Some(TrackSelection {
            audio_track_indices: audio_indices,
            subtitle_track_index: sub_idx,
            subtitle_mode: sub_mode,
            audio_mix_mode: mix_mode,
        }))
    }

    /// Builds a [`CustomConversionOptions`] if any option flag was provided.
    pub fn build_custom_options(&self) -> Option<CustomConversionOptions> {
        if !self.has_custom_options() {
            return None;
        }
        Some(CustomConversionOptions {
            resolution: self.resolution.unwrap_or(ResolutionTarget::Original),
            crf: self.crf,
            video_bitrate_kbps: None,
            video_codec: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            tone_mapping: self.tone_mapping.unwrap_or(ToneMappingOption::Hable),
        })
    }
}

/// Help text in Spanish for the CLI.
pub fn help_text() -> String {
    format!(
        "\
Uso: cauce-cli convert [OPCIONES] <INPUT>...

Convierte archivos multimedia mediante el motor de Cauce.

Argumentos posicionales:
  <INPUT>...                  Uno o más archivos de entrada a procesar.

Opciones:
  --preset <id>               Preset de conversión (por defecto: {DEFAULT_PRESET}).
  --output-dir <ruta>         Directorio de salida para los archivos convertidos (requerido).
  --audio-track <índice>      Índice de pista de audio a incluir (repetible).
  --audio-lang <código>       Idioma de pista de audio (ej. 'es', 'spa', 'es-419', 'en').
  --mix <modo>                Modo de mezcla de audio: 'passthrough', 'dialogue', 'dual'.
  --subtitles <modo|índice>   Subtítulos: 'none', 'soft', 'burn' o índice numérico.
  --resolution <resolución>   Resolución: 'original', '1080p', '720p', '480p'.
  --crf <0-51>                Factor de compresión CRF entre 0 y 51.
  --tone-mapping <algoritmo>  Mapeo de tonos HDR a SDR: 'hable', 'mobius', 'reinhard'.
  --json                      Imprime la respuesta Job[] del daemon en formato JSON.
  -h, --help                  Muestra esta ayuda.
"
    )
}

/// Parses CLI arguments into [`ConvertArgs`].
pub fn parse_convert_args(args: &[String]) -> Result<ConvertArgs, CliError> {
    let mut tokens = args;
    if let Some(first) = tokens.first() {
        if first == "cauce-cli" || first.ends_with("/cauce-cli") {
            tokens = &tokens[1..];
        }
    }

    if tokens.is_empty() {
        return Err(CliError::Usage(
            "Se requiere un subcomando. Uso: cauce-cli convert [OPCIONES] <INPUT>...\n\
             Para ver la ayuda completa: cauce-cli --help"
                .to_string(),
        ));
    }

    if tokens[0] == "-h" || tokens[0] == "--help" {
        return Err(CliError::Help(help_text()));
    }

    if tokens[0] != "convert" {
        return Err(CliError::Usage(format!(
            "Subcomando desconocido '{}'. Subcomandos disponibles: 'convert'.\n\
             Uso: cauce-cli convert [OPCIONES] <INPUT>...",
            tokens[0]
        )));
    }

    let flags = &tokens[1..];
    let mut inputs = Vec::new();
    let mut preset = DEFAULT_PRESET.to_string();
    let mut output_dir: Option<PathBuf> = None;
    let mut audio_tracks = Vec::new();
    let mut audio_lang: Option<String> = None;
    let mut mix: Option<AudioMixMode> = None;
    let mut subtitles: Option<(Option<u32>, SubtitleMode)> = None;
    let mut resolution: Option<ResolutionTarget> = None;
    let mut crf: Option<u8> = None;
    let mut tone_mapping: Option<ToneMappingOption> = None;
    let mut json = false;

    let mut i = 0;
    while i < flags.len() {
        let arg = &flags[i];
        match arg.as_str() {
            "-h" | "--help" => return Err(CliError::Help(help_text())),
            "--preset" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--preset'.".to_string(),
                    ));
                }
                preset = flags[i].clone();
            }
            "--output-dir" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--output-dir'.".to_string(),
                    ));
                }
                output_dir = Some(PathBuf::from(&flags[i]));
            }
            "--audio-track" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--audio-track'.".to_string(),
                    ));
                }
                let val = &flags[i];
                let track_idx = val.parse::<u32>().map_err(|_| {
                    CliError::Usage(format!(
                        "Índice de pista de audio inválido para '--audio-track': '{val}'. Debe ser un número entero mayor o igual a 0."
                    ))
                })?;
                audio_tracks.push(track_idx);
            }
            "--audio-lang" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--audio-lang'.".to_string(),
                    ));
                }
                audio_lang = Some(flags[i].clone());
            }
            "--mix" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--mix'.".to_string(),
                    ));
                }
                let val = &flags[i];
                match val.to_ascii_lowercase().as_str() {
                    "passthrough" => mix = Some(AudioMixMode::Passthrough),
                    "dialogue" => mix = Some(AudioMixMode::StereoDialogueBoost),
                    "dual" => mix = Some(AudioMixMode::DualTrackUniversal),
                    _ => {
                        return Err(CliError::Usage(format!(
                            "Modo de mezcla desconocido para '--mix': '{val}'. Valores permitidos: 'passthrough', 'dialogue', 'dual'."
                        )));
                    }
                }
            }
            "--subtitles" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--subtitles'.".to_string(),
                    ));
                }
                let val = &flags[i];
                match val.to_ascii_lowercase().as_str() {
                    "none" => subtitles = Some((None, SubtitleMode::Strip)),
                    "soft" => subtitles = Some((None, SubtitleMode::SoftEmbed)),
                    "burn" => subtitles = Some((None, SubtitleMode::BurnIn)),
                    other => {
                        let idx = other.parse::<u32>().map_err(|_| {
                            CliError::Usage(format!(
                                "Valor inválido para '--subtitles': '{val}'. Valores permitidos: 'none', 'soft', 'burn' o un índice numérico."
                            ))
                        })?;
                        subtitles = Some((Some(idx), SubtitleMode::SoftEmbed));
                    }
                }
            }
            "--resolution" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--resolution'.".to_string(),
                    ));
                }
                let val = &flags[i];
                match val.to_ascii_lowercase().as_str() {
                    "original" => resolution = Some(ResolutionTarget::Original),
                    "1080p" => resolution = Some(ResolutionTarget::MaxHeight(1080)),
                    "720p" => resolution = Some(ResolutionTarget::MaxHeight(720)),
                    "480p" => resolution = Some(ResolutionTarget::MaxHeight(480)),
                    _ => {
                        return Err(CliError::Usage(format!(
                            "Resolución desconocida para '--resolution': '{val}'. Valores permitidos: 'original', '1080p', '720p', '480p'."
                        )));
                    }
                }
            }
            "--crf" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--crf'.".to_string(),
                    ));
                }
                let val = &flags[i];
                let num = val.parse::<i64>().map_err(|_| {
                    CliError::Usage(format!(
                        "El valor de '--crf' debe ser un número entero entre 0 y 51, pero se recibió '{val}'."
                    ))
                })?;
                if !(0..=51).contains(&num) {
                    return Err(CliError::Usage(format!(
                        "El valor de '--crf' debe estar en el rango 0..=51, pero se recibió '{val}'."
                    )));
                }
                crf = Some(num as u8);
            }
            "--tone-mapping" => {
                i += 1;
                if i >= flags.len() {
                    return Err(CliError::Usage(
                        "Falta el valor para el argumento '--tone-mapping'.".to_string(),
                    ));
                }
                let val = &flags[i];
                match val.to_ascii_lowercase().as_str() {
                    "hable" => tone_mapping = Some(ToneMappingOption::Hable),
                    "mobius" => tone_mapping = Some(ToneMappingOption::Mobius),
                    "reinhard" => tone_mapping = Some(ToneMappingOption::Reinhard),
                    _ => {
                        return Err(CliError::Usage(format!(
                            "Algoritmo de mapeo de tonos desconocido para '--tone-mapping': '{val}'. Valores permitidos: 'hable', 'mobius', 'reinhard'."
                        )));
                    }
                }
            }
            "--json" => {
                json = true;
            }
            other if other.starts_with('-') => {
                return Err(CliError::Usage(format!("Opción desconocida: '{other}'.")));
            }
            positional => {
                inputs.push(PathBuf::from(positional));
            }
        }
        i += 1;
    }

    if inputs.is_empty() {
        return Err(CliError::Usage(
            "Se requiere al menos una ruta de entrada para 'cauce-cli convert'.".to_string(),
        ));
    }

    let output_dir = output_dir.ok_or_else(|| {
        CliError::Usage("El argumento '--output-dir' es obligatorio.".to_string())
    })?;

    Ok(ConvertArgs {
        inputs,
        preset,
        output_dir,
        audio_tracks,
        audio_lang,
        mix,
        subtitles,
        resolution,
        crf,
        tone_mapping,
        json,
    })
}

/// Normalizes a language code for matching.
///
/// Converts to lowercase and normalizes common 3-letter codes to their 2-letter
/// equivalents ('spa' -> 'es', 'eng' -> 'en', 'por' -> 'pt', 'fra'/'fre' -> 'fr',
/// 'deu'/'ger' -> 'de', 'ita' -> 'it'). Subtags (e.g. 'es-419') are stripped to
/// their primary subtag before comparison, and prefix matching allows 2-letter
/// codes to match full identifiers.
pub fn normalize_language_code(code: &str) -> String {
    let clean = code.trim().to_ascii_lowercase();
    let base = clean.split(['-', '_']).next().unwrap_or(&clean);
    match base {
        "spa" => "es".to_string(),
        "eng" => "en".to_string(),
        "por" => "pt".to_string(),
        "fra" | "fre" => "fr".to_string(),
        "deu" | "ger" => "de".to_string(),
        "ita" => "it".to_string(),
        other => {
            if other.len() >= 2 {
                other[..2].to_string()
            } else {
                other.to_string()
            }
        }
    }
}

/// Compares a stream language tag against a target language query.
pub fn language_matches(stream_lang: Option<&str>, target_lang: &str) -> bool {
    let Some(stream) = stream_lang else {
        return false;
    };
    let s_norm = normalize_language_code(stream);
    let t_norm = normalize_language_code(target_lang);
    s_norm == t_norm || s_norm.starts_with(&t_norm) || t_norm.starts_with(&s_norm)
}

/// Resolves an audio language query against a probed media snapshot.
pub fn resolve_audio_language(snapshot: &MediaSnapshot, code: &str) -> Result<u32, String> {
    for stream in &snapshot.audio_streams {
        if language_matches(stream.language.as_deref(), code) {
            return Ok(stream.index);
        }
    }

    let available = if snapshot.audio_streams.is_empty() {
        "ninguna pista de audio en la entrada".to_string()
    } else {
        snapshot
            .audio_streams
            .iter()
            .map(|s| s.language.as_deref().unwrap_or("indefinido"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    Err(format!(
        "No se encontró ninguna pista de audio con el idioma '{code}'. Idiomas disponibles en la entrada: [{available}]."
    ))
}

/// Builds the JSON-RPC request for `jobs.add`.
///
/// When `track_selection` or `custom_options` are `None`, they are omitted entirely
/// from the parameters, maintaining byte-identical parity with Cauce 0.1.x requests.
pub fn build_jobs_add_request(
    inputs: &[PathBuf],
    output_dir: &Path,
    preset_id: &str,
    track_selection: Option<&TrackSelection>,
    custom_options: Option<&CustomConversionOptions>,
) -> Value {
    let mut params = serde_json::json!({
        "inputs": inputs.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "output_dir": output_dir.to_string_lossy(),
        "preset_id": preset_id,
    });

    if let Some(selection) = track_selection {
        if let Ok(value) = serde_json::to_value(selection) {
            params["track_selection"] = value;
        }
    }

    if let Some(options) = custom_options {
        if let Ok(value) = serde_json::to_value(options) {
            params["custom_options"] = value;
        }
    }

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "jobs.add",
        "params": params,
    })
}

/// Resolves the daemon socket path at `$XDG_RUNTIME_DIR/cauce/daemon.sock`.
pub fn socket_path() -> Result<PathBuf, CliError> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
        CliError::Runtime(
            "XDG_RUNTIME_DIR no está definido. No se puede determinar la ruta del socket del daemon."
                .to_string(),
        )
    })?;
    let path = PathBuf::from(runtime_dir);
    if !path.is_absolute() {
        return Err(CliError::Runtime(
            "XDG_RUNTIME_DIR no es una ruta absoluta. No se puede determinar la ruta del socket del daemon."
                .to_string(),
        ));
    }
    Ok(path.join("cauce").join("daemon.sock"))
}

/// Simple, synchronous JSON-RPC 2.0 client over a Unix domain socket.
pub struct RpcClient {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

impl RpcClient {
    /// Connects to the daemon socket.
    pub fn connect(path: &Path) -> Result<Self, CliError> {
        let stream = UnixStream::connect(path).map_err(|err| {
            CliError::Runtime(format!(
                "No se pudo conectar al daemon de Cauce en '{}': {err}. Asegurate de que el daemon esté en ejecución.",
                path.display()
            ))
        })?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| CliError::Runtime(format!("Error al configurar el socket: {e}")))?;
        let reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| CliError::Runtime(format!("Error al clonar el socket: {e}")))?,
        );
        Ok(Self {
            reader,
            writer: stream,
            next_id: 1,
        })
    }

    /// Sends a JSON-RPC request and waits for the correlated reply, skipping events.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, CliError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut payload = serde_json::to_string(&request).map_err(|e| {
            CliError::Runtime(format!("Error al serializar la petición JSON-RPC: {e}"))
        })?;
        payload.push('\n');

        self.writer.write_all(payload.as_bytes()).map_err(|e| {
            CliError::Runtime(format!("Error al escribir en el socket del daemon: {e}"))
        })?;
        self.writer.flush().map_err(|e| {
            CliError::Runtime(format!("Error al sincronizar el socket del daemon: {e}"))
        })?;

        loop {
            let mut line = String::new();
            let n = self.reader.read_line(&mut line).map_err(|e| {
                CliError::Runtime(format!("Error al leer del socket del daemon: {e}"))
            })?;
            if n == 0 {
                return Err(CliError::Runtime(
                    "El daemon de Cauce cerró la conexión inesperadamente.".to_string(),
                ));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let msg: Value = serde_json::from_str(trimmed).map_err(|e| {
                CliError::Runtime(format!("Respuesta inválida recibida del daemon: {e}"))
            })?;

            let Some(msg_id) = msg.get("id").and_then(|id_val| id_val.as_u64()) else {
                // Notifications carry no id or id is null; skip them.
                continue;
            };

            if msg_id == id {
                if let Some(err) = msg.get("error") {
                    let message = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Error desconocido devuelto por el daemon");
                    let code = err
                        .get("code")
                        .and_then(|c| c.as_str())
                        .unwrap_or("INTERNAL");
                    return Err(CliError::Runtime(format!("{code}: {message}")));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }
}

/// Executes the CLI command with the given argument list and streams.
pub fn run(args: Vec<String>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    match execute_run(&args, stdout) {
        Ok(()) => EXIT_SUCCESS,
        Err(err) => {
            let code = err.exit_code();
            let _ = match &err {
                CliError::Help(msg) => writeln!(stdout, "{msg}"),
                CliError::Usage(msg) | CliError::Runtime(msg) => writeln!(stderr, "Error: {msg}"),
            };
            code
        }
    }
}

fn execute_run(args: &[String], stdout: &mut dyn Write) -> Result<(), CliError> {
    let parsed = parse_convert_args(args)?;

    let sock_path = socket_path()?;
    let mut client = RpcClient::connect(&sock_path)?;

    // Handshake
    client.call(
        "initialize",
        serde_json::json!({
            "client_name": "cauce-cli",
            "client_version": env!("CARGO_PKG_VERSION"),
        }),
    )?;

    // Validate preset against daemon's presets.list
    let presets = client.call("presets.list", serde_json::json!({}))?;
    let preset_exists = presets
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|p| p.get("id").and_then(Value::as_str) == Some(&parsed.preset))
        })
        .unwrap_or(false);

    if !preset_exists {
        return Err(CliError::Usage(format!(
            "El preset '{}' no existe en el catálogo.",
            parsed.preset
        )));
    }

    // Inspect inputs with media.inspect
    let inspect_paths = parsed
        .inputs
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let inspect_result = client.call(
        "media.inspect",
        serde_json::json!({ "paths": inspect_paths }),
    )?;

    let snapshots: Vec<MediaSnapshot> = serde_json::from_value(inspect_result)
        .map_err(|e| CliError::Runtime(format!("Respuesta inesperada de 'media.inspect': {e}")))?;

    if snapshots.is_empty() {
        return Err(CliError::Runtime(
            "El daemon no devolvió información de análisis para los archivos de entrada."
                .to_string(),
        ));
    }

    // Resolve audio language if requested
    let mut audio_tracks = parsed.audio_tracks.clone();
    if let Some(ref lang) = parsed.audio_lang {
        let resolved_index =
            resolve_audio_language(&snapshots[0], lang).map_err(CliError::Usage)?;
        if !audio_tracks.contains(&resolved_index) {
            audio_tracks.push(resolved_index);
        }
    }

    // Mix mode check: if non-passthrough but empty tracks, pick the default/first audio track
    let mix_mode = parsed.mix.unwrap_or(AudioMixMode::Passthrough);
    if mix_mode != AudioMixMode::Passthrough && audio_tracks.is_empty() {
        if let Some(stream) = snapshots[0]
            .audio_streams
            .iter()
            .find(|s| s.is_default)
            .or_else(|| snapshots[0].audio_streams.first())
        {
            audio_tracks.push(stream.index);
        }
    }

    // Build track selection if any track-related flag was present
    let track_selection = if parsed.has_track_selection() {
        let (sub_idx, sub_mode) = parsed.subtitles.unwrap_or((None, SubtitleMode::SoftEmbed));
        Some(TrackSelection {
            audio_track_indices: audio_tracks,
            subtitle_track_index: sub_idx,
            subtitle_mode: sub_mode,
            audio_mix_mode: mix_mode,
        })
    } else {
        None
    };

    let custom_options = parsed.build_custom_options();

    let request = build_jobs_add_request(
        &parsed.inputs,
        &parsed.output_dir,
        &parsed.preset,
        track_selection.as_ref(),
        custom_options.as_ref(),
    );

    let jobs_result = client.call("jobs.add", request["params"].clone())?;

    if parsed.json {
        let json_str = serde_json::to_string(&jobs_result).map_err(|e| {
            CliError::Runtime(format!("Error al serializar el resultado en JSON: {e}"))
        })?;
        writeln!(stdout, "{json_str}").map_err(|e| {
            CliError::Runtime(format!("Error al escribir en la salida estándar: {e}"))
        })?;
    } else {
        let jobs: Vec<cauce_core::Job> = serde_json::from_value(jobs_result.clone())
            .map_err(|e| CliError::Runtime(format!("Respuesta inesperada de 'jobs.add': {e}")))?;
        writeln!(stdout, "Trabajos encolados: {}", jobs.len()).map_err(|e| {
            CliError::Runtime(format!("Error al escribir en la salida estándar: {e}"))
        })?;
        for job in &jobs {
            writeln!(
                stdout,
                "  [{}] {} -> {} ({})",
                job.id, job.input_path, job.output_path, job.preset_id
            )
            .map_err(|e| {
                CliError::Runtime(format!("Error al escribir en la salida estándar: {e}"))
            })?;
        }
    }

    Ok(())
}
