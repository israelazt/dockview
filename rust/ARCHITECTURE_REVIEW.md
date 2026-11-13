# Inspección del proyecto: qué se puede desacoplar (2026-11-12)

Revisión de solo lectura de los dos crates (`dockview-core` y `dockview-app`, ≈ 22.800 líneas con tests y ejemplos). Se midieron tamaños, funciones largas, qué toca qué campo de `App`, funciones duplicadas, dependencias entre módulos y densidad de tests.

## Veredicto

**La arquitectura de base es sana; los problemas están en cuatro puntos concretos.** El núcleo (`dockview-core`) no depende de la interfaz, y dentro de la app las dependencias van en un solo sentido, sin ciclos. Lo que se ha vuelto difícil de mantener es:

1. **`App` en `main.rs`** es un objeto «dios»: 64 campos, 75 métodos, casi sin tests.
2. **Código repetido en el núcleo** (hilos, GPU, estado), que cada fuente nueva copia otra vez.
3. **`recorder.rs`** (1553 líneas) mezcla cuatro responsabilidades.
4. **Las cadenas de texto** (`i18n.rs`) obligan a editar cuatro sitios por cada mensaje nuevo.

## Cifras

| Medida | Resultado |
|---|---|
| Archivos más grandes | `main.rs` 2014 · `recorder.rs` 1553 · `compose.rs` 901 · `screen.rs` 885 · `encode.rs` 795 · `i18n.rs` 734 |
| `App` (`main.rs`) | **64 campos, 75 métodos**; los más largos tocan hasta 21 campos a la vez (`update_stats`, 118 líneas; `render`, 19 campos; `apply`, 166 líneas) |
| Campo más compartido | `settings`: lo usan 31 métodos; luego `cli` (16) y `recorder` (12) |
| Funciones de más de 150 líneas | `draw_replay_popover` 339 · `draw_pill` 204 · `apply` 166 · `screen::run` 164 · `App::new` 163 · `draw_popover` 152 |
| Densidad de tests | `main.rs` **0,2 por 100 líneas** (4 tests en 1961); `scene_runtime.rs`, `scene_actions.rs`, `audio.rs`, `gpu.rs` y las piezas de la píldora y los popovers: **0** |
| Dependencias entre módulos de la app | Sin ciclos. Una inversión: `stats_log → ui` (un registro de datos depende de un tipo de la interfaz) |

## Hallazgos

### A. `App` es un objeto «dios» (el más importante)
Cinco responsabilidades viven mezcladas en una sola estructura:

| Responsabilidad | Campos / métodos | Aprox. |
|---|---|---|
| **Fuentes y audio** | `source`, `source_kind`, `video_devices`, `audio_devices`, `video_index`, `audio_index`, `audio`, `monitors`, `windows`, `loopback`, `input_slot`; `select_video`, `select_audio`, `start_screen`, `route_audio`, `maintain_source`, `restore_screen`… | ≈ 350 líneas |
| **Replay, grabación y micrófono** | `recorder`, `saving`, `hotkey`, `mic`, `record_when_ready`, `last_saved`, `low_disk_warned`, `mic_level`…; `sync_replay`, `toggle_record`, `begin_recording`, `set_mic`, `poll_replay`, `save_replay`, `restart_recorder`… | ≈ 400 líneas |
| **Estadísticas y diagnóstico** | `stats`, `sampler`, `fps`, `fallback`, `switch_check`, `latency_*`; `update_stats` (118), `judge_switch`, `switch_to_compressed`… | ≈ 200 líneas |
| **Escenas** | `scene`, `overlays`, `scene_base` (ya en `scene_runtime.rs` y `scene_actions.rs`) | ≈ 600 líneas, bien separadas |
| **Ventana, dibujo y eventos** | `window`, `renderer`, `ui`, `controls`, `pointer`, `toasts`…; `render`, `window_event`, `apply`, `model` | ≈ 600 líneas |

