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

## 3. Sistema de diseño (parte 0, antes de dibujar nada nuevo)

Valores muestreados de la maqueta (aproximados, se fijan con tests de contraste como los actuales):

| Token | Valor aprox. | Uso |
|---|---|---|
| Fondo de la ventana | `#141414` | hueco entre tarjetas |
| Superficie (tarjeta) | `#181818` – `#202020` | paneles, barra lateral |
| Superficie elevada / seleccionada | `#282828` | fila activa, píldoras |
| Acento principal | `#00F2AA` (relleno de botones `#00C288`) | emisión, interruptores en «on», escena activa |
| Acento secundario | `#00D6E4` | transición, acción de «al aire» |
| Texto | blanco roto; secundario gris medio | |
| Radios | grandes (≈ 16–24 px en tarjetas, píldora completa en botones) | |

A decidir en la parte 0: tema claro equivalente (hoy hay claro y oscuro y se mantienen), tipografía (la maqueta usa una sans geométrica con números tabulares; **Inter** ya está y se conserva salvo que el autor pida otra), iconos (Lucide ya está; la maqueta usa iconos de línea del mismo estilo), movimiento y tamaños mínimos (la maqueta mide 1600 px de ancho; la ventana por defecto es 1280 × 720, así que hay que definir cómo **se comprime** el diseño: tamaño mínimo de ventana y qué se oculta primero).

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
Una franja de unos 34 px (la de `skeleton.png`; las pantallas de ejemplo la dibujan algo más alta) en la parte superior: a la izquierda el **logo**, **LUMINIFEX STUDIO** en mayúsculas con espaciado entre letras y una **etiqueta** redondeada `v0.4.2 · D3D11 / Vulkan`; a la derecha **minimizar, maximizar/restaurar y cerrar**, de línea fina y sin fondo (cerrar se pondrá rojo al pasar el ratón, como en Windows). Sin texto de título aparte.

### Cómo es hoy
La ventana usa la **barra nativa de Windows** (`Window::default_attributes().with_title(…)`, `shell/events.rs`). El plan de interfaz (`UI_PLAN.md`, riesgo 3) ya dejaba los botones propios «para después». Todo lo demás se dibuja con nuestro propio renderizador sobre Direct3D 11 (sin imágenes: formas SDF, texto con atlas de glifos e iconos Lucide como fuente).

### Qué se hace
1. **Marco de la ventana (Windows).** Se quita la barra nativa **pero se conserva el marco de Windows** (sombra, esquinas redondeadas de Windows 11, bordes de redimensionado y el **menú de ajuste de ventanas al pasar el ratón por maximizar**). Se logra extendiendo el área de cliente sobre la barra y respondiendo a los mensajes de la ventana (`WM_NCCALCSIZE`, `WM_NCHITTEST`) desde `crates/dockview/src/os/windows.rs`. Alternativa descartada: ventana sin decoraciones (`with_decorations(false)`) con arrastre y redimensionado a mano; es más sencilla pero pierde el ajuste de ventanas y la sombra nativa.
2. **Portabilidad.** Todo lo de Windows queda detrás de `os/` (regla de `ARCHITECTURE.md`). El dominio solo conoce un **modelo de la barra** (alto, zonas: arrastre, logo, etiqueta, botones; estado maximizada / pantalla completa) y una función pura que dice **qué hay bajo un punto**. `os/other.rs` usará la ventana sin decoraciones y las funciones de arrastre de `winit` (`drag_window`, `drag_resize_window`).
3. **Dibujo.** Un componente nuevo en `dockview-ui` (`titlebar`) construye la barra a partir del modelo: logo, nombre, etiqueta y tres botones con estados (reposo, hover, pulsado; cerrar en rojo). Respeta tema claro/oscuro, español/inglés, `--scale` y los ajustes de animación del sistema.
4. **Comportamiento.** Arrastrar desde la zona libre mueve la ventana; **doble clic** maximiza/restaura; clic derecho abre el menú del sistema; botón cerrar mantiene el comportamiento actual (bandeja si está activada); en **pantalla completa la barra se oculta** y el vídeo vuelve a ocupar todo; al maximizar se compensa el borde invisible de Windows.
5. **La etiqueta.** `v{versión del crate} · D3D11` con datos reales (versión desde `Cargo.toml`, no escrita a mano). «Vulkan» ✂ hasta que el renderizador de Linux lo haga cierto (Fase 5): la etiqueta dirá el renderizador que de verdad se usa.
6. **El nombre y el logo.** Hasta la parte R, el nombre visible sale de **una sola constante** (`APP_NAME`) para que el cambio sea una línea; la parte 1 la deja ya en «Luminifex». El **logo** se dibuja de la forma que permita nuestro renderizador (ver pregunta abierta).

