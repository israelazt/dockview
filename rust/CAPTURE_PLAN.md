# Captura desde el propio PC, webcam y escenas — plan

Objetivo: que Dockview sea la herramienta única para grabar y transmitir (LoL y otros juegos en este mismo PC, capturadora, webcam), con un editor rápido. Se avanza por pasos y **cada paso se mide antes de construir encima**. No se persigue la amplitud de OBS; se compite en sencillez, replay siempre activo, editor vertical, bajo consumo y diagnóstico claro.

## Orden

| Paso | Qué | Listo cuando |
|---|---|---|
| 1 ✅ | Micrófono en su pista del replay | Un clip trae mezcla, juego y micrófono |
| 2 | **Spike** (hecho salvo la prueba con LoL): captura de pantalla/ventana + audio del sistema o del proceso, sin interfaz | Se conoce el coste en FPS, CPU y la sincronía con LoL abierto |
| 2b | Spike de la Canon M50 Mark II como webcam | Se sabe si aparece en Media Foundation, y con qué resolución, FPS y retraso |
| 3 | **Captura de pantalla y de aplicaciones** (fuentes): ver la sección de abajo | Dockview graba un monitor o una app con su audio y el micrófono |
| 4 | **Grabación normal** (inicio/fin): ver la sección de abajo | Una partida entera sin perder FPS |
| 5 | Editor rápido (recorte, exportación horizontal y vertical) | Un clip listo para redes en menos de un minuto |
| 6 | Escenas (6a, plan abajo) y streaming (RTMP) | Transmitir con una escena de dos capas |

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

## Paso 3 — Captura de pantalla y de aplicaciones: plan

Lo que se quiere: capturar **un monitor completo o una aplicación concreta**, con el **audio de todo el sistema o solo de esa aplicación**, **más el micrófono si está activo**, y que el replay y la grabación funcionen igual que con la capturadora.

### Decisiones de diseño
1. **Todo entra como frames NV12 en el mismo `FrameSlot`** que usa la capturadora. Así el visor, el replay, la grabación, las partes por cambio de resolución y todo lo hecho en G-1..G-4 funciona sin tocarse.
2. **La conversión BGRA→NV12 se hace en la GPU** con el procesador de video de D3D11 (también escala y rellena con barras negras). A la RAM solo baja el frame NV12 (unos 3 MB por frame a 1080p), así que la CPU casi no se toca. Esto cierra la duda abierta del spike (convertir en CPU habría sido caro).
3. **Tamaño de salida fijo**: el del origen al empezar, par y con tope de 1080 px de alto. Si una ventana se redimensiona, la imagen se reescala dentro del mismo lienzo: la grabación no se parte en partes.
4. **Ritmo**: se pide a Windows 11 un intervalo mínimo entre frames ligeramente menor que el objetivo (hallazgo del spike: por defecto se pierde ~20 %). Windows entrega el último estado aunque la pantalla se quede quieta.
5. **Audio**: monitor → loopback del **sistema**; ventana → loopback **solo del proceso** de esa app (y sus procesos hijos). Va a la pista «juego» del replay y la grabación, **sin reproducirse** (el audio ya suena por sí solo). El micrófono sigue siendo su propia pista.
6. **La ventana de Dockview se excluye de la captura** cuando captura pantalla, para que la vista previa no se refleje a sí misma.

### Fases
| Fase | Qué | Se comprueba con |
|---|---|---|
| S-1 ✅ | Núcleo: listar monitores y ventanas; `ScreenCapture` (monitor o ventana → NV12 por GPU); captura de audio por loopback del sistema o de un proceso; herramienta `screen_record` | Grabar Edge y un monitor con audio; medir FPS, CPU y sincronía; ver que los colores son correctos |
| S-2 ✅ | App: elegir la fuente (capturadora, monitor, ventana), enrutar el audio y excluir la ventana de Dockview | Con clics reales |
| S-3 ✅ | Casos: ventana cerrada o minimizada, el programa se cierra, monitor que cambia, cursor | Pruebas |
| S-4 | Prueba con LoL (anti-trampas, FPS del juego, coste) | La haces tú |

### Resultados de S-1 (GTX 1660 Ti, dos monitores 1080p)

