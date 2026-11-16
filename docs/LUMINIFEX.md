# Luminifex — qué es y qué objetivo tiene

> **Sobre el nombre.** El producto se llamará **Luminifex**. Hoy el proyecto, el código, el ejecutable y la carpeta de datos todavía se llaman *Dockview*, y seguirán así hasta el final del desarrollo (ver «El renombrado» al final). Este documento habla del producto con su nombre futuro; cuando aparezca «Dockview» es porque se habla del código tal como está hoy.

## En una frase

**Luminifex es la aplicación completa, ligera y abierta para gamers y streamers: ve tu capturadora o tu pantalla sin retraso, graba, guarda lo que acaba de pasar, compone escenas, captura pantallazos y emite en directo, sin cuentas obligatorias ni telemetría.**

## Por qué existe

Nació como un visor mínimo: ver la salida HDMI de una capturadora en el PC con la menor latencia posible, para jugar a una Nintendo Switch «en modo televisión», sin abrir un programa de producción entero. Ese visor (primero en C#/WPF) funcionaba, y de ahí salieron las preguntas que lo han ido haciendo crecer: «¿y si pudiera guardar lo que acaba de pasar?», «¿y grabar con la voz aparte?», «¿y componer la cámara y el juego en una escena?», «¿y emitir?». La migración a Rust (más rápida, más pequeña y portable) fue la ocasión de hacerlo bien desde la base.

La idea que sostiene todo: **las herramientas de streaming son potentes pero pesadas y complicadas; las sencillas se quedan cortas.** Luminifex quiere ser lo que se abre y funciona, y que crece contigo.

## Objetivo

**Misión.** Que cualquiera que juegue pueda **verse, guardarse y compartirse** con calidad profesional y con el menor coste posible para su equipo y para su tiempo.

**Visión.** Ser la aplicación de referencia para gamers streamers que valoran la sencillez, la baja latencia y el bajo consumo: desde quien solo quiere ver su consola en el monitor hasta quien emite cada noche con escenas, mezcla de audio y alertas.

**Qué significa «completa».** Cubrir, sin salir de la aplicación, todo lo que un streamer hace a diario: ver, grabar, repetir, componer, mezclar, emitir, capturar y compartir (ver `STREAMER_PLAN.md`).

## Principios

1. **Sencillez primero.** Se abre y funciona. Lo avanzado está ahí pero no estorba; la interfaz aparece cuando hace falta y se esconde sola.
2. **Baja latencia y poco consumo, medidos.** No es una promesa: cada cambio se mide (hoy, ~1–2 ms de latencia interna al ver la capturadora, 1080p60, y unos 54 MB en la bandeja con la repetición apagada). Si un cambio empeora una cifra, se dice.
3. **Local y privado.** Todo se queda en tu equipo. Sin cuentas obligatorias, sin telemetría, sin enviar nada tuyo. Las conexiones a plataformas (Twitch, YouTube…) las inicias tú y las puedes cortar.
4. **Abierta.** Código abierto (licencia MIT) y pensada para recibir plugins de la comunidad, con una interfaz pública estable y la seguridad pensada desde el principio.
5. **Multiplataforma por diseño.** Windows hoy; Linux antes de la 1.0; iOS fuera del alcance. Nada del sistema operativo fuera de una capa propia, detrás de puertos neutros.
6. **Robusta.** Una capturadora que se desenchufa, un disco que se llena o un driver que se reinicia no tiran la aplicación ni pierden lo grabado.
7. **Para todos.** Claro y oscuro, inglés y español, HiDPI, respeto a los ajustes de animación y de contraste del sistema.

## Para quién

| Perfil | Qué necesita | Qué le damos |
|---|---|---|
| **Jugador con capturadora** (consola en el monitor) | Ver sin retraso, con el sonido de la capturadora | El visor de baja latencia, sincronía de audio ajustable |
| **Streamer de consola** | Emitir y grabar, voz y música separadas | Grabación de 3 pistas, escenas, (próximamente) emisión y mezclador |
| **Streamer de PC** | Emitir con poco coste, escenas, cámara y alertas | Captura de pantalla y ventana por GPU, compositor, (próximamente) fuentes de navegador |
| **Creador de clips** | Guardar lo que acaba de pasar y publicarlo rápido | Repetición instantánea, capturador de pantallazos, (próximamente) editor de clips y formato vertical |

## Qué hace (los pilares)

| Pilar | Qué es | Estado |
|---|---|---|
| **Ver** | Mostrar la capturadora, un monitor o una ventana en vivo, con estadísticas y un diagnóstico en lenguaje llano | ✅ |
| **Grabar** | MP4 recuperable con vídeo y 3 pistas de audio (juego, micrófono, mezcla), en calidad normal o alta | ✅ |
| **Repetir** | Un búfer de segundos a minutos; con un atajo global guardas lo que acaba de pasar | ✅ |
| **Componer** | Escenas con varias fuentes en capas libres (capturadora, cámara, pantalla, ventana), editor sobre la imagen, transiciones suaves o corte, cambio por atajo | ✅ |
| **Capturar pantallazos** | Equivalente a la Herramienta Recortes: rectángulo, ventana, pantalla completa y forma libre; atajo global, bandeja, aviso | ◐ falta el editor |
| **Mezclar audio** | Una mezcla por fuente con medidores, supresión de ruido, puerta, compresor | ⏳ |
| **Emitir** | Directo a Twitch, YouTube y Kick con salud de red y reconexión | ⏳ |
| **Extender** | Plugins de la comunidad (fuentes, efectos, salidas, automatización) | ⏳ plan escrito |

## Lo que Luminifex **no** es

- **No es un clon de OBS Studio.** No intenta igualar sus cientos de funciones; intenta ser más sencilla, más ligera y mejor en lo suyo.
- **No es un editor de vídeo completo.** Recorta clips y prepara formato vertical; no sustituye a Premiere o DaVinci.
- **No es un servicio en la nube.** No tiene cuentas propias ni guarda tu contenido fuera de tu equipo.
- **No es solo para streamers.** Quien solo quiere ver su consola en el monitor también es un usuario de primera.

## Cómo está hecha (en pocas líneas)

Un programa en **Rust** organizado en capas con dependencias solo hacia abajo (una guarda automática lo comprueba):
- **domain**: las reglas y los datos puros (escenas, ajustes, selección, atajos, textos), sin Windows ni disco, con tests.
- **platform**: lo que toca el sistema (captura, audio, codificador, GPU, atajos, bandeja). Implementa los **puertos** que define el dominio.
- **app**: el núcleo con los servicios, probado con dobles falsos de cada puerto, sin hardware.
- **ui**: la interfaz propia, dibujada en la GPU con un motor pequeño (fuentes Inter y Lucide, animaciones, claro/oscuro).
- **binario**: la ventana y el cableado de los adaptadores reales.

El reparto permite probar casi todo sin capturadora ni pantalla, y es lo que hace posible llevarla a otras plataformas sin reescribir las reglas. Detalle y reglas: `ARCHITECTURE.md`.

## Plataformas

| Plataforma | Estado | Notas |
|---|---|---|
| **Windows 10/11** | ✅ Hoy | Direct3D 11, Media Foundation, WASAPI, Windows Graphics Capture |
| **Linux** | Objetivo de la 1.0 | Pide reescribir el renderizador y el codificador (hoy atados a Direct3D y Media Foundation); captura por portal en Wayland, PipeWire para audio |
| **iOS** | Fuera de alcance de la 1.0 | El sistema no deja capturar la pantalla fuera de la propia aplicación; sería otro producto (visor y editor) |

## Cómo medimos el éxito

| Qué | Meta |
|---|---|
| Latencia al ver la capturadora | ≤ 2 ms internos, sin empeorar entre versiones |
| Coste de un directo (CPU/GPU) | Menor que el de OBS con la misma escena en el mismo equipo (a medir) |
| Memoria y CPU con la ventana cerrada (bandeja) | < 60 MB y ~1 % con la repetición apagada |
| Tiempo hasta el primer directo de un usuario nuevo | < 5 minutos, con el asistente de primera vez |
| Estabilidad | Sin cierres inesperados en una sesión de 8 horas; recuperar siempre lo grabado |
| Arranque | < 1 segundo hasta ver imagen |

## Hitos

| Versión | Qué trae | Estado |
|---|---|---|
| **0.x** (hoy) | Ver, grabar, repetir, escenas, pantallazos (sin editor), modo Estudio, instalador listo | En desarrollo |
| **0.5** | Capturador completo (editor), pruebas con hardware cerradas, aviso de actualización | Próximo |
| **1.0 para streamers** | Emitir con salud de red, mezclador con supresión de ruido, fuentes de texto/imagen/navegador, plantillas de escenas, cámara virtual, actualizaciones, Linux | Objetivo |
| **Después** | Cuentas conectadas y chat, webcam avanzada, clips y vertical, detección de juego, plugins | Según prioridad |

El detalle y el orden están en `ROADMAP.md` y `STREAMER_PLAN.md`.

## Decisiones tomadas

- **Plataformas:** Windows y Linux antes de la 1.0; iOS después.
- **Atajo global de captura:** Impr Pant, configurable.
- **Modo residente:** dentro de la propia aplicación (icono en la bandeja).
- **OCR y ocultar datos en pantallazos:** segunda versión del capturador.
- **Versiones:** estable y beta, con número de versión del formato de ajustes.
- **Firma de código:** se decide antes del primer lanzamiento público.
- **Instalar:** hasta una etapa final; mientras tanto se desarrolla con `cargo run`.

## Preguntas abiertas

- Público principal (consola o PC), plataformas de directo por orden, chat y alertas propios o de terceros, requisitos exactos de la 1.0 (emisión múltiple, cámara virtual, editor de clips) y modelo de negocio. Están en `STREAMER_PLAN.md`.
- Eslogan, identidad visual y logotipo de Luminifex.

## El renombrado (para el final)

Cuando llegue el momento de pasar de *Dockview* a *Luminifex* hay que decidir, y confirmar antes de tocar nada:
- **Datos del usuario:** hoy los ajustes están en `%APPDATA%\Dockview\settings.json` y los clips en `Vídeos\Dockview`. Lo recomendado es migrar los ajustes solos al primer arranque (sin borrar lo viejo).
- **Alcance:** los crates (`dockview-*`), el ejecutable, el título de la ventana, el instalador, el identificador de la aplicación, los textos y los documentos; y si se incluye o no el programa original en C# de la raíz, que ya no se desarrolla.
- **Fuera del código:** el repositorio de GitHub y la carpeta local, que renombra el autor.
- **Instalador y actualizaciones:** el identificador del instalador no debe cambiar (así una actualización reconoce la instalación anterior).

## Documentos relacionados

`ROADMAP.md` (qué falta y en qué orden) · `STREAMER_PLAN.md` (qué necesita un streamer) · `ARCHITECTURE.md` (cómo está hecha) · `SNIP_PLAN.md` (capturador de pantallazos) · `STUDIO_PLAN.md` (modo Estudio) · `CAPTURE_PLAN.md` (captura y escenas) · `REPLAY_PLAN.md` (repetición instantánea) · `PLUGINS_PLAN.md` (plugins) · `HARDENING_PLAN.md` (robustez) · `UI_PLAN.md` (interfaz).
