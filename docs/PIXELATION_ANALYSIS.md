# Pixelado en colores oscuros: encargo de segunda opinión

**Fecha:** 2026-11-27 · **Rama:** `migration/rust` (`923860f`) · **Estado:** sin cambios de código.

**Para quien lo revisa (Opus):** Sonnet analizó el problema y propuso una solución. Queremos
otra perspectiva, no una confirmación. Revisa las pruebas por tu cuenta, busca explicaciones
alternativas y di dónde el razonamiento es débil. No implementes todavía: primero el diagnóstico
y el plan.

**Reglas:** responder en español. El usuario puede estar jugando: no abras la app, no abras la
capturadora (tampoco para listar sus modos) y no hagas pruebas de hardware sin preguntarle.
Compila con pocos procesos. El usuario revisa la app a mano.

## 1. Síntoma

En el Visor Directo, con una Switch 2 (Leyendas Pokémon Z-A y el menú HOME), el usuario ve
pixelado **en todos los colores oscuros**. El ejemplo más claro: arriba y abajo de las letras de
"¿Quieres descansar hasta la noche?", en el cuadro de diálogo azul oscuro.

## 2. Pruebas

Capturas con la tecla C (1920×1080, en `C:\Users\Israel\Pictures\Screenshots\`). Salen del frame
de la capturadora, convertido en CPU (`crates/dockview-domain/src/frame_image.rs`), sin escalado
ni preset de render.

- `Screenshot 2026-11-27 165130.png`: menú HOME.
- `Screenshot 2026-11-27 165513.png`: cuadro de diálogo.

### Estado que muestra la app (captura del panel Estadísticas, 17:00)

| Dato | Valor |
|---|---|
| Dispositivo | UGREEN 25173 (el link guardado en ajustes es `vid_3188&pid_1000`) |
| Modo | NV12 · 1920×1080 · 240 fps |
| Datos del modo | 5972 Mbps |
| Recibidos / mostrados | 113 / 112 fps (en otra captura, 93 mostrados) |
| Saltados | 1,0 /s |
| CPU de la app | 95 % de un núcleo (7,9 % total) |
| Monitor / ventana | 144 Hz / 1280×720 |
| Diagnóstico | "Este modo necesita un puerto USB 3 rápido" |

Ajustes en `%APPDATA%\Dockview\settings.json`: `videoMode: auto`, `renderPreset: 2`,
replay activo (120 s, 40 Mbps).

### Medido en los píxeles

1. **Gris liso del menú HOME:** solo niveles 39, 40 y 41, en un patrón que se repite con periodo
   8×8. Se comparó en dos bloques de 24×12 píxeles, en (800,60) y (1400,200): la fase es
   coherente con una cuadrícula alineada a la imagen.
2. **Cuadro de diálogo, zona sin texto** (x 480–1359, y 944–1007, canal G): salto medio entre
   vecinos horizontales según `x mod 8`, de 0 a 7:
   `0.895 0.461 0.516 0.386 0.578 0.495 0.496 0.692`.
   En vertical (`y mod 8`) el resultado **no es concluyente**:
   `0.788 0.614 0.870 0.708 0.874 0.740 0.846 0.597`.
3. **Halos rectangulares alrededor del texto,** visibles al amplificar la luminancia ×16
   (recorte desde (520,855), 320×100). Sus bordes parecen caer en múltiplos de 8 (x 536, 560,
   584; y 888, 896, 928, 936). **Esas coordenadas se leyeron a ojo** en el recorte ampliado 5×;
   no salen de una medición automática.
4. **Niveles:** en el menú HOME (muestreo 1 de cada 2 píxeles, canal G) hay valores desde 0
   hasta 254. No se ve un corte evidente en las sombras. Esto no demuestra que la matriz y el
   rango sean correctos.

No se revisó: la crominancia por separado, el frame YUV crudo, la evolución entre frames ni una
referencia de la misma escena sin la capturadora. La conclusión sobre el rango tampoco se
contrastó con lo que declara el dispositivo.

### Log (`%APPDATA%\Dockview\dockview.log`)

Varias sesiones repiten esta secuencia (hoy, 16:32):

```
[auto] USB too slow for the uncompressed mode (157 fps): switching to a compressed mode
[auto] compressed mode: 120 fps (was 157 fps)
[auto] no clear improvement: going back to automatic mode
```

Los fps "antes" registrados van de 134 a **195** (hay además entradas de 59–60 fps, sin revisar
con qué dispositivo o modo).

**Contradicción sin resolver:** 1080p NV12 a 195 fps son **4852 Mbps** de datos útiles (164 fps
son 4081). Un enlace USB de 5 Gbps lleva como máximo unos 4000 Mbps tras la codificación 8b/10b.
Entonces ocurre al menos una de estas cosas: el enlace es de 10 Gbps; lo que viaja por USB no es
NV12 sin comprimir; o el contador de fps no mide lo que creemos (cuenta `FrameSlot::published`,
también posibles ráfagas o repetidos). No se ha comprobado ninguna.

## 3. Código relevante

| Qué | Dónde |
|---|---|
| Elección del modo (`select_best`, `Balanced`) | `crates/dockview-domain/src/stream.rs` |
| Apertura de la capturadora y formato de salida | `crates/dockview-platform/src/capture.rs` |
| Opciones de captura según ajustes | `crates/dockview-app/src/core/sources.rs` (`capture_options`) |
| Cambio automático a comprimido | `crates/dockview-app/src/auto_switch.rs`, `core/stats.rs` |
| YUV → RGB en pantalla | `crates/dockview-platform/src/render.hlsl` |
| Selector "Modo de video" (solo Automático / Comprimido) | `crates/dockview-ui/src/options.rs` |

- El perfil por defecto, `Balanced`, filtra los modos 1920×1080 a ≥55 fps, prefiere los no
  comprimidos y toma el de más píxeles × fps. No mira el ancho de banda ni limita los fps.
- `is_compressed` sale del nombre del subtipo nativo (`"MJPEG"`).
- El lector de Media Foundation se crea con `MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING`; la
  salida es NV12 salvo que el modo sea YUY2.

## 4. Lo que concluyó Sonnet (a revisar)

**Hipótesis:** hay compresión por bloques tipo JPEG antes de que la imagen llegue al código de la
app, quizá dentro de la capturadora para poder ofrecer 240 fps anunciando NV12. **No está
probado.** Que la app diga "NV12" no demuestra qué formato viaja por el cable.

**Hipótesis alternativa, anotada al escribir este documento y sin verificar:** Windows (Camera
Frame Server) o Media Foundation podrían estar mostrando como tipo nativo NV12 un flujo que por
USB llega en MJPEG y se decodifica por el camino. Explicaría a la vez los bloques de 8×8 y los
195 fps. Si fuera así, `is_compressed` clasificaría mal el modo y el "automático prefiere sin
comprimir" no se cumpliría. Hay que contrastarlo con documentación y con lo que anuncian los
descriptores USB (por ejemplo con USB Device Tree Viewer, o listando los modos por otra API).

**Propuesta presentada al usuario (sin aprobar ni implementar):**
1. Limitar el automático a 120 fps (1080p120 NV12 ≈ 2986 Mbps). La Switch 2 no envía más de
   120 fps a 1080p (dato no verificado contra una fuente en esta sesión). Si no existe un modo
   dentro del límite, conservar el comportamiento actual.
2. Registrar en el log todos los modos del dispositivo y el elegido.
3. Repetir la captura de la misma escena y comparar; si no cambia, probar 60 fps.
4. Dejar para después un selector manual. Hoy no se puede elegir un modo desde la interfaz, así
   que el paso "probar 60 fps" necesita antes algún mecanismo (opción de línea de órdenes,
   ajuste oculto o el selector).

**Errores de Sonnet durante la sesión:** pidió al usuario elegir 1080p120 y 1080p60 sin haber
comprobado que la interfaz lo permitiera (no lo permite); y al principio planteó el dithering de
la consola o de la capturadora como explicación posible del patrón, que no se ha descartado.
Tampoco se comprobó el modo que usó el cambio "comprimido" que entregó 120 fps.

## 5. Preguntas para la segunda opinión

1. ¿Las mediciones bastan para hablar de bloques tipo JPEG? ¿Qué otra cosa produciría ese patrón
   (dithering de la consola, escalado o dithering de la capturadora, artefactos ya presentes en
   el juego como DLSS, TAA o texturas comprimidas, el monitor)? El usuario lo ve en vivo y la
   foto lo contiene, pero no hay referencia de la fuente.
2. ¿Qué explica los 195 fps frente al límite de USB 5 Gbps?
3. ¿Cómo saber con certeza qué viaja por USB, con el menor esfuerzo y sin depender de nuestra app?
4. ¿Cuál es el experimento más barato que separe las hipótesis? Ten en cuenta que los fotogramas
   de un juego cambian: hace falta una escena estática y comparable (p. ej. el menú HOME).
5. ¿El límite de 120 fps es la política correcta, o conviene decidir por ancho de banda, por fps
   realmente recibidos, por lo que envía la fuente o dejar que elija el usuario? ¿Qué pasa con
   otras capturadoras (la Pacoxi da como máximo 1080p50) y con fuentes de 144 Hz o más?
6. ¿Hay algo en nuestra conversión (matriz, rango, muestreo de crominancia, precisión de 8 bits,
   presets) que empeore la percepción en sombras en vivo, aunque no cree los bloques?
7. Si los bloques vienen del dispositivo y ningún modo los quita, ¿merece la pena un filtro de
   deblocking o de-banding? Coste en latencia y GPU, y efecto en el detalle.

## 6. Entregable

Añade al final de este documento una sección **"Segunda opinión (Opus)"** con: veredicto sobre
cada hipótesis y su nivel de confianza, errores que encuentres en este análisis, el experimento
siguiente paso a paso (indicando qué tiene que hacer el usuario) y el plan recomendado en tareas
pequeñas que Sonnet pueda implementar. Marca qué comprobaste y qué supones.

## Segunda opinión (Opus)

**Fecha:** 2026-11-27. Sin abrir la app ni la capturadora. Reanalicé las dos capturas PNG con
scripts propios (numpy + PIL) y leí `capture.rs`, `mf.rs`, `render.hlsl`, `frame_image.rs` y
`render_plan.rs`. Los scripts están en el scratchpad de la sesión (`a.py`…`k.py`). No son parte
del repositorio.

### Resumen

Hay **dos fenómenos distintos**, y el análisis anterior los mezclaba:

1. **El gris liso del menú HOME no tiene bloques de compresión: es un dithering ordenado.** La
   misma tesela de 8×8 se repite **idéntica bit a bit** en 16 955 de los 32 400 bloques de la
   imagen, es decir, en todo el fondo. Un JPEG no puede producir eso: un bloque liso solo tiene el
   coeficiente DC y se decodifica liso. La tesela (niveles 39/40/41, ±1 código, R=G=B en el
   98,8 % de los píxeles) es la firma de un dithering de periodo 8 aplicado antes, en la consola o
   en el conversor de color o escalador de la capturadora.
2. **Alrededor del texto del diálogo sí hay compresión por bloques DCT de 8×8**, con la rejilla
   alineada al origen del fotograma. Lo medí de forma automática (abajo), así que las
   coordenadas "a ojo" del punto 3 quedan confirmadas.

Nada de esto lo crea la app: la foto (tecla C) sale del frame NV12 tal como lo entrega Media
Foundation, y la app no comprime nada en ese camino. Sí hay algo nuestro que puede hacerlo **más
visible en vivo que en la foto** (ver H6).

### Mediciones nuevas (comprobado)

**M1. La tesela del HOME** (`165130.png`, canal G, bloque en (1400,200)):

```
40 40 40 41 40 39 40 41
40 40 40 40 40 41 41 40
41 40 40 41 41 40 40 41
39 41 41 39 39 40 41 39
41 40 39 41 41 40 40 41
40 41 41 40 40 41 41 40
40 40 40 40 40 40 40 40
40 41 41 40 40 41 41 40
```

Es idéntica en todos los bloques del fondo, a cualquier distancia. Visto con `#` (bloque igual a
la tesela) y `.` (distinto), el mapa reproduce la silueta del menú: los iconos y el texto
cambian los bloques y el fondo es la tesela.

