# Como Instalar Bootcamp Tracker en Android

Bootcamp Tracker es una aplicacion web (PWA), lo que significa que puedes usarla directamente desde el navegador de tu Android como si fuera una app nativa. No necesitas instalar nada desde la Play Store.

---

## Opcion 1: La Forma Mas Facil (Recomendada)

### Paso 1: Ejecuta la aplicacion en tu computadora

Si tienes el proyecto en tu computadora:

```bash
cd bootcamp-tracker
npm run dev
```

Esto iniciara el servidor. Veras algo como:

```
Bootcamp Tracker server running on http://localhost:3001
VITE v5.x.x  ready in 200 ms
```

### Paso 2: Encuentra tu direccion IP local

- **Windows**: Abre CMD y escribe `ipconfig` - busca "Direccion IPv4"
- **Mac**: Abre Terminal y escribe `ipconfig getifaddr en0`
- **Linux**: Abre Terminal y escribe `hostname -I`

Ejemplo: `192.168.1.100`

### Paso 3: Accede desde tu Android

1. Asegurate de que tu Android y tu computadora esten en la misma red WiFi
2. Abre el navegador en tu Android (Chrome)
3. Escribe: `http://TU_IP:5173`

Ejemplo: `http://192.168.1.100:5173`

### Paso 4: Agregalo a tu pantalla de inicio (opcional)

1. En Chrome Android, toca el menu (tres puntos)
2. Selecciona "Agregar a pantalla de inicio"
3. Listo! Ahora tendras un icono como si fuera una app

---

## Opcion 2: Usando ngrok (Si estas fuera de casa)

Si quieres acceder desde cualquier lugar (no solo tu red local):

### Paso 1: Instala ngrok

```bash
npm install -g ngrok
```

### Paso 2: Expone tu servidor local

```bash
ngrok http 5173
```

### Paso 3: Comparte el enlace

Ngrok te dara un enlace como `https://abc123.ngrok.io`. Compartelo y accede desde tu Android desde cualquier lugar!

---

## Opcion 3: Con Android Studio (Emulador)

Si quieres probar como desarrollador:

### Paso 1: Descarga Android Studio

Ve a: https://developer.android.com/studio

### Paso 2: Crea un emulador

1. Abre Android Studio
2. Ve a "AVD Manager"
3. Crea un nuevo dispositivo virtual (Pixel recommended)
4. Descarga una imagen de sistema (API 34 o superior)

### Paso 3: Ejecuta la app

1. En tu terminal: `cd bootcamp-tracker && npm run dev`
2. En Android Studio, inicia el emulador
3. Abre Chrome en el emulador
4. Ve a `http://localhost:5173`

---

## Solucion de Problemas

### "No se puede acceder a este sitio"

- Verifica que tu Android y PC esten en la SAME WiFi
- Verifica el firewall de Windows (permite Node.js)
- Intenta con `http://192.168.1.X:5173` (sin https)

### "La pagina no carga"

- Verifica que el servidor este corriendo (`npm run dev`)
- Verifica el puerto: el cliente usa 5173, el servidor 3001

### "Error de conexion"

- Asegurate de usar HTTP, no HTTPS
- Verifica tu IP con `ipconfig` (Windows) o `hostname -I` (Linux/Mac)

---

## Notas Importantes

- La aplicacion funciona completamente offline una vez cargada
- Todos los datos se guardan localmente en tu navegador
- Si quieres persistencia, necesitas el backend corriendo
- El backend debe iniciarse primero, luego el frontend

## Estructura de Puertos

| Servicio | Puerto |
|----------|--------|
| Frontend (Vite) | 5173 |
| Backend (Server) | 3001 |

Accede al frontend desde Android: `http://TU_IP:5173`