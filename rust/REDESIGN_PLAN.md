# Luminifex — plan de rediseño

> Estado: **borrador para revisión** (2026-11-14). Se trabaja **parte por parte**: cada parte es un bloque con su diseño, sus tareas atómicas (un commit + push cada una) y una **pausa de revisión** al cerrarla. Nada de la parte N+1 empieza hasta que el autor da por buena la N.

## 1. Punto de partida

**Fuente:** `rust/design/modo-estudio.png` y `rust/design/visor-directo.png` (dos pantallas de una maqueta con el nombre **Luminifex**) y `rust/design/skeleton.png`, el **esqueleto** que se repite en toda la aplicación (ver §2.1). Donde el esqueleto y las pantallas difieren, **manda el esqueleto**.

**Decisiones del autor (2026-11-14):**
1. Es la dirección para **toda** la aplicación, no solo para dos pantallas.
2. El rediseño **incluye el rebrandeo completo** (Dockview → Luminifex): nombre, logo, ejecutable, datos, instalador, textos y documentos.
3. La maqueta es una **guía general**, no una especificación: se **adapta a lo que ya existe**. No se inventan funciones por parecerse a la imagen.

**Qué cambia de la regla anterior:** el renombrado ya no espera al final del proyecto (nota de `LUMINIFEX.md`, «El renombrado»); pasa a ser la parte R de este plan. Las decisiones que esa nota dejaba abiertas (datos del usuario, alcance, identificador del instalador) se cierran en la parte R antes de tocar nada.

## 2. El esqueleto (lo que se repite en toda la app)

`skeleton.png` es la ventana vacía: lo que rodea al contenido en **todas** las pantallas. Tiene cuatro regiones fijas y un área de contenido:

| Región | Qué contiene (según el esqueleto) | Sitio |
|---|---|---|
| **A. Barra de título** | Logo, **LUMINIFEX STUDIO** (mayúsculas con espaciado), etiqueta `v0.4.2 · D3D11 / Vulkan`; a la derecha minimizar, maximizar y cerrar. Unos 34 px | arriba |
| **B. Fila de navegación** | Pestañas en píldora: **Visor Directo · Modo Estudio · Grabaciones · Ajustes** (la activa rellena de verde); a la derecha estado: `NVENC READY │ 0 DROPS │ Offline` y un botón redondo de cuenta | bajo A |
| **C. Barra lateral** | Iconos de línea: cámara, mezclador, pulso, capas; abajo terminal y ayuda. El activo lleva **caja con borde verde y una marca vertical verde a su izquierda** | izquierda, bajo B |
| **D. Franja de atajos** | `Atajos:` + chips de tecla (`Espacio` Transición · `F1-F4` Escenas · `F9` Clip instantáneo · `F10` Grabar) y, a la derecha, `Luminifex Rust Core Engine 0.4.2` con un punto verde | **arriba** del contenido (en las pantallas de ejemplo está abajo; en el esqueleto, arriba) |
| **Contenido** | Fondo casi negro (≈ `#0A0A0B`), más oscuro que el de las tarjetas de las pantallas | resto |

Diferencias del esqueleto frente a las pantallas de ejemplo (el esqueleto manda): el título dice «LUMINIFEX **STUDIO**» (en las pantallas, «LUMINIFEX»), la pestaña dice «Grabaciones» (no «Grabaciones & Clips»), la etiqueta es `D3D11 / Vulkan`, el estado dice `NVENC READY` (no `ACTIVE`) y los atajos van arriba.

Esto reordena las partes: primero se construye el **esqueleto completo** (partes 1 a 4) y después cada pantalla se rellena dentro de él.

## 2.1. Cómo se lee la maqueta

Cada elemento de la maqueta se clasifica así:

| Marca | Significa | Qué se hace |
|---|---|---|
| ✅ | Ya existe en la app | Se rediseña (forma y sitio), no se reescribe |
| 🔧 | Existe a medias | Se completa dentro de la parte que lo muestra |
| ⏳ | No existe (mezclador, cámara virtual, fuente de navegador…) | **No se dibuja vacío.** Se deja el hueco previsto en el diseño y se enciende cuando la función exista (ver `STREAMER_PLAN.md`) |
| ✂ | Se descarta o se cambia | Contenido inventado o que contradice los principios (ver `LUMINIFEX.md`) |