**M2. Halo confinado a bloques de 8×8** (`165513.png`, región x 500–1400, y 850–960). Se cuentan
los píxeles que se apartan del fondo en bloques de 8×8 que **no contienen texto pero lo tienen
a ≤3 px**, desplazando la rejilla de 0 a 7 píxeles:

| Umbral texto / halo | off 0 | off 1 | off 2 | off 3 | off 4 | off 5 | off 6 | off 7 |
|---|---|---|---|---|---|---|---|---|
| +30 / ≥3 | **0,78** | 4,67 | 11,68 | 13,77 | 10,95 | 10,77 | 9,44 | 3,70 |
| +60 / ≥3 | **0,98** | 5,19 | 13,17 | 13,74 | 11,25 | 12,76 | 9,97 | 6,66 |
| +60 / ≥5 | **0,84** | 2,82 | 7,37 | 7,07 | 6,33 | 7,47 | 5,26 | 4,07 |

Con la rejilla en (0,0), los bloques sin texto están casi limpios: hay entre 5 y 17 veces menos
halo que con la rejilla desplazada. El halo no decae con la distancia al texto: tiene un repunte
a 5–6 px y desaparece de golpe a 8 px (fracción 0,000 desde 8 px). Así se comporta el *ringing*
de un códec que transforma bloques de 8×8 de forma independiente, sin filtro de deblocking: JPEG
o MJPEG, o algo propietario del mismo tipo. El TAA, el DLSS o las texturas BCn (bloques de 4×4
en espacio de textura) no quedan confinados a una rejilla de 8 px del fotograma de salida.

