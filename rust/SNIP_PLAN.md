# Capturador de pantalla (equivalente a la Herramienta Recortes) — plan (2026-11-14)

## Por qué

Dockview debe funcionar también como capturador de pantallazos y hacer **todo lo que hace la app de Microsoft** (Herramienta Recortes / Snipping Tool de Windows 11). Hoy solo captura vídeo (capturadora, pantalla, ventana); no hay fotos fijas. El `--screenshot` actual es solo un volcado de la propia interfaz para las pruebas, no esta función.

## Qué hace la app de Microsoft (lo que hay que igualar)

| Área | Funciones |
|---|---|
| **Modos de recorte** | Rectángulo, ventana, pantalla completa, forma libre |
| **Retardo** | Sin retardo, 3, 5 y 10 s |
| **Cómo se lanza** | Atajo global (Win+Shift+S, Impr Pant), botón «Nuevo» en la app, icono de la bandeja |
| **Selección** | La pantalla se atenúa, barra flotante con los modos, recuadro con el tamaño, varios monitores y DPI distintos |
| **Después de capturar** | Copia al portapapeles, aviso con vista previa, abrir en el editor, guardar automático en `Imágenes\Capturas de pantalla` |
| **Editor** | Bolígrafo y resaltador (colores y grosor), borrador, regla y transportador, recortar, zoom, deshacer/rehacer, texto sobre la imagen, forma automática al mantener el trazo |
| **Acciones de imagen** | Copiar texto de la imagen (OCR), ocultar datos (redactar), selector de color |
| **Salida** | Guardar como PNG/JPG/GIF, copiar, compartir, abrir con otra app, imprimir |
| **Ajustes** | Copiar al portapapeles automáticamente, preguntar al cerrar sin guardar, incluir el borde de ventana, color del contorno de selección, incluir el cursor, ventanas de editor múltiples |
| **Grabación de pantalla** | Grabar un área con audio. **Ya existe** en Dockview (fuentes + grabación): solo hay que ofrecerlo desde el mismo flujo de selección |

## Diseño

- **La captura reutiliza lo que ya hay.** Windows Graphics Capture ya sirve monitores y ventanas en `dockview-platform`; una foto fija es un fotograma de esa fuente. La ventana se captura aunque esté tapada.
- **Selección con superposición.** Al lanzar, se congela una imagen de todos los monitores y se muestra en ventanas sin borde encima de cada uno, atenuada. Se recorta de la imagen congelada (nunca de la pantalla en vivo), así que lo capturado es exactamente lo que se vio.
- **Residente.** Para el atajo global la app debe seguir viva sin ventana: icono en la bandeja y opción «iniciar con Windows». Es un modo nuevo de ejecución (arranque oculto), con su coste en memoria que hay que medir.
- **Editor propio.** Documento de capas vectoriales sobre la imagen (trazos, formas, texto, recorte) con deshacer/rehacer. Reglas y estado en `dockview-domain` (con tests); dibujo y herramientas en `dockview-ui`; captura, portapapeles y bandeja en `dockview-platform`; cableado en `shell/`. Las guardas de tamaño (400 líneas) y de dependencias siguen vigentes, y `main.rs` solo orquesta.
- **Dónde vive en la interfaz.** Botón «Captura» en la píldora y sección **Capturas** en el panel de Estudio (modo, retardo, carpeta, ajustes). El editor es una ventana propia.

## Multiplataforma

La app será para Windows, Linux e iOS (ver `ARCHITECTURE.md`, regla 2). Para el capturador:
- **Portátil, en `domain`/`app`:** el objetivo (`SnapTarget`), el trabajo (`SnapJob`), el modelo de escritorio y de imagen, el selector (`select.rs`: rectángulo, ventana, pantalla completa, forma libre), recortar con forma, formatos y nombres, y el servicio `snap_service`.
- **Detrás de puertos** (`ScreenGrabber`, `ImageOutput`, `SelectionOverlay`): coger la pantalla, escribir la imagen/portapapeles y mostrar la superposición. Windows: GDI + WIC + ventana Win32. Linux: X11/Wayland (en Wayland captura y selección pasan por el portal `xdg-desktop-portal`) y GTK/Qt o un lienzo propio. **iOS: no aplica** (el sistema no deja capturar la pantalla fuera de la propia app); allí solo tendrían sentido el editor y el OCR sobre una imagen dada.
- Atajo global, bandeja e «iniciar con el sistema» (S-C) son lo más dependiente de la plataforma: se diseñan como puertos desde el principio.

## Cómo se trabaja