### Inventario de la maqueta

| Elemento de la maqueta | Marca | Notas para adaptarlo |
|---|---|---|
| Barra de título propia: logo, nombre, versión, minimizar / maximizar / cerrar | 🔧 | Hoy es la barra nativa de Windows. **Parte 1** |
| Etiqueta de versión (`v0.4.2 [Direct3D 11 – Vulkan Ready]` / `D3D11 / Vulkan`) | 🔧 | Versión real + renderizador real (D3D11). «Vulkan» ✂ hasta que el renderizador de Linux exista (Fase 5) |
| Pestañas Visor Directo · Modo Estudio · Grabaciones & Clips · Ajustes | 🔧 | Hoy hay dos modos (Ver / Estudio, con Tab) y los ajustes viven en una sección del panel. «Grabaciones & Clips» es una pantalla nueva sobre lo que ya se guarda |
| Estado `NVIDIA NVENC ACTIVE` · `0 DROPS` · `Local / Offline` | 🔧 | Datos reales del codificador y de la emisión; «Local / Offline» pasa a decir si se está emitiendo |
| Avatar de usuario | ✂ | No hay cuentas (principio 3). Se sustituye por el acceso a Ajustes / Acerca de |
| Barra lateral de iconos (cámara, mezclador, pulso, capas, terminal, ayuda) | 🔧 | Atajos a secciones del panel. «Mezclador» ⏳; «terminal» → registro / diagnóstico |
| Franja de estado del compositor (escena activa, FPS, render, VRAM, CPU) | ✅ | Ya medimos FPS, latencia, CPU y memoria. «VRAM» hay que medirla (🔧). «Zero-copy / framebuffer» ✂ (jerga) |
| Escenas con atajo F1–F4 | ✅ | Existen las escenas y su atajo por teclado |
| Capas y fuentes (ojo, candado, reordenar, formato) | 🔧 | Hay visibilidad, orden y editor sobre la imagen; falta candado y el formato en la fila |
| Lienzo con guías, «Snap to Grid», zoom, «Ajustar» | 🔧 | Hay editor de capas sobre la imagen; el snap a cuadrícula y el zoom son nuevos |
| Alerta «¡Nuevo suscriptor!» y fuente «Alertas SVG/Web» | ⏳ | Necesita la fuente de navegador (`STREAMER_PLAN.md`, C-2) |
| Barra de transiciones (Corte, Disolución, Stinger, «Transicionar al aire», Previo/Vivo) | 🔧 | Hay corte y fundido. Stinger y Previo/Vivo ⏳ |
| Control de emisión (botón, bitrate, drops, ping) | ✅ | La emisión ya existe en el núcleo; falta toda la interfaz (A-2, A-5) |
| Grabar multipista (F10), Buffer instantáneo (F9, RAM asignada) | ✅ | Existen; cambia la forma |
| Cámara virtual (interruptor) | ⏳ | `STREAMER_PLAN.md`, D-1 |
| Mezclador de 3 pistas (faders, medidores, M/S, IA Denoise, puerta) | ⏳ | `STREAMER_PLAN.md`, B-1…B-5. Hoy hay volumen, sincronía y medidor del micrófono |
| Pie con atajos rápidos | ✅ | Los atajos existen; se muestran según el contexto |
| Visor: píldoras de dispositivo, latencia, FPS/RAM/CPU/GPU, «100 % autónomo», replay armado | ✅ | Todo se mide ya (excepto el desglose por GPU, 🔧) |
| «Salud del pipeline» (panel flotante con diagnóstico y dos filtros) | ✅ | Es el panel de estadísticas y el diagnóstico en lenguaje llano |
| Barra flotante inferior (Guardar clip, Grabar, Captura, volumen, ocultar HUD, pantalla completa) | ✅ | Es la píldora actual, rediseñada |

## 3. Sistema de diseño (parte 0 — hecha)