**M3. Crominancia (indicio, no prueba):** en Cb, los bloques de 16×8 en (0,0) dan 0,000 y en
(8,0) dan 0,003. Encaja con un MJPEG 4:2:2, que es lo habitual en UVC, pero hay pocos bloques.

**M4. Blockiness global** (|dif.| entre vecinos según `x mod 8`, en toda la imagen): sin pico
claro en ninguna de las dos capturas. Es coherente con una compresión de calidad alta, que
apenas deja escalones en zonas lisas y se nota en los bordes de alto contraste, como el texto.
Por eso la medición 2 del análisis ("salto medio por x mod 8") era débil: mira en el sitio donde
el efecto es menor.

### Veredicto por hipótesis

| # | Hipótesis | Veredicto | Confianza |
|---|---|---|---|
| H1 | Compresión tipo JPEG en algún punto antes de la app | **Sí, para el texto del diálogo** (M2). No explica el gris del HOME | Alta (~85 %) en el "qué"; nula en el "dónde" |
| H2 | Dithering de la consola o de la capturadora | **Sí, explica el gris liso** (M1). Sonnet lo descartó demasiado pronto: no fue un error plantearlo | Alta en que es dithering ordenado; no sé si es de la consola o de la capturadora |
| H3 | Windows (Frame Server o una DMFT del fabricante) anuncia como NV12 lo que viaja comprimido | Plausible y es lo que mejor cuadra con la cifra de 240 fps. Sin verificar | Media |
| H4 | La capturadora comprime por dentro y lo descomprime antes del USB (cable con NV12 real) | Posible si el enlace es de 10 Gbps | Media-baja |
| H5 | Artefactos del juego (TAA, DLSS, texturas) | Improbable para el halo (M2). No se puede descartar como contribución en escenas 3D | Baja |
| H6 | Nuestro renderizado empeora la percepción en vivo | Probable como **amplificador**, no como origen (ver Q6) | Media |
| H7 | El monitor | No se puede evaluar. Un ±1 código en niveles ~40 es casi invisible en un panel normal, así que si se ve claramente, algo lo amplifica (H6 o el modo del monitor) | Baja |

