# Hoja de ruta de Dockview (2026-11-14)

Lo que falta, ordenado por dependencias. Cada plan (`STUDIO_PLAN.md`, `CAPTURE_PLAN.md`, `SNIP_PLAN.md`, `HARDENING_PLAN.md`...) guarda su propio detalle; esto es el mapa entre ellos. Tamaños: **S** = una sesión corta, **M** = un bloque entero, **L** = varios bloques.

## Dónde estamos

| Hecho | Estado |
|---|---|
| Migración a Rust, robustez (8 bloques), modo Estudio (A-E), escenas con editor libre, atajos y corte/morph, capturador de pantallazos **S-A y S-B**, instalador (probado en el CI) | Subido; todo con tests y con la app real **salvo lo que necesita tu hardware** (Fase 0) |

## Fase 0 · Probar con tu hardware (tú; yo preparo y anoto)

Es lo que más riesgo acumula: mucho código verificado solo con tests y capturas. Una sesión de ~1 hora **sin jugar** cierra casi todo. Yo escribo los resultados en los planes.

| # | Prueba | Plan | Tamaño |
|---|---|---|---|
| 0-1 | Revisar el modo Estudio en la app real (clics, escribir un nombre, Esc/Tab) y fijar la nueva línea base (`ui_shots.ps1` + `ui_compare.py`) | STUDIO (pausas C, D, E) | S |
| 0-2 | Desenchufar y reenchufar la UGREEN viendo y grabando; arrancar sin capturadora; colores de las dos tarjetas | HARDENING 2-9, 5 | S |
| 0-3 | Escena con capturadora y un juego abierto: coste de CPU/GPU con compositor y editor, y el morph con clics reales | CAPTURE E-3d | S |
| 0-4 | Grabar con panel abierto y revisar las 3 pistas de audio (hace falta ffmpeg o similar) | STUDIO E-3 | S |
| 0-5 | `tools/measure.ps1` (línea base, ~16 min) | HARDENING 7-1 | S |
| 0-6 | Prueba de una hora grabando (CPU, GPU, RAM, deriva de audio, tamaño) | CAPTURE G-5 | S |
| 0-7 | Un Windows limpio: arranca sin instalar nada y solo escribe en `%APPDATA%\Dockview` | HARDENING 6-9 | S |
| 0-8 | Capturar con monitores a distinta escala (125/150 %) y con HDR, si tienes | SNIP S-A/S-B | S |

## Fase 1 · Decisiones tuyas (bloquean la Fase 2 y la 5)

| # | Decisión | Opciones | Mi recomendación |
|---|---|---|---|
| D1 | Atajo global de captura | Impr Pant (hay que desactivar el de Windows), Win+Shift+S (Windows lo reserva), otra combinación | Impr Pant configurable, con aviso si está ocupado |
| D2 | Proceso residente | Dentro de Dockview, o un ejecutable ligero aparte que comparta las bibliotecas | Dentro de Dockview al principio (un solo programa); separar si pesa en reposo |
| D3 | OCR y ocultar datos | En la primera versión o en una segunda | Segunda versión: son lo más caro |
| D4 | Plataformas antes de 1.0 | Solo Windows, Windows + Linux, o las tres | Windows + Linux; iOS solo si encuentras un sentido (visor/editor) |
| D5 | Firma de código | Comprar certificado, o distribuir sin firmar | Firmar antes del primer lanzamiento público |
| D6 | Política de versiones | Estable y beta, número de formato de ajustes | Dos canales y número de versión en `settings.json` |

## Fase 2 · Capturador de pantallazos (`SNIP_PLAN.md`) — necesita D1, D2

```
S-C  bandeja + atajo global + residente + aviso tras capturar   (M)   ← D1, D2
S-D  editor: lienzo, capas, bolígrafo, resaltador, borrador, recorte,
     texto, regla, forma automática, guardar/copiar/compartir   (L)   ← S-C
S-E  sección «Capturas» en Estudio, selector de retardo con cuenta atrás,
     grabación de pantalla desde el mismo flujo                 (M)   ← S-C
     OCR y ocultar datos                                        (M)   ← D3, S-D
```
Mientras esté sin decidir D1/D2 se puede adelantar **S-D** (el editor no depende del atajo) y el selector de retardo.