**Estado (2026-11-14):** hecha y subida, sin abrir la app. Los valores viven en `dockview-domain` (`theme.rs` para los colores, `design.rs` para las medidas) con pruebas.

**Los grises (oscuro), una sola familia neutra construida sobre `#202020`** (el color de las tarjetas de la maqueta, que el autor eligió como base):

| Escalón | Valor | Uso |
|---|---|---|
| G0 | `#0F0F0F` | detrás de todo |
| G1 | `#141414` | el marco de la ventana: barra de título, fila, barra lateral |
| G2 | `#1A1A1A` | píldoras y chips sobre el marco |
| **G3** | **`#202020`** | **tarjetas y paneles (base de la familia)**; es también la superficie del panel de Estudio y de la píldora |
| G4 | `#282828` | fila seleccionada o elevada |
| G5 | `#303030` | pulsada |

Las líneas y bordes son **blanco al 8–10 %** (no un gris fijo), así sirven sobre cualquier escalón. Una prueba comprueba que todos los grises son neutros (sin tinte azulado) y suben de claridad.

**Acento = el verde de la marca** (`#00E0A4`): es el acento del tema oscuro y rellena la pestaña activa. En el tema claro el acento es un verde más oscuro (`#008F66`, para que se lea sobre blanco) y el relleno de marca `#00C98F` lleva texto oscuro. El azul `#00AAFF` anterior desaparece (deslizadores, marcas de selección y demás pasan al verde).

**Tema claro:** se mantiene, con el marco `#EDEDED`, píldoras `#F7F7F7`, superficie blanca y las mismas líneas (negro al 8–9 %).

**Medidas (`design.rs`):** radios de tarjeta 16, control 10 y chip 8 (las píldoras son redondas del todo); **tamaño mínimo de ventana 960 × 540** (con el marco y el panel de Estudio abiertos aún quedan ≥ 480 × 360 para la imagen; una prueba lo comprueba).

**Tipografía e iconos:** se conservan **Inter** y **Lucide**, que ya están y combinan con la maqueta.

**Pendiente de la parte 0** (con la app real): ajustar a ojo los valores si algún gris no convence, y regenerar la línea base de capturas.

## 4. Las partes

Orden pensado para que cada parte deje la app usable y verificable (capturas píxel a píxel con `ui_shots.ps1`, tema claro y oscuro, inglés y español, `--scale` 1 y 2).

| # | Parte | Qué entrega | Depende de |
|---|---|---|---|
| **0** | Sistema de diseño | Tokens nuevos (oscuro y claro), radios, sombras, tamaños mínimos, reglas de compresión, tabla de iconos | — |
| **1** | **Región A · Barra de título** | Logo + nombre + etiqueta + minimizar / maximizar / cerrar propios, arrastre, doble clic, bordes de redimensionado, ajuste de ventanas de Windows 11 | 0 |
| 2 | Región B · Fila de navegación | Pestañas en píldora y estado (codificador, drops, emisión) con datos reales; el botón de cuenta se convierte en acceso a Ajustes / Acerca de (no hay cuentas); relación con los modos actuales y con `Tab` | 1 |
| 3 | Región C · Barra lateral | Iconos con estado activo (caja + marca verde), atajos a secciones; «mezclador» ⏳ hasta que exista | 2 |
| 4 | Región D · Franja de atajos | Chips de tecla según el contexto, versión del motor; **cierra el esqueleto**: toda la app ya vive dentro de él (el contenido actual se coloca en el área vacía sin rediseñarlo aún) | 3 |
| 5 | Visor directo | Píldoras de métricas, panel de salud, barra flotante inferior (reemplaza la píldora) | 4 |
| 6 | Modo Estudio: armazón | Tres columnas, el vídeo dentro de su tarjeta | 4 |
| 7 | Estudio: escenas y capas | Columna izquierda (candado, formato, reordenar) | 6 |
| 8 | Estudio: lienzo y transiciones | Barra de transición, guías, snap, zoom | 6 |
| 9 | Estudio: control de emisión | Botón «en directo», estado, grabación, buffer; **incluye A-2 y A-5 del plan de streaming** (ajustes y clave) | 6 |
| 10 | Grabaciones | Biblioteca de lo guardado (miniaturas, abrir, borrar) | 4 |
| 11 | Ajustes | Pantalla de ajustes completa (vídeo, audio, calidad, tema, idioma, emisión, capturas) | 4 |
| **R** | **Rebrandeo técnico** | Crates, ejecutable, identificador, `%APPDATA%`, instalador, textos, documentos, migración de datos | decisiones de la sección 6; puede ir justo después de la 1 |
| ⏳ | Piezas que esperan su función | Mezclador, cámara virtual, fuente de navegador, stinger, Previo/Vivo | `STREAMER_PLAN.md` |