### Errores o puntos débiles del análisis

1. **"Patrón 8×8 con fase coherente" se leyó como prueba de JPEG, y es lo contrario.** Una
   textura idéntica en cada bloque de una zona lisa descarta que el JPEG sea su origen. Señala
   un dithering anterior, que luego puede atravesar o no un compresor.
2. **El presupuesto de USB es más estricto de lo que dice el documento.** Los 4000 Mbps tras el
   8b/10b son el techo bruto. Lo que rinde de verdad UVC en USB 3.2 Gen 1 ronda los
   3200–3600 Mbps (supuesto, de memoria). Así, ya los **157 fps (3906 Mbps)** del log son
   imposibles sin compresión en un enlace de 5 Gbps, no solo los 195. Y el **modo de 240 fps
   (5972 Mbps) supera incluso el techo bruto** de 5 Gbps: si se anuncia como NV12 sin comprimir,
   o el enlace es Gen 2 (10 Gbps) o lo que viaja no es NV12.
3. **Pista que no se aprovechó:** el modo MJPEG que eligió el automático llegaba a 120 fps,
   mientras el "NV12" llega a 240. En una capturadora normal es al revés: el MJPEG permite más
   fps que el modo sin comprimir porque ocupa menos. Que el formato "sin comprimir" ofrezca el
   doble de fps que el comprimido es muy sospechoso.
4. **El contador de fps no demuestra nada sobre el cable.** Con
   `MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING`, si hay un decodificador en medio, cada muestra
   llega ya como un buffer NV12 de tamaño completo. `publish_frame` solo comprueba el tamaño, así
   que cuenta igual un fotograma decodificado que uno sin comprimir. Tampoco distingue los
   repetidos (la capturadora puede rellenar 240 fps con una fuente de 120 Hz).
5. **113 fps recibidos con un modo de 240** se dejó pasar. Cuadra con una fuente de ~120 Hz,
   con o sin pérdidas, y apunta a que el modo de 240 no aporta nada con esta consola.
6. **Comparar en la foto y no en la ventana.** El usuario lo ve en una ventana de 1280×720 con
   `renderPreset: 2` (Calidad = Catmull-Rom). La foto es 1:1, a 1080p y sin escalar, y no
   enseña lo que pasa al reducir la imagen (Q6).
7. La medición de niveles (punto 4) no puede validar el rango: haría falta el YUV crudo y saber
   qué rango declara el dispositivo. El documento ya lo dice; lo confirmo.

### Respuestas a las preguntas

**Q1. ¿Bastan las mediciones?** Las originales no, pero M2 sí basta para afirmar que hay un
códec de bloques de 8×8 en el camino. Lo que el usuario llama "pixelado en todos los oscuros"
seguramente es la suma de las dos cosas: el dithering de ±1 código, que en las zonas lisas
oscuras se ve como un entramado, y el *ringing* en bloques cerca de los bordes. Las dos se notan
más en las sombras por la gamma, porque ahí un código es un salto relativo mayor.

**Q2. 195 fps frente a 5 Gbps.** Hay tres explicaciones, ordenadas de más a menos probable,
todas sin verificar:
(a) Lo que viaja es MJPEG u otro formato comprimido y una DMFT del driver del fabricante (o el
Frame Server) lo publica como tipo nativo NV12. Explica a la vez los fps, M2 y la pista del
punto 3.
(b) El enlace es USB 3.2 Gen 2 (10 Gbps) y el NV12 es real. Entonces los bloques vienen de
dentro de la capturadora y no dependen del modo.
(c) El contador infla la cifra con repetidos o ráfagas. Esto no basta: 157 fps reales ya no
caben en Gen 1.

**Q3. Saber qué viaja por USB sin nuestra app.**
- **USB Device Tree Viewer** (Uwe Sieber, portable) lee los descriptores **sin abrir el flujo de
  vídeo**. Muestra la velocidad del enlace ("SuperSpeed" son 5 Gbps; "SuperSpeedPlus" son
  10 Gbps) y los descriptores UVC: cada `VS_FORMAT_UNCOMPRESSED` (con su GUID NV12/YUY2),
  `VS_FORMAT_MJPEG` o `VS_FORMAT_FRAME_BASED`, con los fps de cada frame descriptor y su
  `dwMaxBitRate`. Si 1080p a 240 solo aparece bajo MJPEG o FRAME_BASED, H3 queda confirmada.
- **Administrador de dispositivos → Detalles del controlador:** si solo aparece
  `usbvideo.sys`, no hay driver del fabricante. Si hay archivos del fabricante (DLL de una
  DMFT), eso refuerza H3. En el registro, la interfaz de cámara con
  `CameraDeviceMftClsidList` o `CameraDeviceMftCLSID` confirma que hay una DMFT. **No lo he
  comprobado.**

**Q4. El experimento más barato.** Está en la sección siguiente. La idea es comparar el mismo
menú HOME (estático) en tres modos y medirlo con las métricas M1 y M2. Como la app no permite
elegir modo, se puede usar **OBS** como selector (fija resolución, fps y formato) y sacar una
captura 1:1 de la fuente. Ojo: OBS usa DirectShow, y no sé si ese camino pasa por la DMFT; si
OBS y nuestra app dan resultados distintos para el mismo modo, eso también es información.

