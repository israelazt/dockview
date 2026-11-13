# Plan de robustez y calidad (2026-11-13)

## Por qué

La revisión del 2026-11-13 encontró el proyecto sano (fmt, clippy y 421 tests en verde, capas respetadas) pero con huecos en tres sitios: **recuperación ante fallos** (audio, dispositivos, ajustes, GPU), **pruebas de `Core`** (ningún test lo construye) y **distribución** (sin CI, sin icono, sin registro en archivo). Este plan los cierra.

Todo lo que aquí se llama «fallo» sale de leer el código; ninguno se ha reproducido aún con hardware. Cada bloque incluye la prueba real que lo confirma.

## Cómo se trabaja

Igual que en `STUDIO_PLAN.md`:

- Cada tarea es atómica: un cambio pequeño, con sus tests, que deja `cargo fmt`, `cargo clippy --workspace --all-targets` sin avisos y `cargo test --workspace` en verde. **Una tarea = un commit + push.**
- Al terminar un **bloque** hay **pausa para revisar** antes del siguiente.
- Receta de `ARCHITECTURE.md`: regla pura en `domain` → puerto → servicio en `app` con doble falso → adaptador en `platform` → cableado en el binario. Guardas de 400 líneas y de dependencias vigentes.
- Si una tarea toca hilos, GPU, audio o grabación: prueba con la app real (`--video C7`, `--record`), y si toca la interfaz, `ui_shots.ps1` + `ui_compare.py`.

## Orden y relación con el modo Estudio

Los bloques 1 a 4 van primero: son la red de seguridad. Después se puede retomar el **bloque B de Estudio** y alternar con los bloques 5 a 8, que no dependen entre sí.

| Bloque | Tema | Depende de |
|---|---|---|
| 1 | Arnés de pruebas de `Core` | — |
| 2 | Audio y dispositivos que se recuperan solos | 1 |
| 3 | Ajustes a prueba de fallos | 1 |
| 4 | Integración continua y repositorio | — |
| 5 | Render: presets, pérdida de GPU, color | — |
| 6 | Distribución: registro, icono, ejecutable | 4 |
| 7 | Rendimiento (medir antes de tocar) | 6-1 para los registros |
| 8 | `Core` en servicios | 1 |

---

## Bloque 1 — Arnés de pruebas de `Core`

Hoy `Core::new` solo se llama desde `run.rs`. Son ~2.300 líneas (`dockview-app/src/core/*`) sin un test que las ejercite juntas. Este bloque no cambia el comportamiento.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 1-1 ✅ | `app`: sacar los dobles del replay (`World`, `FakeMachine`, `FakeBuffer`, `FakeMic`, `FakeHotkeys`) de `replay_service/tests.rs` a un módulo de pruebas compartido (`src/testing/replay.rs`, solo `#[cfg(test)]`) | Los 19 tests del replay pasan sin cambios |
| 1-2 ✅ | `app`: `testing/sources.rs` con `FakeSources` (`SourceMachine`) y sus manejadores (`FakeCapture`, `FakeScreen`, `FakePlayback`, `FakeLoopback`): anotan cada arranque y dejan fijar el estado desde el test | Test: el doble anota un `start_capture` y devuelve el estado que se le fijó |
| 1-3 ✅ | `app`: `testing/system.rs` con `FakeSystem` (`SystemProbe`), `FakeStats` (`StatsSink`, guarda lo recibido) y `RecordingHost` (`Host`, anota las peticiones) | Test de cada doble |
| 1-4 ✅ | `app`: `testing/rig.rs`: `Rig::new()` construye un `Core` con todos los dobles, `MemorySettingsStore` y una lista de dispositivos configurable | Test: un `Core` arranca con la primera capturadora y su audio emparejado |
| 1-5 ✅ | `app`: el reloj entra por parámetro: `Core::tick_at(now, sources_open)` (y `tick` lo llama con `Instant::now()`); `maintain_source` y `update_stats` dejan de leer el reloj | Tests existentes en verde; el `Rig` puede avanzar el tiempo |
| 1-6 ✅ | Test que faltaba en Estudio A-5: `ToggleStudio` alterna `studio_open` y lo guarda | Test en verde; marcar A-5 como cubierto en `STUDIO_PLAN.md` |
| 1-7 ✅ | Tests de arranque: se restaura el dispositivo guardado; gana el audio de la capturadora sobre el último usado (regresión de `31c45f6`); sin dispositivos no arranca nada | 3 tests |
| 1-8 ✅ | Tests de acciones: `SelectVideo` arrastra su audio; `SetVolume` llega al reproductor y se guarda; `ToggleReplay` grabando emite `StopRecordingFirst` | 3 tests |
| 1-9 ✅ | Tests de recuperación del vídeo: capturadora `Disconnected` → se reabre a los 2 s; pantalla `Ended` → vuelve a la capturadora y emite `SourceLost` | 2 tests |
| 1-10 ✅ | `ARCHITECTURE.md`: añadir el arnés a la receta («el servicio y su parte de `Core` se prueban con el `Rig`») | Doc al día |