**Sobre la parte R.** Conviene en cuanto la parte 1 muestre el nombre nuevo: así no hay pantallas con dos nombres. Se separa de la 1 (que solo dibuja) porque toca el ejecutable, las rutas de datos y el instalador, y eso necesita su propia revisión.

## 5. Parte 1 — Barra de título (diseño)

### Lo que muestra la maqueta
Una franja de unos 34 px (la de `skeleton.png`; las pantallas de ejemplo la dibujan algo más alta) en la parte superior: a la izquierda el **logo**, **LUMINIFEX** en mayúsculas con espaciado entre letras (el esqueleto dice «LUMINIFEX STUDIO»; el autor eligió «LUMINIFEX») y una **etiqueta** redondeada `v0.4.2 · D3D11 / Vulkan`; a la derecha **minimizar, maximizar/restaurar y cerrar**, de línea fina y sin fondo (cerrar se pondrá rojo al pasar el ratón, como en Windows). Sin texto de título aparte.

### Cómo es hoy
La ventana usa la **barra nativa de Windows** (`Window::default_attributes().with_title(…)`, `shell/events.rs`). El plan de interfaz (`UI_PLAN.md`, riesgo 3) ya dejaba los botones propios «para después». Todo lo demás se dibuja con nuestro propio renderizador sobre Direct3D 11 (sin imágenes: formas SDF, texto con atlas de glifos e iconos Lucide como fuente).

### Qué se hace
1. **Marco de la ventana: sin decoraciones (decidido 2026-11-14).** Se quita la barra nativa con `with_decorations(false)` y la ventana es **toda propia**, igual en Windows y en Linux. Se conserva lo que `winit` da sin esfuerzo: sombra (`with_undecorated_shadow(true)` en Windows) y esquinas redondeadas de Windows 11 (`DwmSetWindowAttribute`, desde `os/windows.rs`). Lo que se hace a mano: **arrastrar** (`drag_window`), **redimensionar** desde una banda de unos 6 px en los cuatro lados y las esquinas (`drag_resize_window`, con el cursor de cada zona), el **menú del sistema** con clic derecho en la barra (`show_window_menu`), minimizar, maximizar/restaurar y cerrar. **Se pierde** el menú de ajuste de ventanas de Windows 11 al pasar por maximizar (el ajuste con `Win + flechas` y arrastrar a los bordes de la pantalla siguen funcionando con ventanas sin decoraciones; se comprueba). Alternativa descartada: conservar el marco de Windows con `WM_NCCALCSIZE` / `WM_NCHITTEST`, más fiel pero mucho más delicada.
2. **Portabilidad.** Como la ventana es la misma en todas las plataformas, casi todo es portátil: el dominio tiene el **modelo de la ventana** (alto de la barra, zonas de arrastre / logo / etiqueta / botones, banda de redimensionado, estado maximizada / pantalla completa) y funciones puras que dicen **qué hay bajo un punto** y **qué cursor corresponde**. Solo las esquinas redondeadas y la sombra viven en `os/` (Windows; Linux las hace el compositor).
3. **Dibujo.** Un componente nuevo en `dockview-ui` (`titlebar`) construye la barra a partir del modelo: logo, nombre, etiqueta y tres botones con estados (reposo, hover, pulsado; cerrar en rojo). Respeta tema claro/oscuro, español/inglés, `--scale` y los ajustes de animación del sistema.
4. **Comportamiento.** Arrastrar desde la zona libre mueve la ventana; **doble clic** maximiza/restaura; clic derecho abre el menú del sistema; botón cerrar mantiene el comportamiento actual (bandeja si está activada); en **pantalla completa la barra se oculta** y el vídeo vuelve a ocupar todo; al maximizar se compensa el borde invisible de Windows.
5. **La etiqueta.** `v{versión del crate} · D3D11` con datos reales (versión desde `Cargo.toml`, no escrita a mano). «Vulkan» ✂ hasta que el renderizador de Linux lo haga cierto (Fase 5): la etiqueta dirá el renderizador que de verdad se usa.
6. **El nombre y el logo.** Hasta la parte R, el nombre visible sale de **una sola constante** (`APP_NAME`) para que el cambio sea una línea; la parte 1 la deja ya en «Luminifex». El **logo** se dibuja de la forma que permita nuestro renderizador (ver pregunta abierta).