Síntomas: `apply` es un `match` gigante que conoce a todos los demás; los métodos que mezclan estado de varios mundos (`update_stats` toca 21 campos) son los que no se pueden probar sin abrir una ventana; y cualquier funcionalidad nueva (escenas, transmisión, plugins) acaba tocando `main.rs`.

**Propuesta:** repartir `App` en **servicios que poseen su estado** (`Sources`, `ReplayService`, `StatsService`; `Scenes` ya casi lo es), cada uno con sus métodos y su propio archivo, y con una interfaz pequeña. `App` solo los coordina. Los servicios devuelven **eventos** («grabación terminada», «fuente perdida») en vez de llamar a `toast` directamente; la interfaz decide cómo mostrarlos. Ese mismo bus de eventos es el paso P-3 del plan de plugins.

### B. Código repetido en `dockview-core`
| Qué se repite | Dónde | Cuánto |
|---|---|---|
| **Arrancar un hilo, guardar un `stop`, un `status` y esperarlo al soltar** (`Drop` con tiempo límite) | `ScreenCapture`, `VideoCapture`, `SceneCompositor`, `ReplayRecorder`, `TapCapture`, `AudioEngine` | 6 copias del `Drop` y 6 `set_status` |
| **Crear el dispositivo D3D11 con protección multihilo** | `screen.rs`, `compose.rs` (más `render.rs` y un ejemplo con variantes) | 3 copias |
| **Textura NV12, lectura a la RAM y publicación del frame**, `even`, `rect` | `screen.rs` (`Converter`) y `compose.rs` | ≈ 120 líneas casi idénticas |
| **Cola acotada que descarta lo más viejo** | `FrameTap` (`frame.rs`) y `AudioTap` (`pcm.rs`) | 2 copias del concepto |
| `cpu_seconds` | 3 ejemplos | 3 copias |

**Propuesta:** un `Worker` (hilo + parada + estado + cierre limpio) y un módulo `gpu` compartido (dispositivo, textura NV12, lectura al `Frame`). Es la base común que usaría cualquier fuente nueva, incluidas las de plugins.

### C. `recorder.rs` (1553 líneas, 927 de código)
Mezcla: la API pública de `ReplayRecorder` (≈ 290 líneas), el hilo del codificador (`run`, 130), el guardado de clips (`write_clip`, `SaveHandle`) y las sesiones de grabación normal (`RecCtx` y siete funciones `rec_*`, ≈ 250). **Propuesta:** `recorder/` con `mod.rs`, `thread.rs`, `session.rs`, `clip.rs`, y los tests (≈ 620 líneas) en su propio archivo.

### D. Tamaño de lo que queda en `compose.rs` y `screen.rs`
- `compose.rs`: geometría de capas (pura, sin Windows) + `Compositor` + hilo + tests. La geometría puede vivir junto al modelo de escena y probarse sin GPU.
- `screen.rs`: listado de monitores y ventanas + conversor GPU + hilo de captura. El listado es independiente de la captura.

### E. Interfaz: lo que queda grande
`draw_replay_popover` (339 líneas), `draw_pill` (204) y `draw_popover` (152) construyen todo en una sola función. **Propuesta:** secciones como componentes pequeños (un `fn` por sección, apoyados en `widgets.rs`). Además, `Stats` (un tipo de datos de medidas) debería vivir fuera de `ui` para que `stats_log` no dependa de la interfaz.

### F. Cadenas de texto (`i18n.rs`, 734 líneas)
Cada mensaje nuevo exige **cuatro ediciones**: el `enum`, la lista `ALL_KEYS` (con su longitud), el `match` en inglés y el `match` en español. Una macro que declare cada clave una sola vez, con sus dos textos, lo deja en una. También facilitaría que otros idiomas se añadan como datos.

### G. Pruebas
La lógica de decisión más delicada de la app (qué hacer al perder una fuente, cuándo avisar de poco disco, cuándo cambiar a MJPEG, cómo enrutar el audio) está en `main.rs` y casi sin tests porque necesita una ventana para ejecutarse. Al extraerla en servicios con entradas y salidas simples se puede probar sin ventana.

## Plan de desacoplamiento (priorizado)