### Tareas (cada una, un commit + push)
- **T-1.1** Modelo de la barra y su prueba de zonas (dominio, puro): tamaños, zonas, qué hay bajo un punto, estados.
- **T-1.2** Textos y la constante de nombre/versión (i18n ES/EN; la etiqueta con datos reales).
- **T-1.3** Dibujo de la barra en `dockview-ui` (sin conectar): logo provisional, nombre, etiqueta, tres botones, hover y pulsado.
- **T-1.4** Acciones: `WindowAction` (minimizar, maximizar/restaurar, cerrar, arrastrar) y su ruteo del puntero en el shell; en el dominio y en `app` con pruebas.
- **T-1.5** Marco de Windows: quitar la barra nativa conservando sombra, esquinas, bordes y ajuste de ventanas (`os/windows.rs`); `os/other.rs` equivalente.
- **T-1.6** Doble clic, menú del sistema, pantalla completa (se oculta), ventana maximizada (compensación del borde) y arrastre.
- **T-1.7** Capturas oscuro/claro × ES/EN × escala 1/2 × normal/maximizada; los textos caben; latencia sin cambios (medida con el patrón de prueba).
- **Pausa de revisión** con la app real: arrastrar, redimensionar por los cuatro bordes y esquinas, maximizar con el menú de Windows 11, minimizar a la barra de tareas, cerrar con y sin bandeja, varias pantallas y escalas distintas.

### Riesgos
- **Marco propio en Windows:** es la parte delicada (mensajes `WM_NCCALCSIZE` y `WM_NCHITTEST`, bordes con escalas distintas, ventana en varios monitores). Se aísla en `os/windows.rs` y se prueba con la app real.
- **Captura de pantalla de la propia ventana** (`exclude_from_capture`) y **superposiciones** no deben verse afectadas; se comprueba.
- **Latencia:** la barra son pocas formas; se mide igualmente (E-1 del modo Estudio).

## 6. Decisiones abiertas

0. **Nombre en la barra:** «LUMINIFEX STUDIO» (esqueleto) o «LUMINIFEX» (pantallas de ejemplo). ¿«Studio» es parte del nombre del producto o solo del modo?
1. **Logo (necesario para la parte 1).** ¿Existe ya? ¿En qué formato (SVG recomendado; PNG si no hay otro)? Si aún no existe, la parte 1 usa uno **provisional** dibujado con formas y se cambia sin tocar nada más. Hace falta también el icono del ejecutable (`.ico`) y de la bandeja para la parte R.
2. **Marco de la ventana:** conservar el marco de Windows (recomendado, ver §5) o ventana sin decoraciones totalmente propia.
3. **Parte R, datos del usuario:** migrar `%APPDATA%\Dockview\` a `%APPDATA%\Luminifex\` en el primer arranque sin borrar lo viejo (recomendado); la carpeta de clips (`Vídeos\Dockview`) y el prefijo de los nombres de archivo.
4. **Parte R, alcance:** solo el proyecto Rust, o también el programa original en C# de la raíz (que ya no se desarrolla).
5. **Parte R, fuera del código:** el repositorio de GitHub y la carpeta local los renombra el autor.
6. **Tamaño mínimo de ventana** y comportamiento por debajo de él (parte 0).

## 7. Reglas de trabajo

Tareas atómicas con commit + push; `fmt`, `clippy` sin avisos y `cargo test --workspace` en verde; nada del sistema operativo fuera de `os/` y `platform`; el modo de desarrollo (`cargo run`) no cambia; **pausa de revisión al cerrar cada parte**; las capturas de cada parte se guardan en `ui-previews/` y se comparan con `ui_compare.py`; cada parte actualiza este documento (estado y lo que se aprendió).