### Estado (2026-11-14)
Hecho y subido, **sin abrir la app** (el autor está jugando; todo se verificó con pruebas puras y `DOCKVIEW_NO_HARDWARE=1`): T-1.1 a T-1.6 en código (modelo de la ventana, textos y marca, dibujo de la barra, reglas del puntero, ventana sin decoraciones con sombra y esquinas, y el contenido colocado bajo la barra). **Falta la comprobación visual y táctil con la app real** (ver «Pausa de revisión») y T-1.7 (capturas). Las capturas de `ui-previews/` quedan obsoletas: todas llevan ahora la barra, hay que regenerar la línea base.

### Tareas (cada una, un commit + push)
- **T-1.1** Modelo de la barra y su prueba de zonas (dominio, puro): tamaños, zonas, qué hay bajo un punto, estados.
- **T-1.2** Textos y la constante de nombre/versión (i18n ES/EN; la etiqueta con datos reales).
- **T-1.3** Dibujo de la barra en `dockview-ui` (sin conectar): logo provisional, nombre, etiqueta, tres botones, hover y pulsado.
- **T-1.4** Acciones: `WindowAction` (minimizar, maximizar/restaurar, cerrar, arrastrar) y su ruteo del puntero en el shell; en el dominio y en `app` con pruebas.
- **T-1.5** Quitar la barra nativa: `with_decorations(false)`, sombra y esquinas redondeadas (`os/windows.rs`), y la banda de redimensionado con su cursor.
- **T-1.6** Doble clic, menú del sistema, pantalla completa (se oculta), ventana maximizada (sin banda de redimensionado) y arrastre; comprobar `Win + flechas` y el ajuste a los bordes de la pantalla.
- **T-1.7** Capturas oscuro/claro × ES/EN × escala 1/2 × normal/maximizada; los textos caben; latencia sin cambios (medida con el patrón de prueba).
- **Pausa de revisión** con la app real: arrastrar, redimensionar por los cuatro bordes y esquinas, maximizar y restaurar, `Win + flechas`, minimizar a la barra de tareas, cerrar con y sin bandeja, varias pantallas y escalas distintas.

### Riesgos
- **Ventana sin decoraciones:** el redimensionado y el arrastre a mano deben sentirse como los nativos (bandas con escalas distintas, varios monitores, maximizar sin dejar huecos). Se prueba con la app real; lo de Windows se aísla en `os/windows.rs`.
- **Ajuste de ventanas de Windows 11:** el menú al pasar por maximizar no estará; si se echa en falta se puede añadir más adelante como mejora aislada en `os/windows.rs`.
- **Captura de pantalla de la propia ventana** (`exclude_from_capture`) y **superposiciones** no deben verse afectadas; se comprueba.
- **Latencia:** la barra son pocas formas; se mide igualmente (E-1 del modo Estudio).

## 5.2. Parte 2 — Fila de navegación (diseño)

**Qué muestra el esqueleto:** bajo la barra de título, una fila de unos 56 px. A la izquierda, un contenedor en píldora con cuatro pestañas (**Visor Directo · Modo Estudio · Grabaciones · Ajustes**); la activa va rellena de verde con texto oscuro. A la derecha, otro contenedor con el estado: punto verde + `NVENC READY`, separador, `0 DROPS`, separador, punto gris + `Offline`; y un botón redondo de cuenta.