Cada paso: sin cambio visible, comprobado con los mismos tests y, en la interfaz, con capturas comparadas píxel a píxel; commit y push al terminar.

| Paso | Qué | Valor | Esfuerzo / riesgo |
|---|---|---|---|
| D-1 ✅ | `Worker` común (hilo, parada, estado, cierre) en el núcleo | Quita 7 copias; base para fuentes nuevas | Bajo |
| D-2 ✅ | Módulo `gpu` compartido (dispositivo, textura NV12, lectura al `Frame`) | Quita ≈ 120 líneas duplicadas | Bajo-medio |
| D-3 | Partir `recorder.rs` en `recorder/` | Un archivo por responsabilidad | Medio (mucho movimiento, pocos cambios) |
| D-4 | Extraer `Sources` de `App` (fuentes, audio y enrutado) | Mayor reducción de `main.rs`; permite probar el enrutado | Medio-alto |
| D-5 | Extraer `ReplayService` (replay, grabación, micrófono) y `StatsService` | `App` queda como coordinador | Medio-alto |
| D-6 | Capa de eventos entre servicios y la interfaz | Quita el `toast` de la lógica; base del bus de plugins | Medio |
| D-7 | Macro de textos en `i18n.rs` | De cuatro ediciones a una | Bajo |
| D-8 | Dividir las funciones de dibujo grandes y mover `Stats` fuera de `ui` | Interfaz más fácil de ampliar | Bajo-medio |

**Orden recomendado:** D-1, D-2, D-7 (rápidos y sin riesgo), luego D-4 y D-5 con D-6 (el núcleo del desacoplamiento), y D-3 y D-8 cuando estorben. Cada uno se puede intercalar con funciones nuevas sin bloquearlas.

### Resultado de D-1 (2026-11-12)
`dockview-core/src/worker.rs`: `Worker` (uno o varios hilos con una bandera de parada y un plazo de cierre; al soltarlo espera y, si un hilo sigue bloqueado en el controlador, lo deja terminar solo) y `StatusCell<T>` (estado compartido que no se envenena). Migrados `TestSource`, `VideoCapture`, `ScreenCapture`, `SceneCompositor`, `TapCapture`, `ReplayRecorder` y `AudioEngine`: siete `Drop` y tres `set_status` menos. 5 tests nuevos. Comprobado con los 351 tests y con la app real (capturadora + pantalla en la esquina + grabación; cierre en 326 ms, vídeo bien cerrado).

Cambio pequeño de comportamiento: `TestSource` esperaba a su hilo sin límite; ahora espera como máximo 2 s como las demás. `AudioEngine`, `TapCapture` y `ReplayRecorder` siguen con su estado propio dentro de `Shared`, porque guarda además otras cosas (ganancia, nivel, anillos).

### Resultado de D-2 (2026-11-12)
`dockview-core/src/gpu.rs`: `create_device` (dispositivo con soporte de BGRA y de vídeo, protegido para varios hilos), `nv12_texture`, `Nv12Target` (textura de salida, su vista, la copia de lectura y `publish`, que devuelve la imagen como `Frame`), más `even`, `rect` y `failed`. `screen.rs` (`Converter`) y `compose.rs` (`Compositor`) dejaron de llevar cada uno su copia. 3 tests nuevos, uno con la GPU real.

Se quedaron fuera a propósito `render.rs` y el ejemplo `screen_probe`: crean el dispositivo con otras opciones (sin soporte de vídeo, nivel de características fijo), así que compartirlo obligaría a parametrizarlo sin ganar nada. Comprobado con 354 tests y con la app real (capturadora + pantalla en la esquina + grabación, cierre en 512 ms, vídeo de 13,7 MB).

## Lo que no conviene tocar
- La separación en dos crates y el sentido de las dependencias.
- `Model` y `Action` como frontera entre la lógica y la interfaz: funcionan bien.
- El modelo de escena (`scene.rs`) y `FrameSlot` con redirección: son pequeños y probados.
- Los ejemplos de medición (`*_probe`, `*_record`), salvo para compartir su `cpu_seconds`.