**Cuidado:** la guarda de tamaño cuenta como producción todo archivo que no se llame `tests.rs`; cada archivo de `testing/` debe quedar bajo 400 líneas o la guarda se amplía, con su motivo, para esa carpeta.

**Pausa 1:** revisar que el `Rig` es cómodo de usar antes de escribir más tests sobre él.

**Hecho (2026-11-13).** Diferencias con lo previsto: el reloj entra por `tick_at` en `maintain_source`, `maintain_overlays` y `update_stats`; las transiciones de escena (`scene_transition`) y el servicio del replay (`replay_service`) aún leen `Instant::now()` y se inyectarán cuando un bloque los necesite. Los tests de conjunto viven en `core/tests.rs` (16 tests; el total de `dockview-app` pasó de 58 a 81).

---

## Bloque 2 — Audio y dispositivos que se recuperan solos

Hoy `maintain_source` (`core/sources.rs:238`) solo reconecta el vídeo, y los dispositivos se listan una vez en `run.rs:40`.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 2-1 | `app/sources.rs`: regla pura `audio_is_lost(&AudioStatus) -> bool` y reutilizar `Reconnect` para el audio | Tests de la regla |
| 2-2 | `app`: `Core::maintain_audio(now)`: si el reproductor está `Disconnected` o `Failed`, lo reinicia tras la pausa de `Reconnect`, y reengancha el tap de grabación | Tests con el `Rig`: se reinicia, respeta la pausa, conserva volumen y sincronía |
| 2-3 | `domain` + binario: `AppEvent::AudioLost` y `AudioBack`, sus textos en/es y su aviso en `toast_for` | Test de `toast_for` y guarda de i18n en verde |
| 2-4 | `platform/audio.rs`: el hilo de salida comprueba cada segundo el id de la salida por defecto y se reabre sobre la nueva si cambió | Prueba real: enchufar y quitar auriculares; el sonido sigue |
| 2-5 | `domain` + `platform`: puerto `SourceMachine::devices() -> DeviceLists`; en `platform`, un hilo enumera vídeo y audio cada 2 s y el puerto devuelve la última lista **sin bloquear** | Test del doble; la app arranca igual que antes |
| 2-6 | `run.rs` deja de enumerar: `Core::new` recibe las listas del puerto | `run.rs` más corto; tests en verde |
| 2-7 | `app/sources.rs`: regla pura `reconcile(viejos, nuevos, seleccionado)` → nuevo índice (por enlace simbólico o id, no por posición), aparecidos y desaparecidos | Tests: reordenar, quitar el seleccionado, añadir uno |
| 2-8 | `app`: `Core` aplica `reconcile` en cada `tick`; si no hay fuente y aparece una capturadora, la arranca con su audio | Tests con el `Rig`: enchufar tras arrancar muestra imagen sin reiniciar |
| 2-9 | Prueba real y anotarla en `CAPTURE_PLAN.md` (cierra el pendiente de la línea 116): desenchufar y reenchufar el USB viendo y grabando; arrancar sin capturadora y enchufarla | Resultado anotado |

