# Captura desde el propio PC, webcam y escenas — plan

Objetivo: que Dockview sea la herramienta única para grabar y transmitir (LoL y otros juegos en este mismo PC, capturadora, webcam), con un editor rápido. Se avanza por pasos y **cada paso se mide antes de construir encima**. No se persigue la amplitud de OBS; se compite en sencillez, replay siempre activo, editor vertical, bajo consumo y diagnóstico claro.

## Orden

| Paso | Qué | Listo cuando |
|---|---|---|
| 1 ✅ | Micrófono en su pista del replay | Un clip trae mezcla, juego y micrófono |
| 2 | **Spike** (hecho salvo la prueba con LoL): captura de pantalla/ventana + audio del sistema o del proceso, sin interfaz | Se conoce el coste en FPS, CPU y la sincronía con LoL abierto |
| 2b | Spike de la Canon M50 Mark II como webcam | Se sabe si aparece en Media Foundation, y con qué resolución, FPS y retraso |
| 3 | Fuentes y compositor | Dockview graba la pantalla con el replay actual |
| 4 | Grabación normal (inicio/fin) con codificador por hardware | Una partida entera sin perder FPS |
| 5 | Editor rápido (recorte, exportación horizontal y vertical) | Un clip listo para redes en menos de un minuto |
| 6 | Streaming (RTMP) y escenas | Transmitir con una escena de dos capas |

## Forma que debe tener desde ya (aunque el paso 3 llegue después)

- **Fuente de video** = algo que publica frames en **su propio** `FrameSlot` (capturadora, pantalla, ventana, webcam). Hoy hay una sola; el modelo final es una lista.
- **Fuente de audio** = algo que entrega bloques a un `AudioTap` (capturadora, micrófono, audio del sistema, audio de un proceso). Cada una acaba en su pista.
- **Compositor** (paso 3): dibuja las fuentes en una textura de salida en la GPU. El visor y el recorder leerán esa salida, no el slot de una fuente. Mientras solo haya una fuente, la salida es esa fuente tal cual.
- Los frames de pantalla nacen como **texturas en la GPU** (BGRA). Convertirlos a NV12 en la CPU cuesta demasiado; el camino bueno es GPU → NV12 → codificador por hardware sin pasar por la RAM. El spike mide cuánto se pierde si se hace por CPU, para decidir si hace falta.

## Spike 2: qué mide y cuándo falla

Herramienta: `cargo run --release -p dockview-core --example screen_probe -- [opciones]`.

Mide, durante N segundos:
1. **Captura** (Windows Graphics Capture) de un monitor o de una ventana por título: FPS entregados, intervalo entre frames (media, p99, máximo) y CPU del proceso.
2. Con `--readback`: lo mismo copiando cada frame a la CPU, el caso pesimista.
3. **Audio** por loopback del sistema o solo del proceso del juego: bloques por segundo, nivel y desfase entre el primer frame y el primer bloque.

Criterios (con LoL abierto, medidos por el usuario en su PC):
- El FPS de LoL no baja más de ~3 % con la captura activa (se compara mirando el contador de FPS del juego con y sin el spike).
- La captura sola (sin readback) usa menos de ~5 % de un núcleo; con readback se anota el valor real.
- El audio del proceso llega sin cortes y sin mezclar otros programas.

Riesgos a vigilar: el anti-trampas de LoL (la captura de monitor suele funcionar; la de ventana hay que probarla), el borde amarillo de captura en Windows, y que el loopback por proceso exige Windows 10 2004 o posterior.

## Resultados del spike (2026-11-12, GTX 1660 Ti, monitor 144 Hz)

Con la ventana de Dockview (60 fps reales de la UGREEN) y con Edge reproduciendo video:

| Medida | Resultado |
|---|---|
| Captura por defecto de Windows | **~48 fps** (Windows se salta frames si llegan a menos de ~16 ms) |
| Con `SetMinUpdateInterval` a 1 ms | **60,1 fps**, intervalo mediano 14,6 ms, p99 22,6 ms |
| CPU de la captura (solo GPU) | ~1–2 % de un núcleo |
| CPU con copia de cada frame a la CPU | ~2–3 % de un núcleo. **Solo mide copiar y mapear el frame**; convertir BGRA→NV12 en la CPU (necesario para el codificador actual) no está medido y será bastante más caro |
| Audio del sistema (loopback) | Llega continuo, 100 bloques/s, sin cortes |
| Audio de un solo programa (loopback por proceso) | Capta Edge (RMS 0,053) y da **silencio exacto** en un programa que no suena (WhatsApp): es selectivo |
| Desfase del primer bloque de audio frente al primer frame | +14 a +20 ms (despreciable; se corrige con el reloj común, como ya hacemos) |

Lecciones para el diseño:
1. **Hay que pedir `MinUpdateInterval`** o se pierde un 20 % de los frames. Requiere Windows 11 22H2; en versiones anteriores se queda en el valor por defecto.
2. **La captura solo entrega frames cuando la imagen cambia** (el escritorio quieto dio 18 fps). El grabador necesita un **ritmo constante**: repetir el último frame si no llega otro, y descartar los sobrantes si la pantalla va a 144 Hz.
3. El **audio por proceso** permite grabar el juego sin cuelar Discord, el navegador ni las notificaciones. Hay que dar el PID del proceso dueño de la ventana (y su árbol).
4. Copiar el frame a la CPU cuesta poco, pero **falta medir la conversión BGRA→NV12**. Antes de usar el codificador actual con pantalla hay que medirla (con SIMD o en la GPU con el procesador de video de D3D11); si pasa de ~5 % de un núcleo, se hace en la GPU.

Falta la prueba decisiva, que solo se puede hacer con el juego abierto (anti-trampas de LoL y su efecto en los FPS):

```
screen_probe.exe --window "League of Legends" --audio process --min-interval-ms 1 --seconds 60
screen_probe.exe --window "League of Legends" --audio process --min-interval-ms 1 --seconds 60 --readback
```

Si LoL corre en pantalla completa exclusiva y la ventana no se captura, repetir sin `--window` (captura del monitor) y con `--audio system`.