| Prueba | Resultado |
|---|---|
| Ventana de Edge (1920×1032) con audio **solo de Edge** + micrófono | 13,7 s grabados. El audio del proceso llegó (RMS 0,054) y la mezcla lo lleva. Video y audio terminan con 50 ms de diferencia |
| Monitor 1 con video en movimiento (audio del **sistema**) | **53 fps**, 18 Mbps, **32 % de un núcleo** (captura + conversión + codificación + audio) |
| Monitor 0 (escritorio casi quieto) | 18 fps (la captura solo entrega frames cuando algo cambia), 7 % de un núcleo |
| Colores | Correctos: se convirtió el primer frame NV12 a PNG y los tonos de piel y la geometría son los esperados (la conversión BGRA→NV12 de la GPU respeta BT.709 limitado y la disposición de planos) |
| Frames entregados | Siempre de tamaño par, NV12, tope de 1080 px de alto; `stride == width` |

Los frames de pantalla pasan por el mismo `FrameSlot`, así que el visor, el replay y la grabación (G-1..G-4) no cambiaron. Hay 7 tests del módulo (tamaños, encaje con barras negras, listado de monitores y ventanas, y una captura real del monitor principal).

**Error del micrófono encontrado en esta fase y corregido.** Al grabar vi que la pista del micrófono salía en silencio absoluto. La causa: en el PC del autor la **entrada predeterminada de Windows es «HDMI (UGREEN 25173)», la propia capturadora**, y «Predeterminado de Windows» la abría como micrófono. Ahora el modo **«Automático»** usa la predeterminada salvo que parezca una capturadora (HDMI, capture, UGREEN, Elgato, AVerMedia, Magewell, Cam Link, Stereo Mix…) o sea el dispositivo del sonido del juego; entonces elige la mejor entrada restante, un micrófono real antes que el de una webcam. En este PC elige el **RØDE NT-USB Mini**. Verificado: la pista del micrófono pasó de RMS 0,0000 a 0,0015 con ruido ambiente y pitidos, y la mezcla lo incluye.

### Resultados de S-2

- **Botón de fuente** en la píldora (icono de monitor, a la izquierda de Ajustes). Abre un popover con tres listas: **Capturadora** (los dispositivos de siempre), **Pantalla** («Pantalla 1 · 1920×1080 (principal)», «Pantalla 2 …») y **Ventana de aplicación** (las 8 ventanas más al frente; se actualiza cada 1,5 s mientras está abierto; no lista las ventanas de Dockview). Elegir una cambia la fuente al instante y el popover sigue abierto.
- **Audio enrutado solo**: con la capturadora suena y se graba el audio de la tarjeta (como siempre); con un **monitor** se graba el **sonido del sistema**; con una **ventana**, **solo el sonido de ese programa**. En pantalla y ventana Dockview **no reproduce** ese audio (ya suena solo) y detiene el de la tarjeta. El micrófono sigue siendo su propia pista. Al volver a la capturadora, su audio se reanuda.
- **La ventana de Dockview se excluye de la captura** mientras captura pantalla (`WDA_EXCLUDEFROMCAPTURE`) y se restablece al volver a la capturadora.
- Si **la ventana capturada se cierra o no se puede capturar**, sale un aviso y se vuelve a la capturadora.
- Se recuerda el **último monitor** (no las ventanas, que son volátiles): si la última sesión fue un monitor, arranca en él. Para arrancar directamente: `--screen N` (monitor) o `--window "texto del título"`.
- El replay, la grabación, el micrófono, las partes por cambio de resolución y el aviso de disco funcionan igual con cualquier fuente.
- Verificado con la app real: `--window YouTube --record` capturó Edge (58 fps en el visor), grabó con el audio solo de Edge (RMS 0,11); con clics reales se pasó de la ventana a una cámara de video y la marca de selección se movió. 4 tests nuevos de los controles (acciones, un solo popover a la vez, cabe en 720p con muchas ventanas).
- **Límites conocidos**: la lista de ventanas muestra solo las 8 más al frente; un juego en **pantalla completa exclusiva** puede no capturarse (usar modo ventana sin bordes); no se ha probado con LoL ni con su anti-trampas (S-4).

### Resultados de S-3 (sin cambios de código: todo se comportó bien)

| Caso | Cómo se probó | Resultado |
|---|---|---|
| **Cerrar Dockview mientras graba** | `--test-pattern --record`, cierre normal de la ventana (`CloseMainWindow`, como la X) a los 9 s | La app salió en **0,22 s** y el MP4 es legible (191 frames, 6,35 s). El `Drop` del grabador cierra el archivo |
| **La ventana capturada se cierra durante una grabación** | `--window` sobre un formulario que se cierra solo, con `--record` | Sale el aviso, la app vuelve a la capturadora y **la grabación sigue** en el mismo archivo: 28 s, audio y video terminan juntos (1497 frames) |
| **La ventana capturada se minimiza durante una grabación** | `screen_record`, minimizada 6 s a mitad | Archivo continuo de 24,96 s: audio y video terminan con 20 ms de diferencia; al restaurar, la captura se reanuda sola. Mientras está minimizada el video queda en el último frame (Windows no entrega frames) |
| **Monitor o ventana que cambia de tamaño** | Revisión del código (`screen.rs`): al cambiar `ContentSize` se recrean los buffers y la imagen se reescala al tamaño fijo del video | No se provocó un cambio real de resolución en el monitor del autor |
| **Cursor** | `ScreenOptions::cursor` (activado por defecto) | Sigue visible en la captura; no hay interruptor en la interfaz |