Igual que el resto: tarea atómica = un commit + push, con `fmt`, `clippy` sin avisos y `cargo test --workspace` en verde; pausa de revisión al cerrar cada bloque. Verificación con la app real (varios monitores y escalas) y, donde se pueda, capturas píxel a píxel.

## Bloque S-A — Captura fija (sin interfaz nueva)

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-A1 ✅ | `platform`: obtener una imagen fija de un monitor, de todos juntos o de una ventana (con y sin cursor) | Test con fuente simulada; prueba real en 2 monitores |
| S-A2 ✅ | `domain`: modelo de recorte (`Rect` en píxeles virtuales, modos, retardo) y conversión entre monitores con distinto DPI | Tests de bordes, monitores negativos y escalas mixtas |
| S-A3 ✅ | `platform`: portapapeles (imagen) y guardado PNG/JPG/GIF con nombre por fecha | Pegar en Paint da la misma imagen |
| S-A4 ✅ | CLI de prueba: `--snap monitor|window|full FILE.png` | Resultado idéntico a la pantalla |

**Pausa S-A:** comparar con la Herramienta Recortes (misma ventana, misma imagen) y revisar HDR y DPI.

### Resultados del bloque S-A (2026-11-14)

- **S-A1** `platform/snapshot.rs`: capturas fijas con GDI (sin GPU): un rectángulo del escritorio virtual, un monitor, todo, o una ventana (con `PrintWindow`, entera aunque otras la tapen, recortada a su marco visible), con o sin cursor. `desktop()` da los monitores con su posición (puede ser negativa) y su escala.
- **S-A2** `domain/desktop.rs` e `image.rs` (puros, con tests): rectángulos en píxeles del escritorio virtual, qué monitor tiene un punto o la mayor parte de un recorte, recortar a la pantalla, dónde cae un recorte dentro de la imagen de todo el escritorio, escalas mezcladas; `Image` (BGRA) con recorte, canales RGBA y detección de «todo negro» (ventana protegida).
- **S-A3** `platform/imagefile.rs`: guarda **PNG, JPG y GIF** con WIC (los códecs de Windows, sin librerías nuevas; el GIF se reduce a 256 colores) y copia al portapapeles como `CF_DIB`. Nombre como el de Windows: `Screenshot 2026-11-14 163012.png`.
- **S-A4** `--snap full|monitor:N|window:TEXTO|rect:X,Y,W,H` con `--snap-out ARCHIVO` y/o `--snap-copy`, y `--snap-cursor`; saca la captura y sale sin abrir la ventana. El formato sale de la extensión; crea la carpeta; avisa si la imagen sale toda negra.
- **Verificado con la pantalla real** (dos monitores de 1920×1080): `full` = 3840×1080 con el primario a la izquierda, `monitor:0` y `monitor:1` = 1920×1080 en JPG y GIF, un `rect` de 400×300 con cursor y una ventana (1280×800, no negra). Los tres formatos los abre PIL. Los tests de plataforma que necesitan pantalla se saltan si la sesión no tiene.
- **Sin verificar:** el portapapeles con una app real (solo hay test de la construcción del bitmap; el test que lo escribe de verdad está `#[ignore]` porque pisa el portapapeles de quien lo corre); monitores con distinta escala y HDR (esta máquina tiene dos a 100 %); ventanas protegidas (DRM).

**Pausa S-A.**

## Bloque S-B — Selección

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-B1 ✅ | Superposición atenuada sobre todos los monitores con imagen congelada | Captura con 2 monitores y escalas distintas |
| S-B2 ✅ | Rectángulo: arrastrar, mover, redimensionar, tamaño visible, Esc cancela | Tests de la lógica; prueba real |
| S-B3 ✅ | Ventana: resalta la ventana bajo el cursor y la captura entera | Prueba con ventanas solapadas |
| S-B4 ✅ | Pantalla completa y forma libre | Forma libre recorta con transparencia |
| S-B5 ✅ | Barra flotante de modos y retardo (3/5/10 s) con cuenta atrás | Captura con retardo de un menú desplegable |

### La superposición con el aspecto de la app (2026-11-14)

