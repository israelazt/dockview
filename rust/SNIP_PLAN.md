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

## Cómo se trabaja

Igual que el resto: tarea atómica = un commit + push, con `fmt`, `clippy` sin avisos y `cargo test --workspace` en verde; pausa de revisión al cerrar cada bloque. Verificación con la app real (varios monitores y escalas) y, donde se pueda, capturas píxel a píxel.

## Bloque S-A — Captura fija (sin interfaz nueva)

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-A1 ✅ | `platform`: obtener una imagen fija de un monitor, de todos juntos o de una ventana (con y sin cursor) | Test con fuente simulada; prueba real en 2 monitores |
| S-A2 ✅ | `domain`: modelo de recorte (`Rect` en píxeles virtuales, modos, retardo) y conversión entre monitores con distinto DPI | Tests de bordes, monitores negativos y escalas mixtas |
| S-A3 | `platform`: portapapeles (imagen) y guardado PNG/JPG/GIF con nombre por fecha | Pegar en Paint da la misma imagen |
| S-A4 | CLI de prueba: `--snap monitor|window|full FILE.png` | Resultado idéntico a la pantalla |

**Pausa S-A:** comparar con la Herramienta Recortes (misma ventana, misma imagen) y revisar HDR y DPI.

## Bloque S-B — Selección

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-B1 | Superposición atenuada sobre todos los monitores con imagen congelada | Captura con 2 monitores y escalas distintas |
| S-B2 | Rectángulo: arrastrar, mover, redimensionar, tamaño visible, Esc cancela | Tests de la lógica; prueba real |
| S-B3 | Ventana: resalta la ventana bajo el cursor y la captura entera | Prueba con ventanas solapadas |
| S-B4 | Pantalla completa y forma libre | Forma libre recorta con transparencia |
| S-B5 | Barra flotante de modos y retardo (3/5/10 s) con cuenta atrás | Captura con retardo de un menú desplegable |

**Pausa S-B.**

## Bloque S-C — Lanzamiento y residencia

| Tarea | Qué | Listo cuando |
|---|---|---|
| S-C1 | Icono de bandeja con menú (Nuevo, modos, abrir Dockview, salir) | Prueba real |
| S-C2 | Atajo global configurable (Impr Pant y alternativa a Win+Shift+S) | Funciona con la ventana minimizada; aviso si lo ocupa otra app |
| S-C3 | Arranque oculto y «iniciar con Windows» (opcional) | Mide memoria y CPU en reposo |
| S-C4 | Aviso tras capturar (vista previa, clic abre el editor) | Prueba real |

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

## Decisiones abiertas

1. **¿Atajo?** Win+Shift+S y la tecla Impr Pant los usa el propio Windows; `RegisterHotKey` puede fallar con ellos. Hace falta decidir si se reclama Impr Pant (el usuario debe desactivarla en la configuración de Windows) o se usa otra combinación.
2. **¿Ventana propia o parte de Dockview?** El editor y la bandeja obligan a un proceso residente. Alternativa: un segundo ejecutable ligero que comparta las bibliotecas.
3. **¿Cuándo?** Se propone después del modo Estudio (bloques D y E) y antes de retomar `CAPTURE_PLAN.md`; o en paralelo si se prefiere.
4. **Alcance del OCR y la redacción:** son lo más caro; podrían quedar para el final o una segunda versión.

## Riesgos

1. **DPI mixto y coordenadas negativas** (monitor a la izquierda del principal): la mayor fuente de errores en los recortes; por eso S-A2 va antes que la interfaz.
2. **HDR y monitores a otra profundidad de color:** la imagen congelada puede salir lavada si no se convierte bien.
3. **Memoria en reposo:** un proceso residente con GPU abierta pesa; S-C3 lo mide y puede obligar a crear la GPU solo al capturar.
4. **Ventanas protegidas** (DRM, ventanas elevadas) no se pueden capturar; hay que avisar en vez de guardar una imagen negra.
