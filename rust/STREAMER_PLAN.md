# Dockview para gamers y streamers — qué tenemos y qué falta (2026-11-14)

Objetivo (dicho por el autor): que Dockview sea **la aplicación completa para gamers streamers**. Este documento compara lo que hay con lo que un streamer usa a diario (OBS Studio, Streamlabs, Elgato 4K Capture Utility, Medal, ShadowPlay) y ordena lo que falta. El detalle de cada pieza irá a su propio plan cuando se empiece.

## Para quién

| Perfil | Qué necesita | Ventaja que ya tenemos |
|---|---|---|
| **Streamer de consola** (capturadora + PC aparte o el mismo) | Ver el juego sin retraso, emitir, grabar, voz y música separadas | Visor de capturadora de ~1-2 ms, 1080p60, audio de la propia capturadora, sincronía ajustable |
| **Streamer de PC** (juego en el mismo equipo) | Emitir con poco coste de CPU/GPU, escenas, webcam, alertas | Captura de pantalla/ventana por GPU, compositor en la GPU, escenas con morph |
| **Creador de clips** (TikTok/Shorts/YouTube) | Guardar lo que acaba de pasar y publicarlo rápido | Repetición instantánea, clips con atajo, capturador de pantallazos |

## Lo que tenemos

| Área | Estado | Notas |
|---|---|---|
| Ver la capturadora en vivo | ✅ | Baja latencia, 3 filtros de escalado, estadísticas y diagnóstico en lenguaje llano |
| Audio de entrada | ✅ | Capturadora, micrófono, audio del sistema o de un programa (loopback) |
| Grabar | ✅ | MP4 fragmentado con vídeo y **3 pistas de audio** (juego, micrófono, mezcla), calidad normal/alta, recuperable si se corta |
| Repetición instantánea | ✅ | Búfer de 30 s a varios minutos, guardar clip con atajo global |
| Escenas | ✅ | Varias fuentes (capturadora, webcam, monitor, ventana) en capas libres, editor de capas, orden, morph o corte, atajos globales |
| Pantallazos | ◐ | Capturador tipo Herramienta Recortes: captura, selección, atajo, bandeja, avisos. Falta el editor (S-D) |
| Interfaz | ✅ | Píldora, modo Estudio con panel, claro/oscuro, inglés y español, animaciones |
| Ejecución | ✅ | Solo Windows hoy; instalador listo (no se instala hasta el final) |

## Lo que falta (agrupado por lo que un streamer espera)

Prioridad: **P0** = sin esto no es una app de streaming; **P1** = lo que hace competitiva la experiencia; **P2** = diferenciadores y comodidad; **P3** = largo plazo. Tamaños: S una sesión, M un bloque, L varios bloques.

### A. Emitir en directo — P0
| # | Qué | Tamaño | Notas |
|---|---|---|---|
| A-1 | **Salida RTMP(S)** a Twitch, YouTube y Kick: clave de emisión, servidor, codificador de hardware en CBR, intervalo de keyframe de 2 s, audio AAC | L | Es lo más importante que falta. Hoy codificamos para MP4, no para red |
| A-2 | Ajustes de emisión: resolución, FPS, bitrate, perfil, con **recomendaciones por plataforma** | M | Twitch/YouTube/Kick piden cosas distintas |
| A-3 | Salud de la emisión: bitrate real, fotogramas perdidos, estado de la red, **reconexión automática**, bitrate adaptativo | M | Que un corte de red no tire el directo |
| A-4 | Emitir y grabar a la vez (dos codificadores, o uno compartido) con coste medido | M | La mayoría graba mientras emite |
| A-5 | Botón/atajo/estado de emisión en la píldora y el panel, cuenta atrás, aviso de «en directo» | S | |
| A-6 | SRT y WHIP/WebRTC (menor latencia, mejor con redes malas) | M | Después de RTMP |
| A-7 | Emisión múltiple a varias plataformas | L | P2 |

### B. Audio de streamer — P0/P1
| # | Qué | Tamaño |
|---|---|---|
| B-1 | **Mezclador**: un deslizador, silencio y medidor por fuente (juego, micrófono, música, Discord, audio del sistema, aplicaciones concretas) | M |
| B-2 | Escucha por auriculares (monitoreo) y sincronía por fuente | M |
| B-3 | Filtros del micrófono: **supresión de ruido**, puerta de ruido, compresor, limitador, ecualizador | M |
| B-4 | Pistas de audio configurables en directo y en grabación; **pista aparte para el VOD sin música con derechos** | M |
| B-5 | Atajos «push to talk» y «silenciar» | S |