Pedido: que el menú de captura sea igual al de la app (bordes curvos, iconos, animaciones). Con GDI no se podía (sin suavizado, sin iconos Lucide, sin animaciones con el mismo motor), así que la superposición se rehízo:
- **`platform/overlay_window.rs`:** ventana Win32 sin borde que solo recoge ratón y teclas (devuelve `Input`s del dominio); **`overlay_gpu.rs`:** una GPU mínima (swap chain propio y la captura congelada como una textura BGRA, pixel a pixel) con un gancho para dibujar encima.
- **`ui/snip.rs`:** la barra y la selección se dibujan con el mismo motor y estilo que la app: píldora redondeada con sombra y borde, iconos Lucide (recortar, ventana, pantalla completa, lazo), texto Inter, el indicador del modo que se desliza entre botones, la barra que entra deslizándose y apareciendo, el brillo al pasar el ratón, el resaltado de ventana que viaja de una ventana a la siguiente, el atenuado que aparece suave y la pastilla con el tamaño (en píxeles reales). Respeta «Mostrar animaciones» de Windows. 8 tests.
- **`domain/overlay_input.rs`:** qué hace cada pulsación (botón o selección), portátil y con tests.
- **`dockview/capture_overlay.rs`:** el pegamento (ventana + GPU + motor de interfaz + controlador); el overlay GDI se eliminó.
- **Arreglo general descubierto por el camino:** `Ui::panel` pinta el borde como un rectángulo entero y el relleno encima, así que con relleno transparente o translúcido el color del borde tapaba todo. Se añadió `Ui::outline` (solo el marco) y se usa en la selección, en el editor de capas (**la capa elegida se veía rellena de azul sólido y tapaba su contenido: error de E-3c, ya corregido**) y el campo de renombrar usa un relleno opaco.
- **Verificado con la app real y el ratón:** la barra en el monitor del cursor con el aspecto de la app, el indicador pasando a «Ventana», el resaltado de ventana con borde y tamaño, y la captura resultante (1280×800).
- **Pendiente:** las animaciones solo se ven en movimiento (las capturas son fijas); monitores con distinta escala sin probar; los bordes de la selección son de esquinas rectas.

**Pausa S-B.**

### Resultados del bloque S-B (2026-11-14)

- **Portátil (`domain`):** `select.rs` con el selector (rectángulo, ventana, pantalla completa, forma libre; eventos de puntero y teclas → `Step::{Continue, Done, Cancel}` y qué dibujar), `Selection::cut` (recorta la imagen congelada; la forma libre queda transparente fuera), la barra de botones (`toolbar_layout/hit`, a la escala de cada monitor) y `SnapJob` con retardo. Puertos nuevos: `SelectionOverlay` (con `PickOptions { modo, idioma }`), `ScreenGrabber`, `ImageOutput`. El servicio `app::snap_service` hace todo el flujo contra los puertos y se prueba con dobles (sin pantalla).
- **Windows (`platform`):** `overlay.rs` y `overlay_paint.rs`: una ventana sin borde sobre todos los monitores con la pantalla congelada y atenuada, la selección a brillo normal con borde azul y su tamaño, y la barra de modos en el monitor donde esté el cursor (botones «Rectángulo / Ventana / Pantalla completa / Forma libre» en el idioma del sistema). Esc, clic derecho o Esc sin foco (un temporizador lo lee) cancelan.
- **Línea de comandos:** `--snap select[:rect|window|full|free]` abre la superposición; `--snap-delay N` (0–60 s) espera antes de congelar.
- **Verificado con la app real, ratón y teclado:** arrastre de rectángulo (400×298 de 400×300 pedidos: 2 px de diferencia entre dónde apunta el script y dónde cae el evento), ventana por clic (1920×1032 y 1280×800), forma libre (triángulo 301×301: esquina opuesta transparente), Esc sin archivo, la barra (cambiar a «Ventana» y capturar), el retardo de 2 s y la barra en el monitor del cursor.
- **Pendiente:** el retardo se elige hoy solo por línea de comandos (el selector 3/5/10 s y la cuenta atrás visible llegan con la interfaz de S-C/S-E); la superposición no maneja bien monitores con distinta escala (no probado: los de esta máquina están a 100 %); el rectángulo no se puede ajustar tras soltar (se captura al soltar, como Win+Shift+S).

**Pausa S-B.**

