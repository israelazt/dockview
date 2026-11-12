# Plan de migración Dockview → Rust

## Primary goal

Abrir un único `Dockview.exe` (sin instalador, sin registro, sin cuenta) que muestre el video de una capturadora UVC económica a **1080p** con el audio sincronizado y **sin retraso perceptible**.

Medida de "hecho": doble clic → imagen y sonido de la consola en pantalla; latencia vídeo ≤ ~2 frames de display (≈33 ms a 60 Hz) más la propia de la capturadora; sin saltos de audio en sesiones largas.

## Decisiones de partida

- Referencia: el C# de la raíz (no se modifica hasta terminar).
- UI: `winit` + D3D11/DXGI directo. Audio: `wasapi` o `windows-rs` directo.
- Estructura: workspace `rust/` → `dockview-core` (lógica + captura + audio) y `dockview-app` (ventana + render).
- Cada tarea: un commit, `cargo test` y `cargo clippy` en verde antes de pasar a la siguiente.
- Skills: `rust-best-practices` y `rust-testing` aplican a todo el código.

---

## Phase 0 — Documentation Discovery

**How this serves the primary goal:** evita inventar APIs; todo lo que sigue (captura, render, audio) depende de firmas reales.

### Hechos verificados en el código C# (copiar comportamiento, no código)

| Tema | Referencia | Comportamiento a replicar |
|---|---|---|
| Selección de stream | `Core/StreamSelector.cs:18-46` | Prefiere 1920×1080 con fps ≥ 55; si no existe, usa toda la lista. Perfiles LowLatency / Quality / Balanced. `PreferUncompressed` prioriza formatos no comprimidos. |
| Captura | `Core/VideoCapture.cs:103-187` | `MF_LOW_LATENCY` + `ENABLE_VIDEO_PROCESSING`; elige tipo nativo; hilo propio MTA; fallback si falla 1080p. |
| Audio | `Core/AudioEngine.cs:69-126, 153-177` | Captura WASAPI compartido → buffer 500 ms → salida con 40 ms. Deriva: cada 5 s (tras 3 s), objetivo 50 ms, zona muerta ±15 ms, corrige 1 ms. |
| Ajustes | `Services/SettingsService.cs:64-83` | JSON en `%APPDATA%\Dockview\settings.json`, camelCase, omitir nulos, fallback silencioso a valores por defecto. Campos: `lastVideoDeviceSymbolicLink`, `lastAudioDeviceId`, `audioOffsetMs` (±300), `volume` (0–1), `wasFullscreen`, `renderPreset`. |

### Versiones actuales en crates.io (consultadas con `cargo search`, 2026-11-11)

- `windows = 0.62.2`
- `wasapi = 0.27.0`
- `winit = 0.31.0-beta.4` (la estable previa es 0.30.x — **decidir en T-1.1 y verificar en docs.rs**)

### Allowed APIs (a confirmar leyendo docs.rs antes de usar; NO asumir)