Detalles a tener en cuenta:
- La captura de pantalla **solo entrega frames cuando algo cambia**, así que con una ventana quieta el recuento de frames es bajo (es normal; el archivo mantiene la duración correcta).
- Al perderse la fuente en plena grabación, el video pasa a la capturadora sin cortar el archivo; si no hubiera capturadora o señal, actúa el aviso de «sin señal» de G-3.

## Paso 6a — Escenas: plan (decidido 2026-11-12)

Lo que se quiere: componer **varias fuentes a la vez** (por ejemplo el juego a pantalla completa y la cámara o la capturadora en una esquina), guardar esas composiciones como **escenas** y cambiar entre ellas. El visor, el replay y la grabación ven solo el resultado.

### Decisiones de diseño
1. **Varias fuentes vivas**, cada una con su propio `FrameSlot`. Hoy `Source` (en `main.rs`) es una sola; pasa a ser una lista de fuentes con identificador.
2. **Compositor en la GPU** (`dockview-core`): sube el frame NV12 de cada fuente como textura, dibuja las capas en una textura BGRA, la convierte a NV12 con el procesador de video de D3D11 (el mismo camino de S-1) y la publica en un **slot de salida**. El visor, el replay y la grabación leen ese slot: G-1..G-4 y S-1..S-3 no se tocan.
3. **Atajo sin coste**: una escena de una sola capa a pantalla completa no pasa por el compositor; el frame se reenvía tal cual (igual que hoy).
4. **Modelo**: una escena es una lista ordenada de capas; cada capa tiene fuente, rectángulo **libre y normalizado** (x, y, ancho, alto de 0 a 1), ajuste (llenar, encajar, recortar) y visible/oculta. Se guarda en `settings.json`. El tamaño de salida es fijo (1080p), así no se parte la grabación al cambiar de escena.
5. **Audio**: igual que ahora (pista del juego, micrófono y mezcla). Cada escena indica de qué fuente viene el sonido principal.
6. **Editor libre** (decidido por el autor, estilo OBS): arrastrar y redimensionar cada capa sobre la vista previa, con asas y ajuste a bordes y centro. Más trabajo que las plantillas, pero el modelo con rectángulos libres lo admite desde el principio.

### Fases
| Fase | Qué | Se comprueba con |
|---|---|---|
| E-1 ✅ | Compositor sin interfaz: dos fuentes (pantalla/ventana a pantalla completa + capturadora en una esquina); herramienta de prueba que graba y mide | FPS, CPU y GPU; colores y posiciones correctas; coste con LoL abierto |
| E-2 | La app mantiene varias fuentes a la vez; el compositor alimenta visor, replay y grabación | Grabar una escena con clics reales |
| E-3 | Interfaz: selector de escenas y editor libre sobre la vista previa | Clics reales y capturas |
| E-4 | Cambio de escena con tecla global y transición suave opcional | Pruebas |

### Riesgos
- Más GPU y RAM con dos fuentes más el compositor, en una GTX 1660 Ti con el juego abierto: se mide en E-1 antes de seguir.
- Una captura de pantalla baja a RAM como NV12 y vuelve a subir a la GPU (unos 180 MB/s a 1080p60): debería aguantar; si no, la fuente de pantalla publicará su textura directamente.
- La Canon M50 II (spike 2b) entra después como una fuente más, sin tocar el compositor.

### Resultados de E-1 (GTX 1660 Ti, dos monitores 1080p)

`crates/dockview-core/src/compose.rs`: `Compositor` (dibuja capas en una textura NV12 con el procesador de video de D3D11, sin shaders), `SceneCompositor` (lo ejecuta en un hilo y publica en un `FrameSlot` de salida), `Layer`, `NormRect` y `Fit` (llenar/encajar/recortar). Herramienta de medición: `cargo run --release -p dockview-core --example compose_probe -- --main monitor:1 --pip monitor:0 --seconds 15`.