**Q5. La política.** Un límite fijo de 120 fps funciona en este caso, pero acierta por
casualidad. Propongo:
- **Decidir por presupuesto de ancho de banda**, no por fps: descartar del "sin comprimir" todo
  modo cuya tasa supere un presupuesto conservador (~3200 Mbps si no se sabe que el enlace es de
  10 Gbps). Así entran 1080p120 (2986) y 1080p144 en NV12 si cabe, y se excluyen los modos
  físicamente imposibles. Un modo "sin comprimir" que no cabe en el enlace se debe tratar como
  **sospechoso de ir comprimido**, no como el mejor.
- **Entre los modos válidos, preferir el de menos fps que sea ≥ los fps reales de la fuente**,
  y no el máximo. Mientras no se midan los fps reales, usar como techo el menor de 120 y la
  frecuencia del monitor.
- **Selector manual** en Ajustes. Hace falta de todos modos para diagnosticar y para las
  capturadoras raras.
- Pacoxi (máximo 1080p50): `Balanced` exige ≥55 fps a 1080p. Con la política propuesta no
  cambia nada para ella, pero conviene un test que lo fije. Para fuentes de 144 Hz o más, el
  presupuesto decide solo: 1080p144 NV12 ≈ 3583 Mbps queda al límite en Gen 1.

**Q6. Nuestra conversión.** No crea bloques. La matriz BT.709 para 1080p y el rango limitado por
defecto son correctos si el dispositivo no declara nada. La precisión en float del shader basta.
Hay dos cosas que pueden **amplificar** lo que llega:
- **Catmull-Rom al reducir de 1080 a 720 (Calidad):** el kernel tiene lóbulos negativos, que
  realzan los bordes de bloque y el entramado del dithering. Además, al ser un kernel de 4 taps
  a resolución de origen y sin prefiltrado, una reducción de 1,5× produce aliasing: el patrón de
  periodo 8 se convierte en un moiré de unos 5,3 px en pantalla, más grueso y más visible que el
  original. El bilineal también tiene aliasing, pero no realza. **Supuesto razonado, no
  medido.**
- La conversión en CPU de la foto toma la crominancia por vecino más cercano en 2×2. Sale algo
  de bloque de 2×2 en los bordes de color, que no es lo que se ve aquí.
- No hay dithering en la salida a la swapchain de 8 bits. Eso no crea patrones, pero tampoco
  disimula el banding.

**Q7. ¿Deblocking o de-banding?** Solo si al final no existe ningún modo limpio. Un filtro de
deblocking de 8 px con umbral (al estilo del de H.264, solo en bordes de bloque y solo donde la
diferencia es pequeña) en un pase de shader a 1080p cuesta menos de 0,2 ms de GPU (supuesto) y
no añade latencia de fotogramas. Pero difumina el texto pequeño y el HUD, y no arregla el
dithering (el dithering *es* el detalle). Antes que eso, conviene un "suavizado" opcional al
reducir la imagen (bilineal con mipmaps o un prefiltro de caja al escalar a menos de 1×), que
mata el moiré sin tocar la imagen a 1:1.

### Experimento siguiente (paso a paso)

**Lo hace el usuario cuando no esté jugando. Cada paso necesita su permiso.**

1. **Sin abrir el flujo de vídeo:** abrir USB Device Tree Viewer, seleccionar la UGREEN 25173 y
   copiar (a) la velocidad del enlace y (b) la lista de formatos y fps de los descriptores UVC.
   Pegármelo tal cual. *Esto solo ya resuelve H3 frente a H4 y la Q2.*
2. Administrador de dispositivos → la UGREEN → Controlador → Detalles: anotar la lista de
   archivos.
3. **(Necesita abrir la capturadora.)** Con la Switch 2 en el menú HOME, sin tocar nada, sacar
   capturas 1:1 de la misma pantalla en estos modos:
   a. NV12 1080p a 240 (el de ahora, con la tecla C de la app);
   b. NV12 1080p a 120;
   c. NV12 o YUY2 1080p a 60;
   d. MJPEG 1080p a 120.
   Para b, c y d hace falta OBS (Propiedades del dispositivo → resolución/fps personalizados y
   formato de vídeo) o el mecanismo de la tarea P1. Guardar los PNG con el nombre del modo.
4. Con el HOME estático, sacar en el modo a **dos capturas seguidas**: si la tesela M1 cambia
   entre fotogramas, el dithering es temporal (lo típico de un controlador de pantalla); si no,
   es estático.
5. Yo (o Sonnet) paso el script de M1/M2 por cada PNG y comparo.

**Cómo leer el resultado:**
- El halo M2 desaparece en b o c → la compresión va ligada al modo de alta tasa (H3 o un modo
  comprimido de la capturadora). La solución es la política de la Q5.
- El halo sigue en todos los modos → la capturadora comprime siempre por dentro. La solución sería
  un filtro (Q7) o asumir el límite del hardware.
- La tesela M1 aparece igual en todos los modos y en OBS → el dithering viene de la fuente
  (Switch 2) o de la etapa de entrada HDMI de la capturadora. No es nuestro y no hace falta
  quitarlo; a 1:1 es invisible en la práctica.
- Si es posible, una captura del mismo menú en **otra capturadora** (la Pacoxi, a 1080p50) sirve
  para atribuir la tesela: si sale igual, es de la consola.

