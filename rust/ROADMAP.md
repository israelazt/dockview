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

## Fase 1 · Decisiones (tomadas el 2026-11-14)

| # | Decisión | Resultado |
|---|---|---|
| D1 | Atajo global de captura | **Impr Pant, configurable**: se registra Impr Pant (el usuario desactiva la de Windows en su configuración); si otro programa la tiene, la app avisa y deja elegir otra combinación en Ajustes |
| D2 | Proceso residente | **Dentro de Dockview**: un solo programa; la ventana del visor se abre y se cierra y el proceso sigue en la bandeja. Se separa en un ejecutable ligero solo si pesa en reposo (se mide en S-C) |
| D3 | OCR y ocultar datos | **Segunda versión** del capturador: primero editor, guardar y copiar |
| D4 | Plataformas antes de 1.0 | **Windows y Linux**; iOS se decide después de 1.0. Consecuencia: la Fase 5 pasa a ser requisito de 1.0 y conviene poner el renderizador y el codificador detrás de puertos pronto |
| D5 | Firma de código | **Decidirlo antes del lanzamiento público**. Referencia de coste (2026, a confirmar): certificado OV ~220-300 $/año, EV ~290-500 $/año, Azure Artifact Signing ~10 $/mes (si eres elegible); alternativas por mirar: Microsoft Store y SignPath |
| D6 | Versiones y actualizaciones | **Estable y beta** (tags `v1.2.0` y `v1.3.0-beta.1`, elegible en Ajustes) y **número de versión del formato** en `settings.json` para migrar sin perder nada |

## Fase 2 · Capturador de pantallazos (`SNIP_PLAN.md`) — D1 y D2 decididas: desbloqueado

```
S-C  bandeja + atajo global (Impr Pant) + residente dentro de Dockview
     + aviso tras capturar                                       (M)   ← D1, D2 ✔
S-D  editor: lienzo, capas, bolígrafo, resaltador, borrador, recorte,
     texto, regla, forma automática, guardar/copiar/compartir   (L)   ← S-C
S-E  sección «Capturas» en Estudio, selector de retardo con cuenta atrás,
     grabación de pantalla desde el mismo flujo                 (M)   ← S-C
     OCR y ocultar datos (segunda versión, D3)                  (M)   ← S-D
```
**Orden:** S-C, luego S-D y S-E. En S-C se mide el reposo del proceso residente para confirmar D2.

## Fase 3 · Pulido que no bloquea nada (cuando haga falta)

| # | Qué | Tamaño |
|---|---|---|
| 3-1 | Aviso de versión nueva (fase 1 de actualizaciones): puerto `UpdateSource`, aviso en la interfaz, opción en Ajustes (← D6) | S |
| 3-2 | Duración y curva del morph configurables; bordes redondeados de la selección | S |
| 3-3 | Mostrar la versión en Ajustes; número de versión del formato de `settings.json` | S |
| 3-4 | Separar datos de desarrollo y de la copia instalada (variable de entorno) | S |
| 3-5 | Exploraciones de rendimiento 7-5 (MJPEG por hardware) y 7-6 (pantallas en la GPU) | M |
| 3-6 | Reposo del modo residente: pausar la captura cuando la ventana está oculta y ni la repetición ni la grabación la necesitan (hoy 54 MB y 1,9 % con la repetición apagada; 112 MB y 6 % con ella) | S |

## Fase 4 · Funciones opcionales (según tus ganas; ninguna bloquea 1.0)

| # | Qué | Tamaño |
|---|---|---|
| 4-1 | Cámara Canon M50 II por HDMI (spike 2b) | M |
| 4-2 | Streaming (salida RTMP) | L |
| 4-3 | Plugins: cargador y primer plugin (hoy solo hay estructura, `PLUGINS_PLAN.md`) | L |
| 4-4 | Audio de salida con ganancia (aparcado) | S |

## Fase 5 · Linux (requisito de 1.0 por D4) — lo que más condiciona el diseño

Hoy: solo Windows; el binario se niega a compilar en otro sistema. Hecho: la regla de arquitectura y los puertos neutros del capturador.

```
5-1  Compilar el workspace en Linux: separar lo de Windows del binario y de
     las dependencias (cfg), CI en Linux                                      (M)
5-2  Ventana y renderizador para Linux (winit ya es multiplataforma;
     el renderizador D3D11 habría que reescribirlo o abstraerlo: wgpu/Vulkan)  (L)
5-3  Captura de pantalla y audio en Linux (X11/Wayland con portal, PipeWire)   (L)
5-4  Codificador y grabación en Linux (VAAPI/NVENC/software)                   (L)
5-5  Atajos, bandeja y superposición del capturador en Linux                   (M)
5-6  iOS: fuera de 1.0 (D4); decidir después qué sería Dockview allí           (—)
```
**Riesgo grande:** el renderizador y el codificador están muy ligados a Direct3D 11 y Media Foundation; 5-2 y 5-4 son reescrituras, no portes. Con D4 (Linux en 1.0) hay que **abstraer ahora**: el renderizador y el codificador detrás de puertos (paso 5-0, ver abajo) antes de seguir añadiendo funciones que dependan de ellos.

```
5-0  Poner el renderizador y el codificador detrás de puertos neutros
     (hoy `Renderer` y la grabación son tipos concretos de Windows)             (M)
```

## Etapa final (no antes)

| # | Qué |
|---|---|
| F-1 | Instalar de verdad: ya está hecho y probado en el CI; falta la firma (D5) y, con S-C, el inicio con Windows |
| F-2 | Renombrar a **Luminifex** (decidir qué pasa con `%APPDATA%\Dockview` y el alcance del cambio) |
| F-3 | Versión 1.0: congelar el formato de ajustes, notas de versión, primer release público |

## Orden propuesto

1. **Fase 0** en una sesión tuya (desbloquea saber si lo hecho funciona de verdad).
2. ~~Fase 1~~ ✔ decidida.
3. **Fase 2** (S-C, luego S-D y S-E) y en paralelo **3-1** y **3-3**.
4. **5-0** (puertos del renderizador y el codificador) entra antes de que crezca más código que dependa de Direct3D; después 5-1 y siguientes.
5. **Fase 4** a demanda; **etapa final** al cerrar lo anterior.

## Reglas de trabajo (no cambian)

Tareas atómicas, un commit + push cada una, `fmt`, `clippy` sin avisos y `cargo test --workspace` en verde; pausa de revisión al cerrar cada bloque; nada del sistema operativo fuera de `platform` (ver `ARCHITECTURE.md`); el modo de desarrollo (`cargo run`, el `.exe` de `target`, las herramientas) no cambia por ningún paso de esta hoja.