## Fase 3 · Pulido que no bloquea nada (cuando haga falta)

| # | Qué | Tamaño |
|---|---|---|
| 3-1 | Aviso de versión nueva (fase 1 de actualizaciones): puerto `UpdateSource`, aviso en la interfaz, opción en Ajustes (← D6) | S |
| 3-2 | Duración y curva del morph configurables; bordes redondeados de la selección | S |
| 3-3 | Mostrar la versión en Ajustes; número de versión del formato de `settings.json` | S |
| 3-4 | Separar datos de desarrollo y de la copia instalada (variable de entorno) | S |
| 3-5 | Exploraciones de rendimiento 7-5 (MJPEG por hardware) y 7-6 (pantallas en la GPU) | M |

## Fase 4 · Funciones opcionales (según tus ganas; ninguna bloquea 1.0)

| # | Qué | Tamaño |
|---|---|---|
| 4-1 | Cámara Canon M50 II por HDMI (spike 2b) | M |
| 4-2 | Streaming (salida RTMP) | L |
| 4-3 | Plugins: cargador y primer plugin (hoy solo hay estructura, `PLUGINS_PLAN.md`) | L |
| 4-4 | Audio de salida con ganancia (aparcado) | S |

## Fase 5 · Multiplataforma (según D4) — lo que más condiciona el diseño

Hoy: solo Windows; el binario se niega a compilar en otro sistema. Hecho: la regla de arquitectura y los puertos neutros del capturador.

```
5-1  Compilar el workspace en Linux: separar lo de Windows del binario y de
     las dependencias (cfg), CI en Linux                                      (M)
5-2  Ventana y renderizador para Linux (winit ya es multiplataforma;
     el renderizador D3D11 habría que reescribirlo o abstraerlo: wgpu/Vulkan)  (L)
5-3  Captura de pantalla y audio en Linux (X11/Wayland con portal, PipeWire)   (L)
5-4  Codificador y grabación en Linux (VAAPI/NVENC/software)                   (L)
5-5  Atajos, bandeja y superposición del capturador en Linux                   (M)
5-6  iOS: decidir qué es Dockview allí (← D4) y, si procede, visor/editor       (L)
```
**Riesgo grande:** el renderizador y el codificador están muy ligados a Direct3D 11 y Media Foundation; 5-2 y 5-4 son reescrituras, no portes. Conviene decidir D4 pronto porque cambia cuánto invertir en abstraer **ahora** (por ejemplo, el renderizador detrás de un puerto) en vez de arrepentirse luego.

## Etapa final (no antes)

| # | Qué |
|---|---|
| F-1 | Instalar de verdad: ya está hecho y probado en el CI; falta la firma (D5) y, con S-C, el inicio con Windows |
| F-2 | Renombrar a **Luminifex** (decidir qué pasa con `%APPDATA%\Dockview` y el alcance del cambio) |
| F-3 | Versión 1.0: congelar el formato de ajustes, notas de versión, primer release público |

## Orden propuesto

1. **Fase 0** en una sesión tuya (desbloquea saber si lo hecho funciona de verdad).
2. **Fase 1**: D1-D6 (media hora de decisiones).
3. **Fase 2** (S-C, luego S-E; S-D puede ir antes si D1/D2 tardan) y en paralelo **3-1** y **3-3**.
4. **Fase 5** empieza con 5-1 en cuanto D4 diga que Linux entra (y, antes, el renderizador detrás de un puerto si es que sí).
5. **Fase 4** a demanda; **etapa final** al cerrar lo anterior.

## Reglas de trabajo (no cambian)

Tareas atómicas, un commit + push cada una, `fmt`, `clippy` sin avisos y `cargo test --workspace` en verde; pausa de revisión al cerrar cada bloque; nada del sistema operativo fuera de `platform` (ver `ARCHITECTURE.md`); el modo de desarrollo (`cargo run`, el `.exe` de `target`, las herramientas) no cambia por ningún paso de esta hoja.