| Prueba | Resultado |
|---|---|
| Orden y posición de las capas | Test con GPU real: la capa de abajo se ve donde la de arriba no está, la de arriba cubre su cuarto de pantalla y una capa oculta desaparece. La entrada YUY2 (capturadoras sin comprimir) también funciona |
| Monitor 1 a pantalla completa + monitor 0 en la esquina (grabado 15 s) | Colores correctos y el recuadro cae abajo a la derecha. Archivo continuo de 14,5 s a 54,5 fps (la fuente principal entrega 49,8) |
| Coste (captura de dos pantallas + composición + codificación) | **40 % de un núcleo**, frente al 32 % de una sola pantalla en S-1: la composición añade en torno a un 8 % |
| Monitor 1 + capturadora UGREEN en la esquina | Funciona, pero la capturadora estaba sin señal (recuadro negro); no se validó con imagen real de la consola |

**Hallazgos**
- Las texturas de entrada NV12 necesitan `BIND_SHADER_RESOURCE | BIND_RENDER_TARGET`; con solo `SHADER_RESOURCE` Windows rechaza la vista de entrada (`E_INVALIDARG`).
- Sin tope, dos fuentes de 60 fps producían **110 imágenes por segundo** (64 % de un núcleo y el codificador trabajando el doble). El compositor se limita a `OUTPUT_FPS` = 60 y dibuja solo cuando alguna fuente visible tiene un frame nuevo.
- Nada de esto toca todavía la app: el visor y el replay siguen leyendo una sola fuente (E-2).
- **Pendiente de medir**: GPU y FPS del juego con LoL abierto (lo mide el autor).

## Segunda capturadora: C7 USB3.0 (2026-11-12)

Capturadora muy económica (VID 345F, PID 2131, USB 3). Se midió con la Switch conectada.

| Dato | Resultado |
|---|---|
| Modos | NV12 hasta **1080p a 50 fps** (sin 1080p60); 60 fps solo hasta 1280×720 (NV12 y MJPEG); YUY2 solo 10 fps a 1080p |
| FPS reales | Exactos en 1080p50, 1080p30, 720p60 y 720×480@60, con 0 frames perdidos |
| Color | Rango completo correcto: negros 0-4, blancos 255 |
| Coste de Dockview a 1080p50 | 25 % de un núcleo (2,1 % del total), 1,4 ms de latencia propia |
| Prueba de juego real (autor) | Audio y video sincronizados, una misión de campaña sin problemas; se nota el límite de 50 fps y una ligera pérdida de calidad. Sirve para jugar en casa |

Pensada también como entrada de la **Canon M50 Mark II por HDMI limpio** (una cámara rara vez pasa de 30 fps, así que el tope de 50 no importa). Pendiente el spike 2b: modos, FPS reales y retraso de la cámara.

**Fallo encontrado y corregido:** el audio de la C7 se llama «Interfaz de sonido digital (2- C7 USB3.0 Audio)», sin ninguna palabra que delate una capturadora, y Windows lo puso como entrada predeterminada. Con la UGREEN en uso, el modo «Automático» habría elegido ese audio como micrófono (pista muda). Ahora `pick_microphone` recibe todos los dispositivos de video y descarta el audio de **cualquier** capturadora que no sea una webcam (la webcam sigue valiendo como último recurso). Verificado con la app real: con la C7 como entrada predeterminada elige el RØDE.

## Tooltips (2026-11-12)

Cada control explica qué hace al dejar el ratón encima medio segundo (aparece con un fundido corto; desaparece al pulsar o al moverse a otro control).

- **Botones de la píldora**: fuente, repetición instantánea, guardar clip (con los segundos y el atajo, o «actívala para guardar clips» si está apagada), grabar (cambia según el estado: empezar / cancelar / detener, con Ctrl+Alt+R), ajustes, pantalla completa (con F o Esc), volumen y los dos botones de sincronía de audio (adelantar o retrasar el sonido, con las flechas).
- **Dentro de los popovers**: interruptor del replay, menos o más tiempo, micrófono, abrir o cambiar la carpeta, calidad Normal o Alta, modo de video Auto o Comprimido, estadísticas y los tres modos de imagen. Salen **a la izquierda del popover** para no tapar las filas vecinas; los de la píldora salen encima del botón. Siempre dentro de la ventana.
- El botón que acaba de abrir un popover no muestra tooltip.
- Las filas de dispositivos, monitores y ventanas no llevan tooltip: su etiqueta ya lo dice todo.
- Textos en español e inglés (27 nuevos), probados para que no quede ningún `{marcador}` sin rellenar.
- Corregido de paso: el texto del micrófono en español se salía del popover; ahora es más corto.