### Plan recomendado (tareas pequeñas para Sonnet)

No empezar P2 en adelante hasta tener los resultados de los pasos 1 y 3 del experimento.

- **P0 — Herramienta de medida** (`tools/`, Python, fuera del binario): un script que, dado un
  PNG, informe del número de bloques idénticos a la tesela dominante (M1) y del halo por offset
  de rejilla (M2). Sin tests de la app; con un PNG sintético de prueba (JPEG calidad 85 frente a
  un original sin comprimir) para comprobar que distingue los dos casos.
- **P1 — Modo forzado para diagnóstico:** permitir fijar el modo por índice o por
  "formato/ancho/alto/fps" con un ajuste oculto en `settings.json` (`videoModeOverride`). La
  ruta ya existe: `options.mode_index`. Test de dominio que compruebe que, si el override no
  coincide con ningún modo, se usa el automático.
- **P2 — Registrar los modos en el log:** al abrir la capturadora, volcar todos los tipos
  nativos (subtipo, tamaño, fps, Mbps calculados) y el elegido. Volcar también, si se pueden
  leer, `MF_MT_YUV_MATRIX`, `MF_MT_VIDEO_NOMINAL_RANGE` y `MF_MT_COMPRESSED` y
  `MF_MT_ALL_SAMPLES_INDEPENDENT` del tipo nativo.
- **P3 — Contar fotogramas únicos:** en el visor de estadísticas, distinguir los "recibidos" de
  los "únicos" (hash barato de unas filas del plano Y, o comparar timestamps). Es la cifra real
  de la fuente que necesita la política.
- **P4 — Política por presupuesto** en `stream.rs` (`select_best`, `Balanced`): descartar los
  modos sin comprimir cuya tasa supere el presupuesto, y preferir el menor fps ≥ el objetivo
  (por defecto, el menor de 120 y la frecuencia del monitor). Tests: UGREEN (240/120/60 NV12 +
  MJPEG 120) → elige NV12 a 120; Pacoxi (1080p50) → sin cambios; si no hay ningún modo dentro
  del presupuesto, se conserva el comportamiento actual.
- **P5 — Selector manual de modo** en Ajustes (UI + i18n), sobre P1.
- **P6 — Escalado sin aliasing:** cuando el factor de escala es < 1, usar prefiltro o mipmaps en
  Calidad, y ofrecer el bilineal como opción por defecto al reducir. Antes, comprobar con el
  usuario si en Equilibrado (bilineal) se nota menos (es una prueba manual barata: pulsar el
  botón de preset).
- **P7 (opcional, solo si el experimento dice que la capturadora comprime siempre):** pase de
  deblocking de 8 px con umbral, desactivable y apagado por defecto, con medida de coste en GPU.

### Qué comprobé y qué supongo

**Comprobado (en esta sesión):** M1 a M4 sobre los dos PNG; que la foto sale del frame NV12 de
Media Foundation sin pasar por el escalado; que el lector se crea con
`ENABLE_VIDEO_PROCESSING` y que `publish_frame` solo valida el tamaño del buffer; que
`is_compressed` depende solo del GUID `MFVideoFormat_MJPG`; que el preset 2 es Catmull-Rom
sobre la textura sin mipmaps; las cuentas de ancho de banda (1920×1080×1,5×8×fps).

**Supuesto, sin verificar:** el rendimiento práctico de UVC en Gen 1 (~3,2–3,6 Gbps); que la
UGREEN 25173 use una DMFT o un driver propio; que la Switch 2 saque 120 Hz a 1080p; el coste
de un shader de deblocking; el efecto visual del Catmull-Rom al reducir (razonado, no medido);
que OBS por DirectShow vea el mismo flujo que Media Foundation.

### Actualización: descriptores USB leídos (Opus, 2026-11-27)

Los leí sin abrir el flujo de vídeo, con las mismas peticiones al hub que hace UsbTreeView
(`IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2` y `GET_DESCRIPTOR` de la configuración,
desde PowerShell). No descargué ni ejecuté herramientas de terceros. El volcado bruto
(`cfg.bin`, 1522 bytes) y el decodificador (`uvc.py`) están en el scratchpad de la sesión.

**Comprobado:**

- **Enlace:** el puerto 8 del root hub USB 3 (`ROOT_HUB30\5&23f8e3f5`) funciona a SuperSpeed
  (flags `0x3`), es decir, **5 Gbps**. El dispositivo no admite SuperSpeedPlus y su `bcdUSB` es
  3.00. **H4 queda descartada:** no hay 10 Gbps.
- **Driver:** `usbvideo.inf` de Microsoft (10.0.26100.9974), sin driver ni DMFT del fabricante.
  No hay `CameraDeviceMft*` en el registro del dispositivo ni en sus interfaces.
- **Vídeo por bulk** (endpoint 0x83, maxBurst 15). También hay un interfaz HID del fabricante
  (MI_07), seguramente para configuración o firmware.
- **Formatos que anuncia la capturadora:**