**Pausa 2:** probar con la UGREEN y la pacoxi; decidir si los avisos de audio molestan o ayudan.

---

## Bloque 3 — Ajustes a prueba de fallos

`settings.json` ya guarda escenas y alias. Hoy se escribe con `fs::write` directo y un archivo ilegible carga los valores por defecto en silencio; el siguiente guardado lo pisa.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 3-1 | `platform/settings_store.rs`: guardado atómico (escribir `settings.json.tmp`, sincronizar, renombrar encima) | Tests: contenido correcto, no queda el temporal, un temporal viejo no estorba |
| 3-2 | `domain`: `AppSettings::try_from_json -> Result`; `from_json` se apoya en él | Tests de JSON válido, vacío y roto |
| 3-3 | `platform`: un archivo que existe pero no se puede leer se copia a `settings.corrupt.json` antes de devolver los valores por defecto | Test: el archivo roto se conserva |
| 3-4 | `domain` + `platform`: `SettingsStore::save` devuelve `Result`; `Core` registra el fallo y avisa una sola vez (`AppEvent::SettingsNotSaved`, textos en/es) | Tests con un almacén que falla |
| 3-5 | `app`: guardado agrupado: las acciones marcan los ajustes como «sucios» y `tick_at` guarda como mucho cada 500 ms; cerrar la ventana guarda siempre | Tests con el `Rig`: arrastrar el volumen 60 veces produce 1 o 2 guardados; al cerrar no se pierde nada |
| 3-6 | `domain`: recordar el monitor por su nombre de dispositivo (`\\.\DISPLAY1`) en vez de por posición; los archivos viejos con `lastMonitor` numérico siguen cargando | Tests de compatibilidad y de monitores reordenados |

**Pausa 3:** revisar un `settings.json` real antes y después.

---

## Bloque 4 — Integración continua y repositorio

| Tarea | Qué | Listo cuando |
|---|---|---|
| 4-1 | Los tests que necesitan GPU o codificador por hardware se saltan con un mensaje claro cuando no los hay (una función común `hardware_or_skip()` en `platform`) | `cargo test` pasa con la variable `DOCKVIEW_NO_HARDWARE=1` |
| 4-2 | `.github/workflows/rust.yml` en `windows-latest`: `fmt --check`, `clippy -D warnings`, `test --workspace`, con caché de cargo | El flujo pasa en GitHub en un push |
| 4-3 | Sacar `.claude/settings.local.json` de git y añadirlo a `.gitignore` | `git ls-files` ya no lo lista |
| 4-4 | Apuntar `origin` a `israelazt/dockview` | `git remote -v` muestra la URL nueva y el push funciona |
| 4-5 | `rust/README.md`: los cinco crates reales, cómo compilar, probar y lanzar | Coincide con `ARCHITECTURE.md` |
| 4-6 | `README.md` raíz: aviso de que el desarrollo vive en `rust/` y quitar `your-username`; `PLAN.md`: cabecera «histórico» con enlace a los planes vigentes | Sin instrucciones falsas. La reescritura completa espera al cambio de nombre a Luminifex |

**Pausa 4:** con CI en verde es buen momento para retomar el bloque B de Estudio.

---

## Bloque 5 — Render: presets, pérdida de GPU y color

