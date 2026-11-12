# Captura desde el propio PC, webcam y escenas — plan

Objetivo: que Dockview sea la herramienta única para grabar y transmitir (LoL y otros juegos en este mismo PC, capturadora, webcam), con un editor rápido. Se avanza por pasos y **cada paso se mide antes de construir encima**. No se persigue la amplitud de OBS; se compite en sencillez, replay siempre activo, editor vertical, bajo consumo y diagnóstico claro.

## Orden

| Paso | Qué | Listo cuando |
|---|---|---|
| 1 ✅ | Micrófono en su pista del replay | Un clip trae mezcla, juego y micrófono |
| 2 | **Spike** (hecho salvo la prueba con LoL): captura de pantalla/ventana + audio del sistema o del proceso, sin interfaz | Se conoce el coste en FPS, CPU y la sincronía con LoL abierto |
| 2b | Spike de la Canon M50 Mark II como webcam | Se sabe si aparece en Media Foundation, y con qué resolución, FPS y retraso |
| 3 | Fuentes y compositor | Dockview graba la pantalla con el replay actual |
| 4 | **Grabación normal** (inicio/fin): ver la sección de abajo | Una partida entera sin perder FPS |
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

## Grabación normal (paso 4) — plan

Caso de uso: alguien se sienta a jugar, graba la partida (para sacar un clip o hacer una guía) y sube el MP4 a YouTube. Atajo **Ctrl+Alt+R**; sin pre-roll en la primera versión.

Decisiones: se codifica **una sola vez** (la grabación reutiliza los frames comprimidos del replay); el archivo es un **MP4 fragmentado**, que sigue siendo válido si el programa se cierra mal; el audio lleva siempre **tres pistas** (mezcla, juego, micrófono) para que encender el micrófono a mitad de una guía no se pierda; la calidad se ofrece como «Normal» y «Alta», sin hablar de bitrates; funciona con cualquier fuente que alimente al recorder (capturadora hoy, pantalla cuando llegue el paso 3).

| Fase | Qué | Estado |
|---|---|---|
| G-1 | Escritor de archivo en el núcleo: empieza en un fotograma clave, video y tres pistas en directo | ✅ |
| G-2 | Botón, atajo Ctrl+Alt+R, cronómetro, punto rojo y avisos | ✅ |
| G-3 | Casos incómodos: poco espacio en disco, señal que se corta, cambio de resolución, sin señal al iniciar, cerrar la app grabando | ✅ (falta la prueba con el cable de la capturadora) |
| G-4 | Ajuste de calidad, carpeta y acceso a la última grabación | ✅ |
| G-5 | Prueba de 1 hora con la capturadora: CPU, GPU, RAM, deriva de audio y tamaño | pendiente |

### Resultados de G-1 (UGREEN 25173, 1080p60)

- Grabación de 39 s: **2354 frames, 60,0 fps exactos**, audio y video terminan con 20 ms de diferencia, 126 fragmentos, tres pistas de audio.
- CPU del proceso grabando (captura + codificación + audio + micrófono + archivo): **~25 % de un núcleo**.
- **Corte forzado** (proceso matado a los ~14 s): el archivo conserva **12,07 s de video y las tres pistas de audio completas** hasta el último fragmento. Windows lo abre como video 1920×1080 pero no sabe su duración (la cabecera de duración se escribe al cerrar); los reproductores lo reproducen igualmente.
- El bitrate real depende de la imagen: con una pantalla casi estática salió ~1 Mbps aunque el objetivo era 20 Mbps. Con movimiento real subirá; hay que medirlo en G-5 con juego.
- Herramientas: `record_live` (graba N segundos de la capturadora) y `file_check` (cuenta muestras por pista, sin decodificar).

### Resultados de G-2