| Formato | 1920×1080 | Otros |
|---|---|---|
| YUY2 | 60, 50, 30… | 1440p50; 720p60 |
| **NV12** | **120**, 60, 50… | 2160p30; 1440p60; 720p60 |
| I420 | 120, 60, 50… | igual que NV12 |
| **MJPEG** | **240**, 120, 60, 50… | 2160p60; **1440p144**; 720p60 |
| RGB24 | 60, 50… | 720p60 |
| P010 (10 bits) | 60, 50… | 720p60 |

  Todos los formatos declaran color BT.709 (primarios, transferencia y matriz = 1), y UVC no
  declara el rango.

**Conclusión:** **1080p a 240 fps solo existe en MJPEG.** El "NV12 · 1920×1080 · 240 fps ·
5972 Mbps" del panel lo construye la app a partir del tipo *nativo* que le da Media Foundation
(`format_name`, `fps` y `raw_mbps` de `StreamInfo`). Por tanto, **Windows publica como tipo
nativo NV12 un modo que en el cable es MJPEG**: es el sistema el que lo sintetiza y lo
decodifica, no un driver del fabricante. Esto explica a la vez:

- los bloques de 8×8 alrededor del texto (M2) y el indicio de 4:2:2 en la crominancia (M3);
- los 157–195 fps del log, imposibles en NV12 real a 5 Gbps;
- que `is_compressed` dé `false`, que `raw_mbps` muestre 5972 Mbps y que `select_best` lo
  prefiera como "sin comprimir" con más píxeles × fps.

**H3 queda confirmada en lo esencial** (confianza alta). El único matiz es *quién* lo sintetiza
dentro de Windows (Frame Server o la fuente de captura de MF). Eso no cambia la solución.

**Sigue sin explicar:** la tesela de dithering del HOME (M1). La compresión no la produce y es
anterior a ella. Probablemente viene de la consola o de la entrada HDMI de la capturadora. A 1:1
es ±1 código.

**Qué cambia en el plan:**

- **Existe un modo limpio y real: NV12 1080p120** (2986 Mbps por bulk en Gen 1; cabe, aunque
  justo). Para la Switch 2, que no parece mandar más de ~120 fps (113 recibidos), es el modo
  correcto. El experimento ya no necesita probar 60 fps salvo que el de 120 pierda fotogramas.
- **P4 (política) pasa a ser lo prioritario** y gana una regla concreta: un tipo nativo "sin
  comprimir" cuya `raw_mbps` supere lo que permite el enlace (en Gen 1, más de ~3600 Mbps) se
  trata como **sintetizado desde MJPEG**, igual que `is_compressed`. Además, cuando haya NV12
  real y MJPEG a la misma resolución, se prefiere el NV12 real con el fps más alto que quepa.
- **Falta un dato que solo se obtiene abriendo la capturadora** (se necesita permiso): la lista
  de tipos nativos que ve Media Foundation. Sirve para saber si Windows añade NV12 sintetizados
  *además* de los reales (por ejemplo, dos NV12 1080p120) y si algún atributo los distingue (el
  orden en la lista, `MF_MT_ALL_SAMPLES_INDEPENDENT` o un atributo del Frame Server). La tarea
  P2 (registrar todos los tipos con sus atributos) responde a esto la primera vez que se abra
  la app.
- **P1 (modo forzado)** sigue siendo útil para la prueba visual: comparar 1080p120 NV12 real
  con el "NV12 240" sintetizado en el menú HOME y confirmar que el halo M2 desaparece.

### Actualización: tipos nativos que ve Media Foundation (Opus, 2026-11-27)

Con permiso del usuario, una sonda temporal (ya borrada; no está en el repositorio) abrió la
UGREEN en modo compartido **con Dockview abierto a la vez (PID 17360)** y volcó todos los
atributos de cada tipo nativo, sin leer fotogramas. Salida completa:
`mf_types.txt` en el scratchpad de la sesión (127 tipos, 2472 líneas).

**Comprobado:**

- Del [0] al [60] están los formatos reales de los descriptores: YUY2, NV12 e I420. El
  **NV12 1080p120 real es el [19]** (`10000000/83333`).
- Del [61] al [106], **Windows intercala un "NV12" gemelo delante de cada modo MJPEG**:
  [61] NV12 1080p240 / [62] MJPG 1080p240, [63] NV12 1080p120 / [64] MJPG 1080p120, y así
  hasta 640×480. Hay, por tanto, **dos NV12 1080p120**: el real ([19]) y uno decodificado
  desde MJPEG ([63]). Lo mismo pasa con 4K60 y 1440p144, que solo existen en MJPEG.
- Después vienen RGB24 (`????`, [107]–[118]) y P010 ([119]–[126]).

**Atributos que los distinguen** (el NV12 real frente al decodificado, en todos los pares):

| Atributo | NV12 real [19] | NV12 decodificado [61]/[63] |
|---|---|---|
| `{EA031A62-8BBB-43C5-B5C4-572D2D231C18}` | ausente | **`1`** |
| `MF_MT_SAMPLE_SIZE` | 3 110 400 | ausente |
| `MF_MT_AVG_BITRATE` | 2 985 984 000 | ausente |
| `MF_MT_DEFAULT_STRIDE` | 1920 | ausente |
| `MF_MT_VIDEO_NOMINAL_RANGE` | 2 (16–235) | **1 (0–255)** |
| `MF_MT_VIDEO_CHROMA_SITING` | 6 | 1 |