- Media Foundation: `MFStartup`, `MFEnumDeviceSources`, `MFCreateSourceReaderFromMediaSource`, `IMFSourceReader::{GetNativeMediaType, SetCurrentMediaType, ReadSample}`, atributos `MF_LOW_LATENCY`, `MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING`, `MF_SOURCE_READER_D3D_MANAGER`.
- DXGI/D3D11: `CreateDXGIFactory2`, `IDXGIFactory2::CreateSwapChainForHwnd`, `DXGI_SWAP_EFFECT_FLIP_DISCARD`, `DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT`, `IDXGISwapChain2::{SetMaximumFrameLatency, GetFrameLatencyWaitableObject}`, `DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING`.
- Fuentes: [Reduce latency with DXGI 1.3 swap chains](https://msdn.microsoft.com/en-us/library/ie/dn448914(v=win.10)), [Variable refresh rate displays](https://learn.microsoft.com/vi-vn/windows/win32/direct3ddxgi/variable-refresh-rate-displays), [DXGI flip model](https://learn.microsoft.com/en-ie/Windows/Win32/direct3ddxgi/for-best-performance--use-dxgi-flip-model).

### Anti-patrones (guardas)

- No poner colas de frames: **último frame gana**.
- No convertir color en CPU: YUY2/NV12 → shader en GPU.
- No usar `ResizeBuffers` esperando cambiar el flag waitable (se fija al crear).
- `ALLOW_TEARING` solo con sync interval 0 y el mismo flag en `ResizeBuffers`.
- No usar `unwrap()` en rutas de hardware; error → estado visible en UI, no pánico.
- Capturadoras baratas suelen exponer 1080p60 solo en **MJPEG** (comprimido): hay que decodificarlo; no asumir YUY2 a 1080p60. Confirmar con el dispositivo real (T-2.1).

---

## Phase 1 — Lógica pura (`dockview-core`, sin hardware)

**How this serves the primary goal:** elegir el modo 1080p60 correcto y recordar dispositivos es lo que hace que "abrir y ver" funcione sin configurar; es testeable sin capturadora.

- **T-1.1** Decidir versión de `winit` (0.30 estable vs 0.31 beta) leyendo docs.rs; fijar en `Cargo.toml` del workspace.
- **T-1.2** `settings.rs`: `AppSettings` + `RenderPreset` con `serde` (camelCase, omitir `None`). *Test:* valores por defecto, rango de `audio_offset_ms`.
- **T-1.3** `settings.rs`: `load()`/`save()` en `%APPDATA%\Dockview\settings.json` con fallback silencioso. *Test:* archivo ausente y JSON corrupto → defaults. Portar `Dockview.Tests/AppSettingsTests.cs`.
- **T-1.4** `stream.rs`: struct `StreamInfo` (ancho, alto, fps, comprimido, índice). 
- **T-1.5** `stream.rs`: `select_best()` con perfiles. Portar `Dockview.Tests/StreamSelectorTests.cs` (118 líneas) tal cual.
- **T-1.6** `device.rs`: emparejamiento audio↔video por palabras comunes del nombre. Portar `Dockview.Tests/DeviceMatcherTests.cs`.

**Verificación:** `cargo test -p dockview-core` ≥ todos los casos de los 3 archivos C# portados. **Guarda:** no copiar la API de .NET; usar `Option`/`Result`.

---

## Phase 2 — Captura de video (Media Foundation)

**How this serves the primary goal:** sin frames de la capturadora no hay nada que mostrar; aquí se fija el formato 1080p y la latencia de entrada.

- **T-2.1** Binario de prueba `examples/list_devices.rs`: enumerar capturadoras (nombre + symbolic link) con `MFEnumDeviceSources`. *Verifica:* aparece tu capturadora.
- **T-2.2** Enumerar tipos nativos de la capturadora y volcarlos (formato, ancho, alto, fps). *Verifica:* sabemos si 1080p60 es MJPEG/YUY2/NV12.
- **T-2.3** Abrir `IMFSourceReader` con `MF_LOW_LATENCY`; fijar el tipo elegido por `select_best`.
- **T-2.4** Hilo de captura (`ReadSample` síncrono) que publica el **último frame** en un slot compartido (`Mutex<Option<Frame>>` o triple buffer). Medir fps recibidos.
- **T-2.5** Decodificación MJPEG→NV12/YUY2 si el modo es comprimido (decoder MF, idealmente con `MF_SOURCE_READER_D3D_MANAGER` para hardware).
- **T-2.6** Detección de desconexión y reintento (evento de error → estado `Disconnected`).

**Verificación:** ejemplo imprime "1920x1080 @ ~60 fps" durante 60 s sin acumular retraso (timestamps vs reloj). **Guarda:** sin `Vec` por frame; reutilizar buffers.

---

## Phase 3 — Ventana y render D3D11

**How this serves the primary goal:** es donde se gana la latencia baja y se elimina la copia CPU→bitmap del WPF.

- **T-3.1** Ventana `winit` con título y cierre limpio.
- **T-3.2** Dispositivo D3D11 + swap chain **flip model** con `FRAME_LATENCY_WAITABLE_OBJECT`; `SetMaximumFrameLatency(1)`.
- **T-3.3** Pintar un color sólido en bucle esperando el waitable object. *Verifica:* 60 fps estables, uso de CPU bajo.
- **T-3.4** Textura dinámica + shader: subir frame NV12/YUY2 y convertir a RGB en GPU; dibujar quad a pantalla con letterbox 16:9.
- **T-3.5** Conectar captura (Phase 2) → render: el render toma siempre el último frame.
- **T-3.6** Resize de ventana sin recrear el pipeline (`ResizeBuffers` con los mismos flags).
- **T-3.7** Pantalla completa borderless (F11/Esc).
- **T-3.8** Contador de fps y latencia estimada en título/overlay (para medir de verdad).

**Verificación:** ver Switch a 1080p, sin tearing, comparación subjetiva y medida contra la app C#. **Guarda:** no `Present` sin esperar el waitable.

---

## Phase 4 — Audio (WASAPI)

**How this serves the primary goal:** audio sin retraso ni cortes; es la mitad de "transmitir video y audio".

- **T-4.1** Enumerar dispositivos de captura y render (nombre + id) con `wasapi`.
- **T-4.2** Captura en modo compartido, event-driven, hilo de prioridad alta.
- **T-4.3** Salida a dispositivo por defecto; anillo (ring buffer) lock-free entre captura y salida.
- **T-4.4** Volumen (0–1).
- **T-4.5** Offset de sincronización ±300 ms (retrasar/adelantar en el anillo). Portar la lógica de `AudioSyncBuffer.cs`.
- **T-4.6** Corrección de deriva (objetivo 50 ms, zona muerta ±15 ms, 1 ms por ajuste). *Test:* simular deriva sobre el anillo sin hardware.
- **T-4.7** Selección automática de audio según el video elegido (reusa T-1.6).

**Verificación:** 30 min de audio continuo sin chasquidos y con buffer estable en ≈50 ms. **Guarda:** nada de asignaciones ni locks en el callback de audio.

---

## Phase 5 — Interfaz mínima y ajustes

**How this serves the primary goal:** que el usuario cambie de dispositivo, volumen y sync sin tocar archivos, y que lo recuerde.

- **T-5.1** Selección de dispositivo de video y audio (lista simple).
- **T-5.2** Controles auto-ocultables: volumen, offset de audio, presets de render.
- **T-5.3** Presets de render: Performance (30 fps), Balanced (60), Quality (60 con filtro mejor).
- **T-5.4** Cargar y guardar ajustes (T-1.3) al abrir/cerrar.
- **T-5.5** Arranque automático con el último dispositivo guardado; si falla, mostrar estado claro (no `MessageBox` bloqueante).

**Verificación:** cerrar y abrir recupera dispositivos, volumen y offset. **Guarda:** no introducir dependencias pesadas de UI sin justificarlo (decisión egui vs propio se toma aquí si hace falta).

---

## Phase 6 — Ejecutable portable

**How this serves the primary goal:** cumple el requisito de "un .exe sin instalador ni registro".

- **T-6.1** Perfil release ya definido (LTO, `panic=abort`, `strip`); medir tamaño.
- **T-6.2** Enlazar CRT estáticamente (`-C target-feature=+crt-static`) para no exigir redistribuibles de VC++.
- **T-6.3** Icono y manifiesto (DPI-aware, sin pedir elevación) con `build.rs`/`winres`.
- **T-6.4** Probar en máquina limpia: ninguna instalación, ninguna escritura fuera de `%APPDATA%\Dockview`.
- **T-6.5** CI de release (GitHub Actions) que publique el `.exe`.

**Verificación:** `.exe` único, arranca en un Windows limpio sin diálogos de registro. **Guarda:** sin telemetría, sin red.

---

## Final phase — Verificación y retirada del C#

**How this serves the primary goal:** confirma el objetivo y deja un repositorio sin duplicidades.

- **T-V.1** `cargo test --workspace` y `cargo clippy --workspace -- -D warnings` en verde.
- **T-V.2** `grep` de `unwrap(`/`expect(` en rutas de hardware: debe quedar solo en tests.
- **T-V.3** Prueba de 1 h: sin crecimiento de memoria, sin deriva de audio, sin acumulación de latencia.
- **T-V.4** Comparación de latencia con la app C# (misma capturadora, misma consola).
- **T-V.5** Archivar el C# (rama `archive/csharp` o tag) y mover `rust/` a la raíz.
- **T-V.6** Actualizar README (URL de clonado, requisitos, build).

## Fuera de alcance por ahora

Backend D3DImage/DXGI exclusivo, G-Sync exclusivo, grabación, streaming, soporte multiplataforma.