**Cómo se adapta a lo que hay (decidido por mí, revisable):**
- **Pestañas.** Hoy hay dos modos (Ver, que es el panel cerrado, y Estudio, que es el panel abierto) y las secciones *Grabación* y *Ajustes* viven dentro del panel. Hasta que existan las pantallas propias (partes 10 y 11): **Visor Directo** cierra el panel; **Modo Estudio** lo abre; **Grabaciones** lo abre en su sección de grabación; **Ajustes** lo abre en su sección de ajustes. La pestaña activa es Visor si el panel está cerrado (o en pantalla completa) y, si está abierto, la última de las otras tres que se eligió (al arrancar, Estudio); `Tab` y el botón de la píldora lo abren en esa misma pestaña. Las partes 10 y 11 cambian solo a dónde lleva cada pestaña.
- **Estado, con datos reales:**
  - *Codificador:* `NVENC` / `QSV` / `AMF` / `H.264` según el codificador por hardware que Windows da en este PC; `LISTO` si está disponible, `ACTIVO` mientras la repetición o la grabación lo usan, `SIN CODIFICADOR` (en rojo) si no hay. El nombre se averigua **en segundo plano** al arrancar para no retrasar la ventana.
  - *Pérdidas (drops):* imágenes perdidas en esta sesión (las que el visor no llegó a mostrar y las que la emisión tiró).
  - *Red:* `Sin emitir` / `Conectando` / `EN DIRECTO` / `Reconectando` según la emisión.
- **Botón de cuenta:** no hay cuentas; **no se dibuja** hasta que haya algo útil detrás (Acerca de, en una parte posterior).
- **Ventana estrecha:** si no caben las dos mitades, primero desaparecen los datos del estado (de derecha a izquierda) y las pestañas se mantienen.
- **Pantalla completa:** la fila desaparece junto con la barra de título.

**Estructura:** el contenido ahora empieza bajo *barra + fila*. El modelo de la ventana (`window_chrome`) gana una zona `Nav` (el puntero ni llega al contenido ni a la barra), la interfaz dibuja la fila con el mismo `inset` de la parte 1, y el estado sale de un `NavView` en el modelo de vista.

**Estado (2026-11-14):** T-2.1 a T-2.4 hechas y subidas, sin abrir la app (pruebas puras y `DOCKVIEW_NO_HARDWARE=1`). Falta la comprobación visual y táctil con la app real. Cambios respecto al diseño: el botón de cuenta no se dibuja; la pestaña que se recuerda es la última elegida; el estado `Unknown` del codificador no se muestra mientras Windows aún no ha respondido.

**Tareas:** T-2.1 dominio (zona `Nav`, pestañas, `NavView`, textos); T-2.2 dibujo y puntero de la fila (`dockview-ui`, con pruebas de reparto en ventanas estrechas); T-2.3 `Core` y la plataforma (qué codificador hay, pérdidas, red, acciones de pestaña); T-2.4 cableado en el shell (inset, puntero, estado de la pestaña). Pausa de revisión con la app real al cerrar.

## 5.3. Parte 3 — Barra lateral (diseño y estado)

**Esqueleto:** una columna de iconos de línea a la izquierda, bajo la fila de navegación (la fila ocupa todo el ancho): arriba cámara, mezclador, pulso y capas; abajo terminal y ayuda. El icono activo lleva una caja con borde verde y una marca vertical verde en el borde izquierdo.

**Adaptación (revisable):** ancho 52 px. Los cuatro iconos de arriba son **atajos a secciones del panel de Estudio**: cámara → *Fuentes*, mezclador → *Audio* (volumen, sincronía y micrófono; el mezclador de verdad ⏳), pulso → *Estadísticas*, capas → *Escenas*. Pulsarlos abre el panel (pestaña Estudio) en esa sección. El activo es el primero de ellos cuya sección está desplegada (ninguno con el panel cerrado). **Terminal** abre el archivo de registro en el explorador. **Ayuda** no se dibuja (no hay contenido que mostrar; igual que el botón de cuenta). Al dejar el puntero 0,45 s sobre un icono aparece su nombre a la derecha.