| Tarea | Qué | Listo cuando |
|---|---|---|
| 5-1 | `domain`: `render_plan(preset) -> { filtro, fps_max }` sustituye a `filter_for` de `shell/helpers.rs`. Propuesta: Performance = vecino más cercano + tope de 30 fps; Balanced = bilineal; Quality = bicúbico | Tests de los tres presets |
| 5-2 | `shell`: aplicar el tope de fps de Performance (saltarse redibujados de vídeo, nunca los de la interfaz) | `--show-stats` marca ~30 fps mostrados en Performance |
| 5-3 | `platform`: sombreador bicúbico (Catmull-Rom) para NV12 y su `Filter::Bicubic` | Capturas: Balanced idéntico a hoy; Quality más nítido al escalar |
| 5-4 | `platform/render`: `draw`, `upload` y `resize` devuelven un error tipado que distingue `DeviceLost` (`DXGI_ERROR_DEVICE_REMOVED` / `RESET`) del resto | Test de la clasificación del código de error |
| 5-5 | `shell`: ante `DeviceLost`, recrear `Renderer` y `UiGpu` y volver a aplicar filtro y área de vídeo | Prueba real: `dxcap -forcetdr` (o reiniciar el driver con Win+Ctrl+Mayús+B); la imagen vuelve sola |
| 5-6 | `domain`: `ColorInfo { matriz, rango }` en `Frame` y regla pura para el valor por defecto (la de hoy: BT.709 desde 720 líneas, rango limitado) | Tests de la regla |
| 5-7 | `platform/capture.rs`: leer `MF_MT_YUV_MATRIX` y `MF_MT_VIDEO_NOMINAL_RANGE` del tipo negociado; si faltan, la regla por defecto | Test de la conversión de atributos |
| 5-8 | `render.hlsl`: usar matriz y rango del fotograma en vez de decidir por altura | Capturas del patrón de prueba idénticas; negros y blancos correctos con la capturadora real |

**Decisión para la pausa 5:** confirmar la propuesta de 5-1 o dejar solo dos presets.

---

## Bloque 6 — Distribución

Al quitar la consola desaparecen los `eprintln!`, así que el registro va antes.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 6-1 | `domain`: fachada de registro mínima (`log::info/warn/error` hacia un sumidero que fija el binario; sin sumidero no hace nada) | Tests con un sumidero en memoria |
| 6-2 | `platform`: sumidero a `%APPDATA%\Dockview\dockview.log`, que rota al llegar a 1 MB y sigue escribiendo a la consola si la hay | Tests de escritura y rotación |
| 6-3 | Sustituir todos los `eprintln!` de producción por la fachada, y guarda en `architecture.rs` que los prohíbe fuera de tests y ejemplos | Guarda en verde |
| 6-4 | `shell`: si Direct3D o la ventana no arrancan, mostrar un cuadro de mensaje con el motivo en vez de cerrarse en silencio | Prueba forzando el error |
| 6-5 | Binario: `windows_subsystem = "windows"` solo en release, y engancharse a la consola del padre cuando se lanza desde una terminal (`--screenshot`, `--log-stats` siguen imprimiendo) | Doble clic sin ventana negra; `ui_shots.ps1` sigue funcionando |
| 6-6 | `build.rs` en el binario: icono y manifiesto (DPI por monitor v2, sin elevación). La dependencia de compilación se añade a la guarda de dependencias | El `.exe` muestra el icono; nítido al 150 % |
| 6-7 | `tools/`: guion que recorta `lucide.ttf` a los iconos usados e Inter a Latin; test que comprueba que cada icono usado existe en la fuente | Ejecutable más pequeño (hoy 5,3 MB, 2,2 MB son fuentes); capturas idénticas |
| 6-8 | `.github/workflows/release.yml`: al crear una etiqueta, compila en release y adjunta `dockview.exe` | Una etiqueta de prueba produce el artefacto |
| 6-9 | Prueba en un Windows limpio (T-6.4 de `PLAN.md`): arranca sin instalar nada y solo escribe en `%APPDATA%\Dockview` | Resultado anotado |

**Pausa 6:** probar el `.exe` de la etiqueta en otra máquina.

---

## Bloque 7 — Rendimiento