### C. Fuentes y superposiciones (lo que hace un directo «de streamer») — P0/P1
| # | Qué | Tamaño | Notas |
|---|---|---|---|
| C-1 | Fuentes de **texto**, **imagen**, **vídeo/GIF** y **color** en las escenas | M | Hoy solo hay capturadora, monitor y ventana |
| C-2 | **Fuente de navegador** (para alertas de StreamElements/Streamlabs, chat, widgets) con WebView2 en Windows | L | Es lo que usa casi toda la comunidad; sin ella no valen sus herramientas |
| C-3 | **Chat integrado** (Twitch IRC/EventSub, YouTube Live Chat, Kick) y lectura en pantalla | L | Panel propio y/o fuente en la escena |
| C-4 | Alertas propias (seguidor, suscripción, donación) si no se quiere depender de terceros | L | P2 |
| C-5 | Filtros de capa: recorte, **chroma key**, corrección de color, nitidez, máscara | M | |
| C-6 | **Webcam**: fondo desenfocado o eliminado, encuadre automático, espejo | L | P1 |
| C-7 | Grupos de capas, bloqueo, alinear y distribuir, capas ocultas por atajo | S | Mejoras del editor de escenas |
| C-8 | **Plantillas de escenas** («Juego + cámara», «Hablando», «Volvemos enseguida», «Fin») | S | Para empezar en un minuto |

### D. Distribución y plataformas — P1
| # | Qué | Tamaño |
|---|---|---|
| D-1 | **Cámara virtual** (para Discord, Zoom, Teams, navegadores) | M |
| D-2 | **Conectar cuentas** (Twitch/YouTube/Kick): iniciar sesión, cambiar título y categoría, marcar el directo, crear clips en la plataforma | L |
| D-3 | Control remoto: API local (compatible con lo que usa Stream Deck y los mandos) y atajos para todo | M |
| D-4 | Asistente de primera vez (detectar capturadora y micrófono, probar la red, recomendar ajustes) | M |

### E. Clips y contenido — P1/P2
| # | Qué | Tamaño |
|---|---|---|
| E-1 | **Editor de clips**: recortar, marcar el mejor momento, miniatura | L |
| E-2 | **Exportar vertical** (9:16) para TikTok/Shorts/Reels, con encuadre automático de cámara y juego | L |
| E-3 | Marcadores durante el directo y la grabación («esto es bueno») | S |
| E-4 | Subida directa de clips y VODs | M |
| E-5 | Capturas y GIF desde el mismo flujo (en curso: S-C a S-E) | M |

### F. Automatización y juego — P2
| # | Qué | Tamaño |
|---|---|---|
| F-1 | **Detección de juego** y cambio automático de escena | M |
| F-2 | Eventos y reglas («al empezar el directo, silencia la música») | M |
| F-3 | Superposición dentro del juego (frames, tasa de fotogramas, chat) sin impactar el rendimiento | L |
| F-4 | Captura del juego «directa» (hook de juego) para PC con mínimo coste, además de pantalla y ventana | L |
| F-5 | HDR → SDR con tonemapping correcto | M |

### G. Plataforma y producto — P1/P3
| # | Qué | Tamaño |
|---|---|---|
| G-1 | Plugins de la comunidad (el plan ya está escrito, `PLUGINS_PLAN.md`) | L |
| G-2 | Linux (requisito de 1.0 por D4); iOS fuera | L |
| G-3 | Actualizaciones automáticas y canales estable/beta | M |
| G-4 | Perfiles y colecciones de escenas, importar desde OBS | M |

## Cómo se ordena (propuesta)

La brecha más grande es **emitir**. Todo lo demás (mezclador, superposiciones, chat, cámara virtual) importa solo cuando hay un directo que mejorar. Orden sugerido, después de la Fase 0 de pruebas con tu hardware y del capturador (S-D, S-E):

1. **Emitir** (A-1 a A-5): el mínimo para un directo real.
2. **Audio de streamer** (B-1, B-3, B-5) y **fuentes básicas** (C-1, C-8).
3. **Fuente de navegador** (C-2) — desbloquea alertas y chat de terceros — y **chat** (C-3).
4. **Cámara virtual** (D-1) y **cuentas** (D-2).
5. Webcam (C-6), clips (E-1, E-2), automatización (F-1), plugins (G-1).

**Un hito razonable, «Dockview 1.0 para streamers»:** A-1 a A-5, B-1/B-3/B-5, C-1/C-2/C-8, D-1, G-3 y Linux. Sin cuentas conectadas ni chat propio (el chat llega por la fuente de navegador y las herramientas de terceros).

## Qué cambia en la arquitectura

- **Salida de red** (A): un nuevo puerto `StreamOutput` en `domain` (con `RtmpOutput` en Windows y Linux) y un codificador de baja latencia compartido con el de grabación; la grabación y la emisión leen el mismo slot de salida del compositor, así que **el compositor y los slots ya valen**.
- **Mezclador** (B): hoy el audio se mezcla en una pista fija; hace falta un grafo de audio por fuente, con medidores, y un servicio `audio_mixer` en `app`.
- **Fuente de navegador** (C-2): WebView2 en Windows, WebKitGTK/CEF en Linux, detrás de un puerto `BrowserSource` que entrega frames a un `FrameSlot` como cualquier fuente.
- **Plugins** (G-1): conviene hacerlos **antes** de C-3/C-4 para que chat y alertas puedan ser plugins en vez de código propio.
- **Linux** (G-2): el renderizador y el codificador siguen atados a Direct3D 11 y Media Foundation; poner A-1 sobre esos tipos concretos duplicaría el trabajo. Por eso 5-0 (puertos del renderizador y el codificador) va **antes** de A-1.

