# Dockview — repetición instantánea y clips

## Objetivo

Poder guardar **lo que acaba de pasar** (desde unos segundos hasta varios minutos) aunque no estuvieras grabando, y convertirlo en un clip listo para redes sociales, horizontal o vertical, con el audio del juego y del micrófono.

Ejemplo del autor: jugando a Pokémon aparece un shiny muy raro sin estar grabando → un atajo guarda los últimos N minutos.

## Decisiones

| Tema | Decisión |
|---|---|
| Activación | **Manual** (interruptor en la interfaz). Nunca se graba sin que el usuario lo active. |
| Duración | **Personalizable**: desde segundos hasta minutos (valores rápidos de 10 s, 30 s, 1, 2, 5 y 10 min, más un control libre). |
| Dónde vive el búfer | **Memoria RAM**; nada se escribe a disco hasta guardar. Se vacía al cerrar la app. |
| Codificación | H.264 por el **codificador de la GPU** (NVENC, Quick Sync o AMD) a través de Media Foundation: sin DLLs ni licencias extra y el `.exe` sigue siendo portable. |
| Imagen | La que se ve en Dockview (lo que entrega la capturadora). |
| Audio | **Pistas separadas**: juego, micrófono y mezcla. El micrófono es opcional y se muestra un indicador mientras escucha. |
| Privacidad | Todo local, sin telemetría ni cuentas. |
| Calidad del búfer | Es el techo del clip: se graba a calidad alta (p. ej. 25–40 Mbps) y se recodifica solo al exportar. |

## Prueba técnica R-0 (hecha, 2026-11-11)

Medida en el PC del autor (GTX 1660 Ti), con el patrón sintético 1080p60:

| Comprobación | Resultado |
|---|---|
| Codificador por hardware disponible | **NVIDIA H.264 Encoder MFT**; NVENC pasó de 1 a 2 sesiones durante la prueba |
| Velocidad | **6,4× tiempo real** (≈380 fps) → mucho margen para 1× en directo |
| Estructura del MP4 | 300/300 muestras, marcas de tiempo correctas |
| Fidelidad | Decodificado el primer frame: error máximo de color **1/255** |
| Coste de CPU | ≈19 % de un núcleo en el límite superior (incluye generar los frames sintéticos y copiarlos). Se reducirá alimentando el codificador desde texturas de la GPU |

**Todavía sin medir:** tamaño y tasa reales con imagen real (el patrón sintético casi no pesa), coste del codificador mientras se juega, y AAC con varias pistas.

## Incógnitas técnicas a resolver primero

1. **Búfer circular de muestras comprimidas.** `SinkWriter` escribe a un archivo; para guardar «los últimos N minutos» hay que codificar con una transformación (`IMFTransform`), guardar las muestras comprimidas en un anillo y, al guardar, volcar desde el **primer fotograma clave** de la ventana. Hay que conservar las cabeceras (SPS/PPS) del codificador y comprobar que el MP4 resultante se reproduce.
2. **Cero copias.** Alimentar el codificador con una textura NV12 de la GPU en lugar de memoria del sistema.
3. **Sincronía** entre vídeo, audio del juego y micrófono (relojes distintos): usar marcas de tiempo comunes y corregir deriva.
4. **Equipos sin codificador por hardware**: el codificador software de Windows no sostiene 1080p60; hará falta un modo reducido (720p30) o desactivarlo.

## Fases

Cada fase es útil por sí sola.

### R-1 · Búfer de repetición y «guardar los últimos N»
- **R-1.1** Codificar con `IMFTransform` y extraer muestras comprimidas y cabeceras (prueba de concepto).
- **R-1.2** Anillo de muestras en RAM acotado por duración y tamaño, con corte en fotogramas clave.
- **R-1.3** Volcar una ventana del anillo a MP4 *sin recodificar*; comprobar que se reproduce.
- **R-1.4** Audio AAC del juego (desde la tubería WASAPI actual) en su pista.
- **R-1.5** Micrófono opcional en su pista, con selección de dispositivo e indicador visible.
- **R-1.6** Interfaz: interruptor, duración personalizable, botón «Guardar» y atajo global; aviso al guardar.
- **R-1.7** Ajustes persistentes, carpeta de destino y estimación de tamaño antes de activar.
- **R-1.8** Estadísticas: estado del búfer, coste del codificador y avisos de espacio.

### R-2 · Recorte y exportación horizontal
- Marcas de entrada/salida, exportación original (sin recodificar) o recodificada en alta calidad.

### R-3 · Exportación vertical simple
- 9:16 centrado y «encajar con fondo desenfocado»; también 4:5 y 1:1. Escalado de calidad en la GPU.

### R-4 · Editor con marco móvil
- Reproducir el clip dentro de la app (decodificación por hardware) y mover con el ratón un marco 9:16 sobre la imagen; la trayectoria se guarda con suavizado y se puede retocar con puntos clave. Vista previa vertical y zoom con la rueda.

### R-5 · Presets y pulido
- Presets para redes sociales (verificando sus requisitos actuales), miniaturas, gestión de espacio, mezcla de audio por pista.

## Nota de posicionamiento

OBS ya ofrece un búfer de repetición. Lo que diferencia a Dockview es la **sencillez** (se abre y funciona), **que sirva a equipos modestos** y, sobre todo, el **editor vertical con marco móvil**.