Primero medir. Lo que no mejore las cifras no se fusiona.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 7-1 | Línea base con `--log-stats`, 5 min cada una: solo ver, grabando, escena con esquina, y arranque en frío. Cifras al final de este documento | Tabla escrita |
| 7-2 | `platform/audio.rs`: los hilos de audio piden prioridad «Pro Audio» a Windows (MMCSS); comprobar la firma en la documentación de `windows` antes de usarla | Prueba real con la CPU cargada: sin chasquidos; cifras de 7-1 sin empeorar |
| 7-3 | `app`: `Core::model()` sin trabajo repetido: nombres de dispositivo en caché (se recalculan al cambiar las listas o los alias) e idioma resuelto una vez | Tests de `model` iguales; menos reservas por fotograma |
| 7-4 | `platform`: compilar los sombreadores en `build.rs` e incrustar el resultado | Arranque en frío más corto que en 7-1; capturas idénticas |
| 7-5 | **Exploración:** decodificar MJPEG por hardware (`MF_SOURCE_READER_D3D_MANAGER`) en un ejemplo `mjpeg_probe`; medir CPU y latencia contra hoy | Cifras y decisión escritas: seguir o descartar |
| 7-6 | **Exploración:** mantener pantallas y escenas en la GPU (textura compartida entre compositor y renderer, sin `Nv12Target::publish` hacia la CPU) en un ejemplo; medir latencia | Cifras y decisión; si compensa, plan propio |

**Pausa 7:** decidir con las cifras qué exploraciones se convierten en trabajo real.

---

## Bloque 8 — `Core` en servicios

`Core` es una estructura de ~35 campos repartida en archivos con `use super::*`. `ARCHITECTURE.md` dice que cada servicio es dueño de su estado; hoy solo `ReplayService` lo cumple. Con el arnés del bloque 1 este cambio es seguro. No cambia el comportamiento.

| Tarea | Qué | Listo cuando |
|---|---|---|
| 8-1 | `app`: `SourcesService` con el estado de dispositivos (`video_devices`, `audio_devices`, índices, `monitors`, `windows`) y sus consultas | Tests del `Rig` en verde; tests propios del servicio |
| 8-2 | Mover a `SourcesService` la fuente activa y su mantenimiento (`source`, `source_kind`, `Reconnect`, arrancar captura o pantalla) | Ídem |
| 8-3 | Mover a `SourcesService` el audio (`audio`, `loopback`, `route_audio`, `maintain_audio`) | Ídem |
| 8-4 | `app`: `ScenesService` con `scene`, `overlays`, `scene_base` y `transition` y su avance por `tick` | Ídem |
| 8-5 | `Core` queda como coordinador: ajustes, eventos, composición del `Model` y reparto de `Action`s. Quitar los `use super::*` que ya no hagan falta | `Core` con menos de 15 campos |
| 8-6 | `ARCHITECTURE.md` y la memoria del proyecto al día | Docs al día |

**Pausa 8:** revisión final.

---

## Fuera de este plan

- Codificador H.264 por software cuando no hay uno por hardware: hoy no hay replay ni grabación en ese caso. Se valora después de 7-1.
- Ganancia de audio por encima del 100 %: aparcada por decisión del autor.
- Retirar el C# de la raíz y mover `rust/` arriba: fase final de `PLAN.md`, junto con el cambio de nombre a Luminifex.

## Riesgos

1. **El arnés cristaliza el diseño actual.** Si los tests del bloque 1 miran campos internos de `Core`, el bloque 8 los rompe todos. Los tests deben usar solo la superficie pública (`apply`, `on_key`, `tick_at`, `model`, `take_events`) y lo que anotan los dobles.
2. **Enumerar dispositivos cuesta.** Media Foundation tarda y exige un hilo MTA; por eso 2-5 lo hace en un hilo propio y el puerto nunca bloquea el bucle de la ventana.
3. **Los corredores de GitHub no tienen capturadora ni, quizá, codificador por hardware.** 4-1 va antes que 4-2 por eso.
4. **Sin consola, un fallo de arranque es invisible.** 6-1 a 6-4 van antes que 6-5.
5. **El recorte de fuentes puede dejar un icono fuera.** 6-7 incluye el test que lo detecta.