El GUID `EA031A62-…` solo aparece en los tipos decodificados. Creo que es
`MF_MT_FSSourceTypeDecoded` del `mfapi.h` reciente (lo pone el Frame Server en los tipos que
decodifica). **Este nombre no está verificado:** no viene en el SDK 10.0.19041 instalado ni en
el crate `windows` 0.62.2. El comportamiento, en cambio, sí está comprobado.

**Consecuencias:**

1. **Causa raíz del bloqueo, confirmada:** `select_best` (`Balanced`) ve el [61] como "NV12 sin
   comprimir de 1080p240" y lo prefiere por píxeles × fps. En realidad es MJPEG en el cable,
   decodificado por Windows. Todos los "NV12 240 fps" del panel y del log venían de ahí.
2. **La detección correcta no necesita heurísticas de ancho de banda.** Un tipo es comprimido en
   origen si es `MJPG` **o** si lleva el atributo `EA031A62-…` = 1. Respaldo, por si un driver
   no lo pusiera: sin `MF_MT_SAMPLE_SIZE` en un formato sin comprimir, o `raw_mbps` por encima
   del enlace.
3. **Con esa regla, `Balanced` elegiría el [19]** (NV12 1080p120 real, 2986 Mbps), que cabe en
   Gen 1.
4. **Rango:** los decodificados declaran 0–255, y el código ya lo respeta
   (`color_from_attributes`: 1 → `Full`). El NV12 real declara 16–235. Ninguno de los dos
   necesita cambios en la conversión.
5. **El cambio automático a "comprimido"** pasaba de un MJPEG decodificado a otro MJPEG (de 240
   a 120). Por eso "no clear improvement": nunca llegó a probar el NV12 real.

**Plan revisado (sustituye el orden de P0–P7):**

- **P4' — Clasificar bien** en `mf.rs::read_native_types`: marcar `is_compressed = true` cuando
  el tipo lleve el atributo `EA031A62-…` = 1 (constante definida a mano con un comentario sobre
  su origen). Dejar `format_name` como "NV12", pero mostrar en el panel algo como "NV12 (desde
  MJPEG)". Test de dominio: con una lista que imita esta (real 19, decodificado 61/63, MJPG
  62/64), `Balanced` elige el NV12 real a 120.
- **P2 — Registrar en el log** todos los tipos nativos con la marca de decodificado. Sirve para
  ver otras capturadoras.
- **P1/P5 — Modo forzado y selector manual,** como estaban.
- **Prueba visual con el usuario** tras P4': el mismo menú HOME con la tecla C y la métrica M2.
  Se espera que el halo desaparezca y que la tesela M1 (dithering) siga.
- **P6 (escalado sin aliasing)** y **P7 (deblocking)**: siguen siendo opcionales. P7
  probablemente sobra.

### Verificación tras el arreglo (Opus, 2026-11-27 17:29)

Foto con la tecla C: `Screenshot 2026-11-27 172930.png` (menú HOME). Log: `chosen: NV12 · 1920×1080 · 120fps`.
Se compara con `165130.png` (mismo menú, con el NV12 decodificado desde MJPEG a 240).

**Halo M2** (píxeles desviados por bloque de 8×8 junto al texto, rejilla con offset 0…7):

| Texto | Antes (off 0 → máx.) | Ahora (todos los offsets) |
|---|---|---|
| "Cambiar de usuario" | 0,20 → 9,86 | 0,01–1,11, sin patrón de rejilla |
| "Opciones … Continuar" | 0,10 → 14,14 | 0,02–0,10 |
| Reloj | 0,15 → 16,69 | 0,00–0,19 |

**Los bloques han desaparecido.**

**Niveles (la otra razón de que "se vea mejor que antes"):**

| Punto | Antes | Ahora |
|---|---|---|
| Fondo gris | 40 | 28 |
| Píldora negra "En uso" | 16,16,16 | 0,0,0 |
| Rojo del botón ONLINE | 232,36,28 | 249,24,16 |
| Percentil 0,1 / 99,9 (G) | 10 / 243 | 0 / 255 |

Antes, **los negros estaban levantados a 16 y los colores lavados**. El NV12 decodificado desde
MJPEG declaraba rango 0–255 (`NOMINAL_RANGE = 1`), pero el contenido venía en niveles de
vídeo (16–235). La app se fiaba de la declaración y no estiraba. La cuenta encaja: un gris de 28
en niveles de vídeo es Y = 16 + 28·219/255 ≈ 40, justo el 40 que se veía. El NV12 real declara
16–235 y la app lo estira bien. En las sombras es donde más se nota: vuelve el contraste.

**El dithering sigue** (27/28/29), pero ahora se ve su forma real: periodo 4 en horizontal y
4 en vertical, con filas que se repiten de dos en dos. Viene de la fuente. El JPEG lo
transformaba en la tesela de 8×8 idéntica de M1: un patrón de periodo 4 encaja entero en cada
bloque de 8, así que todos los bloques salían iguales. Eso cierra la incógnita de M1.

**Pendiente derivado:** si se fuerza un modo MJPEG (o su NV12 decodificado), esta capturadora
volverá a verse lavada, porque declara 0–255 y manda 16–235. Opciones: ignorar
`NOMINAL_RANGE` en los tipos decodificados y suponer niveles de vídeo (por defecto en HDMI),
o añadir un ajuste manual de rango (Auto / Limitado / Completo). Para decidirlo hace falta
comprobarlo con otra capturadora en MJPEG, por ejemplo la Pacoxi.