- **Botón** en la píldora, junto a «guardar clip»: círculo rojo en reposo, cuadrado rojo (que parpadea mientras arranca) al grabar. **Atajo global Ctrl+Alt+R**, que funciona con otra ventana en primer plano.
- **Indicador siempre visible** arriba a la derecha (punto rojo que late una vez por segundo y cronómetro `MM:SS`, o `H:MM:SS` desde la primera hora). Solo se dibuja en la ventana de Dockview, nunca en el video grabado. Sigue en pantalla aunque la píldora se oculte.
- **Avisos**: «Grabación iniciada», «Grabación guardada», «No se pudo grabar».
- Los archivos se llaman `Dockview Recording AAAA-MM-DD HH-MM-SS.mp4` (los clips sueltos siguen siendo `Dockview AAAA-…`) y van a la misma carpeta que los clips.
- Si el replay estaba apagado, pulsar grabar lo enciende y empieza en cuanto el codificador está listo. Mientras se graba, apagar el replay se rechaza con un aviso (cortaría la grabación).
- Cerrar la app grabando finaliza el archivo (con su índice final), no lo deja a medias.
- Verificado con la UGREEN: el atajo inició y detuvo una grabación de 8,28 s, 497 frames (60 fps) y tres pistas de audio.
- Flags de prueba: `--record` (empieza a grabar al abrir) y `--mic`.

### Resultados de G-3

| Caso | Qué hace Dockview | Cómo se comprobó |
|---|---|---|
| **Poco espacio en disco** | Comprueba cada 2 s el disco del archivo. Avisa una vez por debajo de 5 GB libres («Poco espacio en disco…»), y **cierra el archivo con limpieza por debajo de 1 GB** («Grabación detenida: el disco está casi lleno»). No deja empezar con menos de 2 GB libres | Test con el umbral elevado (no se llenó un disco de verdad) |
| **La señal se corta y vuelve** | La grabación **sigue**, es un solo archivo y el video **mantiene el último frame durante el corte**; el audio rellena con silencio, así que no se desincronizan | Test que apaga la fuente 3 s y la vuelve a encender |
| **La imagen cambia de tamaño** | Un archivo no puede cambiar de resolución: cierra el actual y **sigue en `… (parte 2).mp4`** (y así sucesivamente), con aviso | Test con una fuente que pasa de 640×360 a 800×448: los dos archivos se leen |
| **Se pide grabar y no llega video** | A los 10 s desiste con «No hay señal de video para grabar» | Test |
| **Se cancela antes del primer frame** | Se cancela sin mensaje de error | Test |
| **Se cierra la app grabando** | El archivo se finaliza (con su índice final) | Prueba con captura de pantalla (G-2) |
| **El disco no se puede escribir** | Cierra la grabación y avisa; lo escrito hasta el último fragmento sigue válido | Cubierto por el diseño fragmentado (G-1) |

**Error real encontrado y corregido:** el escritor de MP4 de Windows **ignora los huecos entre frames** y los comprime, mientras que el audio conserva su propia línea de tiempo. Tras un corte de señal de 3 s, el video terminaba 3 s antes que el audio (video 6,27 s frente a 9,30 s de grabación). Ahora `Mp4Remuxer` retiene cada frame hasta ver el siguiente y, si hay un hueco, lo alarga hasta cubrirlo. Esto también protege a los clips del replay.

Pendiente de probar a mano: desenchufar y volver a enchufar el cable USB de la capturadora durante una grabación. Se desconoce aún si el audio de la capturadora vuelve solo tras reconectar (la reconexión del video ya existía; la del audio no está verificada).

### Resultados de G-4

- **Calidad de grabación** (en el popover del replay): **Normal** (16 Mbps, ~7 GB por hora) y **Alta** (40 Mbps, ~18 GB por hora). Es la misma opción para el búfer del replay y las grabaciones, porque comparten el codificador. Alta sigue siendo el valor por defecto, como antes. El popover muestra «Una grabación ocupa unos X por hora».
- Cambiar la calidad **reinicia el búfer del replay** (un codificador nuevo no puede continuar el anterior); avisa con «Calidad cambiada: el búfer del replay empieza de nuevo», conserva el micrófono encendido, y se rechaza mientras se graba.
- **Carpeta**: muestra la ruta y tiene dos botones, «Abrir carpeta» y **«Cambiar…»**, que abre el selector de carpetas de Windows. Comprueba que se pueda escribir en la carpeta elegida; si no, avisa y no la cambia. Vale para clips y grabaciones.
- **Acceso a lo último guardado**: «Abrir carpeta» abre el Explorador con **el último clip o grabación de la sesión seleccionado**; si no hay ninguno (o se movió), abre la carpeta.
- Verificado con clics reales en la app: Normal → `replayBitrateMbps = 16` y la app sigue viva; «Cambiar…» abre el selector nativo y la carpeta elegida queda guardada.
- No verificado: el aspecto visual de la calidad Normal frente a Alta con juego real, ni cuánto pesa de verdad una hora a 16 Mbps con movimiento (G-5).