## Diseño de la emisión (A-1), fijado en 5-0e

El codificador de vídeo (`platform::encode::H264Encoder`, Media Foundation) ya entrega un tipo neutro del dominio, **`EncodedFrame`** (H.264 en Annex B con marca de tiempo, duración y si es fotograma clave). Hoy lo consumen el búfer de la repetición y el escritor de MP4; **emitir es un consumidor más**. De ahí sale esta forma:

```
 compositor ──► slot de salida ──► VideoEncoder (plataforma) ──► EncodedFrame ─┬─► búfer de repetición
                                                                               ├─► escritor MP4 (grabación)
 audio (mezclador) ──► AudioEncoder AAC (plataforma) ──► EncodedAudio ────────┤
                                                                               └─► StreamOutput ──► Transport (TCP/TLS)
```

- **Puerto `VideoEncoder`** (dominio): configuración (resolución, FPS, bitrate, **CBR**, intervalo de fotograma clave de 2 s, baja latencia) y `encode(frame) -> EncodedFrame`, más `set_bitrate()` para el bitrate adaptativo. Windows: Media Foundation (NVENC/QuickSync/AMF/software); Linux: VAAPI/NVENC/x264.
- **Puerto `AudioEncoder`** (dominio): PCM → AAC (o Opus para WHIP/SRT). Hoy el AAC lo hace el escritor de MP4 por dentro; para emitir hay que sacarlo como un codificador aparte (el transformador AAC de Media Foundation lo permite).
- **`StreamOutput`** (servicio portátil, sin sistema operativo): empaqueta vídeo y audio codificados en **FLV**, habla **RTMP** (apretón de manos, fragmentos, comandos `connect`/`publish`), cuenta bitrate real y fotogramas perdidos y **reconecta** con retroceso. Es lógica pura sobre un `Transport`.
- **Puerto `Transport`**: abrir, enviar y cerrar una conexión (RTMP sobre TCP, RTMPS sobre TLS); la implementación real usa `std::net` y `rustls`, **igual en Windows y en Linux**. Se pone en un crate nuevo, `dockview-stream`, que depende solo del dominio: así la parte de red se escribe una vez.
- **Compartir el codificador**: grabar y emitir leen el mismo `EncodedFrame` cuando tienen la misma calidad; si piden calidades distintas, dos codificadores (A-4, medido).
- **Salud**: el servicio publica `bitrate`, `dropped`, `rtt` en un `StreamStats` que la interfaz muestra y que el bitrate adaptativo (A-3) usa.

**Estado de A-1 (2026-11-14):** hecho el lado portátil, probado contra un servidor RTMP falso y por un socket real en local: puertos `VideoEncoder`/`AudioEncoder`, `EncodedAudio` y `StreamStats` en el dominio; crate `dockview-stream` con Annex B→AVCC, cargas FLV, AMF0, chunks, `Publisher` (apretón de manos, `connect`, `createStream`, `publish`), `StreamOutput` (reconexión con retroceso 1→30 s, espera de keyframe, bitrate real, fotogramas perdidos) y `TcpConnector`. **TLS hecho** (`rtmps://` con rustls + ring y las raíces públicas de webpki-roots; probado con un certificado propio y rechazando uno no confiable). **Falta:** los adaptadores de Windows (codificador CBR y AAC aparte), el hilo que lo une al compositor, y probarlo contra un servidor real.

**Riesgos de este diseño:** el AAC hay que desacoplarlo del escritor de MP4 (cambio en el grabador); el CBR estricto y el tamaño de GOP dependen del codificador de hardware de cada GPU (se medirá en la 1660 Ti y en otro equipo); y RTMP de Twitch pide ciertos detalles (Enhanced RTMP para HEVC/AV1) que se dejan para después de H.264.

## Riesgos

1. **Mantener las plataformas de directo** (Twitch, YouTube y Kick cambian API y condiciones): C-2 (navegador) y D-2 (cuentas) dependen de terceros.
2. **Rendimiento a la vez**: emitir + grabar + repetición + escenas con una GTX 1660 Ti; se mide en cada paso (como hasta ahora).
3. **Alcance**: OBS tiene 10 años y cientos de colaboradores. La estrategia es **sencillez, poco consumo y el visor de capturadora**, no igualar cada función.
4. **Derechos de música** en el VOD (B-4) y de transmisión de juegos: no es técnico pero los usuarios lo notan.

## Preguntas para el autor

1. ¿El público principal es el **streamer de consola con capturadora**, el **de PC**, o los dos por igual? (cambia si F-4, captura directa del juego, es P1 o P3).
2. ¿Qué plataformas primero: Twitch, YouTube, Kick?
3. ¿Se quiere **chat y alertas propios** o basta con la fuente de navegador y las herramientas de terceros?
4. ¿Cuenta como requisito de la versión 1.0 la **emisión múltiple**, la **cámara virtual** o el **editor de clips**?
5. ¿Hay un modelo de negocio (gratis, de pago, donaciones)? Afecta a firma de código, actualizaciones y soporte.