## Bloque S-C — Lanzamiento y residencia (D1: Impr Pant; D2: dentro de Dockview)

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-C1 ✅ | `domain`: `HotkeySpec` (leer y escribir «Ctrl+Shift+S», reglas: tecla que escribe necesita modificador, Ctrl+Alt+letra se rechaza por AltGr, las de Dockview están reservadas) y `CaptureSettings` (atajo, guardar, carpeta, formato, copiar, cursor, retardo, modo, quedarse en la bandeja) dentro de `AppSettings`, con `job()` que arma el trabajo de captura. Un atajo mal escrito en el archivo no rompe la carga de los demás ajustes | Tests |
| S-C2 ✅ | `platform`: atajo global configurable (puertos `CaptureHotkeys`/`CaptureTrigger` en el dominio, adaptador de Windows con hilo propio; dejar caer el objeto devuelve la combinación, así que cambiar el ajuste es soltar y registrar de nuevo); aviso si lo ocupa otra app. **Hallazgo con la máquina real:** Windows deja *registrar* Impr Pant pero no entrega la pulsación mientras esté activo el ajuste «Usar el botón Impr Pant para abrir la captura de pantalla» (activado por defecto; la Herramienta Recortes la intercepta y se abre). Por eso `windows_owns_print_screen()` lee ese ajuste del registro y `open_keyboard_settings()` abre la página de Windows donde se desactiva; el aviso al usuario llega con S-C4 | Tests (la combinación ocupada falla y se libera al soltar; el ajuste se lee) |
| S-C3 | `platform`: icono de bandeja con menú (Nuevo recorte: rectángulo / ventana / pantalla completa / forma libre; Abrir Dockview; Salir) y sus eventos | Prueba real |
| S-C4 | Residente: cerrar la ventana la deja en la bandeja (`keepInTray`), arranque oculto (`--tray`), y capturar desde el atajo o la bandeja (guarda en Imágenes\Screenshots y copia) | Mide memoria y CPU en reposo |
| S-C5 | Aviso tras capturar (guardada y copiada; clic abre la carpeta o el editor), con el aspecto de la app | Prueba real |

**Pausa S-C:** revisar el coste en reposo.

## Bloque S-D — Editor

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-D1 | Ventana del editor: lienzo, zoom y desplazamiento | Captura |
| S-D2 | Documento de capas con deshacer/rehacer | Tests de la pila |
| S-D3 | Bolígrafo, resaltador y borrador (color y grosor) | Tests + prueba real con ratón |
| S-D4 | Recortar | Test de la geometría |
| S-D5 | Texto sobre la imagen | Tests de edición |
| S-D6 | Regla y transportador | Test de ángulos |
| S-D7 | Forma automática al mantener el trazo | Tests de reconocimiento |
| S-D8 | Guardar como, copiar, abrir con, imprimir, compartir | Cada salida probada |

**Pausa S-D.**

## Bloque S-E — Acciones de imagen y ajustes

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-E1 | Copiar texto de la imagen (OCR con `Windows.Media.Ocr`) | Prueba en español e inglés |
| S-E2 | Ocultar datos (redactar): detectar correos/teléfonos y tapar de forma irreversible | La imagen guardada no conserva el texto debajo |
| S-E3 | Selector de color | Test del valor leído |
| S-E4 | Sección **Capturas** en el panel de Estudio: ajustes, carpeta, atajo, i18n en/es | Guarda de i18n verde |
| S-E5 | Grabación de pantalla desde el mismo flujo de selección | Selecciona un área y graba con audio |

**Pausa S-E:** revisión final y prueba comparativa completa contra la Herramienta Recortes.

## Decisiones (tomadas el 2026-11-14)

1. **Atajo: Impr Pant, configurable.** Se registra Impr Pant (el usuario desactiva la de Windows en su configuración); si otro programa la tiene, la app avisa y deja elegir otra combinación en Ajustes. Win+Shift+S queda descartada (la reserva Windows).
2. **Residente: dentro de Dockview.** Un solo programa; la ventana del visor se abre y se cierra y el proceso sigue en la bandeja. Se mide el reposo en S-C y solo si pesa se separa en un ejecutable ligero.
3. **Cuándo:** S-C ahora (desbloqueado), luego S-D (editor) y S-E.
4. **OCR y ocultar datos: segunda versión.** La primera versión llega con editor, guardar y copiar.
5. **Plataformas:** Windows y Linux antes de 1.0 (iOS no aplica a este capturador). En Linux, captura y selección pasan por el portal `xdg-desktop-portal` en Wayland.

## Riesgos

1. **DPI mixto y coordenadas negativas** (monitor a la izquierda del principal): la mayor fuente de errores en los recortes; por eso S-A2 va antes que la interfaz.
2. **HDR y monitores a otra profundidad de color:** la imagen congelada puede salir lavada si no se convierte bien.
3. **Memoria en reposo:** un proceso residente con GPU abierta pesa; S-C3 lo mide y puede obligar a crear la GPU solo al capturar.
4. **Ventanas protegidas** (DRM, ventanas elevadas) no se pueden capturar; hay que avisar en vez de guardar una imagen negra.