**Cuándo se ve (decidido 2026-11-14):** solo con el panel de Estudio abierto (Estudio, Grabaciones y Ajustes). En Visor directo no hay barra y el vídeo recupera sus 52 px; en Visor los atajos a secciones ya los dan los botones de la píldora. Con el panel cerrado o en pantalla completa no se dibuja.

**Estructura:** el modelo de la ventana gana la zona `Rail` y el contenido empieza a su derecha (`Ui::set_inset_left`); con pantalla completa la barra desaparece junto con el resto del marco.

**Estado (2026-11-14):** T-3.1 a T-3.3 hechas y subidas, sin abrir la app. La barra **entra y sale deslizándose** con el mismo ritmo que el panel de Estudio (el contenido y la franja de atajos se reparten el ancho a la vez). Falta la comprobación visual con la app real.

## 5.4. Parte 4 — Franja de atajos (diseño y estado)

**Esqueleto:** una barra con borde y esquinas redondeadas sobre el contenido, a la derecha de la barra lateral: `Atajos:` y chips de tecla con su acción (en la maqueta: Espacio, F1-F4, F9, F10) y, a la derecha, el motor con un punto verde.

**Adaptación:** 30 px de alto (la maqueta tiene ~36), siempre visible salvo en pantalla completa; sin la barra lateral ocupa todo el ancho. Solo se listan **atajos que existen**: `Tab` Estudio, `F` Pantalla completa, el atajo global de guardar clip y el de grabar (los que da la máquina: hoy `Ctrl+Alt+S` y `Ctrl+Alt+R`, y si uno no se pudo registrar no se muestra) y `Ctrl+Alt+F1-F9` Escenas. La maqueta enseñaba Espacio (transición), F9 y F10, que aquí no existen. A la derecha, `Luminifex v{versión}` con un punto que sigue el estado de la imagen (verde en directo, ámbar conectando, rojo con problema, gris en reposo). Sin espacio, primero desaparece la versión y después los últimos atajos. La franja solo se lee: no recibe clics.

**Estado (2026-11-14):** T-4.1 a T-4.3 hechas y subidas, sin abrir la app. **Con esto el esqueleto está completo** (barra de título, fila de navegación, barra lateral, franja de atajos); falta tu revisión con la app real y regenerar la línea base de capturas (T-1.7).

## 6. Decisiones

**Tomadas (2026-11-14):**
- **Nombre en la barra: «LUMINIFEX»** (sin «Studio»; el esqueleto decía «LUMINIFEX STUDIO», se sigue la otra opción). «Studio» queda para el modo.
- **Logo: provisional** dibujado con formas en la parte 1; se sustituye cuando llegue el definitivo (idealmente SVG; el icono `.ico` del ejecutable y de la bandeja hacen falta para la parte R).
- **Marco de la ventana: sin decoraciones**, todo propio (ver §5).

**Abiertas:**
1. **Parte R, datos del usuario:** migrar `%APPDATA%\Dockview\` a `%APPDATA%\Luminifex\` en el primer arranque sin borrar lo viejo (recomendado); la carpeta de clips (`Vídeos\Dockview`) y el prefijo de los nombres de archivo.
2. **Parte R, alcance:** solo el proyecto Rust, o también el programa original en C# de la raíz (que ya no se desarrolla).
3. **Parte R, fuera del código:** el repositorio de GitHub y la carpeta local los renombra el autor.
4. ~~Tamaño mínimo de ventana~~ **decidido en la parte 0: 960 × 540.**

## 7. Reglas de trabajo

Tareas atómicas con commit + push; `fmt`, `clippy` sin avisos y `cargo test --workspace` en verde; nada del sistema operativo fuera de `os/` y `platform`; el modo de desarrollo (`cargo run`) no cambia; **pausa de revisión al cerrar cada parte**; las capturas de cada parte se guardan en `ui-previews/` y se comparan con `ui_compare.py`; cada parte actualiza este documento (estado y lo que se aprendió).
